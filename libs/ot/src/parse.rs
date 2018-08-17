// This file is part of OpenFA.
//
// OpenFA is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// OpenFA is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with OpenFA.  If not, see <http://www.gnu.org/licenses/>.
use failure::{err_msg, Fallible};
use num_traits::Num;
use std::{collections::HashMap, marker, str, str::FromStr};

// A resource that can be loaded by an entity.
pub trait Resource {
    fn from_file(filename: &str) -> Fallible<Self>
    where
        Self: marker::Sized;
}

#[derive(Debug, Eq, PartialEq)]
pub enum FieldType {
    Byte,
    Word,
    DWord,
    Ptr,
    Symbol,
}

impl FieldType {
    pub fn from_str(s: &str) -> Fallible<Self> {
        return Ok(match s {
            "byte" => FieldType::Byte,
            "word" => FieldType::Word,
            "dword" => FieldType::DWord,
            "ptr" => FieldType::Ptr,
            "symbol" => FieldType::Symbol,
            _ => bail!("unknown field type {}", s),
        });
    }
}

pub trait GetFieldType {
    fn field_type() -> FieldType;
}

impl GetFieldType for usize {
    fn field_type() -> FieldType {
        panic!("attempted to get field type for pointer field")
    }
}

impl GetFieldType for u8 {
    fn field_type() -> FieldType {
        FieldType::Byte
    }
}

impl GetFieldType for i16 {
    fn field_type() -> FieldType {
        FieldType::Word
    }
}

impl GetFieldType for u16 {
    fn field_type() -> FieldType {
        FieldType::Word
    }
}

impl GetFieldType for i32 {
    fn field_type() -> FieldType {
        FieldType::DWord
    }
}

impl GetFieldType for u32 {
    fn field_type() -> FieldType {
        FieldType::DWord
    }
}

fn partition_line<'a>(line: &'a str) -> Fallible<(FieldType, &'a str, Option<&'a str>)> {
    let mut a = line.splitn(2, ';');
    let mut b = a.next()
        .ok_or_else(|| err_msg("empty line"))?
        .splitn(2, ' ');
    let kind = FieldType::from_str(b.next().ok_or_else(|| err_msg("no kind"))?.trim())?;
    let value = b.next().ok_or_else(|| err_msg("no value"))?.trim();
    let comment = a.next().map(|s| s.trim());
    return Ok((kind, value, comment));
}

pub fn partition_lines<'a>(
    lines: &'a Vec<&'a str>,
) -> Fallible<Vec<(FieldType, &'a str, Option<&'a str>)>> {
    let mut out = Vec::new();
    for line in lines.iter() {
        out.push(partition_line(line)?);
    }
    return Ok(out);
}

pub fn find_pointers<'a>(lines: &Vec<&'a str>) -> Fallible<HashMap<&'a str, Vec<&'a str>>> {
    let mut pointers = HashMap::new();
    let pointer_names = lines
        .iter()
        .filter(|&l| l.starts_with(":"))
        .map(|&l| l)
        .collect::<Vec<&str>>();
    for pointer_name in pointer_names {
        let pointer_data = lines
            .iter()
            .map(|&l| l)
            .skip_while(|&l| l != pointer_name)
            .skip(1)
            .take_while(|&l| !l.starts_with(":") && !l.ends_with("end"))
            .map(|l| l.trim())
            .filter(|l| l.len() != 0)
            .filter(|l| !l.starts_with(";"))
            .collect::<Vec<&str>>();
        pointers.insert(&pointer_name[1..], pointer_data);
    }
    pointers.insert("__empty__", Vec::new());
    return Ok(pointers);
}

pub fn find_section<'a>(lines: &Vec<&'a str>, section_tag: &str) -> Fallible<Vec<&'a str>> {
    let start_pattern = format!("START OF {}", section_tag);
    let end_pattern = format!("END OF {}", section_tag);
    let out = lines
        .iter()
        .skip_while(|&l| l.find(&start_pattern).is_none())
        .take_while(|&l| l.find(&end_pattern).is_none())
        .map(|&l| l.trim())
        .filter(|&l| l.len() != 0 && !l.starts_with(";"))
        .collect::<Vec<&str>>();
    return Ok(out);
}

pub fn follow_pointer<'a>(
    line: &'a str,
    pointers: &'a HashMap<&'a str, Vec<&'a str>>,
) -> Fallible<&'a Vec<&'a str>> {
    let name = ptr(line)?;
    match pointers.get(name) {
        Some(v) => return Ok(v),
        None => bail!("no pointer {} in pointers", name),
    }
}

pub fn maybe_resource_filename<'a>(
    line: &'a str,
    pointers: &'a HashMap<&'a str, Vec<&'a str>>,
) -> Fallible<Option<String>> {
    let maybe_value = ptr(line)
        .ok()
        .and_then(|ptr_name| pointers.get(ptr_name))
        .and_then(|values| values.get(0));
    if let Some(value) = maybe_value {
        return Ok(Some(string(value)?));
    }
    return Ok(None);
}

pub fn maybe_load_resource<'a, T>(
    line: &'a str,
    pointers: &'a HashMap<&'a str, Vec<&'a str>>,
) -> Fallible<Option<T>>
where
    T: Resource,
{
    let maybe_value = ptr(line)
        .ok()
        .and_then(|ptr_name| pointers.get(ptr_name))
        .and_then(|values| values.get(0));
    if let Some(value) = maybe_value {
        let filename = string(value)?;
        let resource = T::from_file(&filename)?;
        return Ok(Some(resource));
    }
    return Ok(None);
}

pub fn hex(n: &str) -> Fallible<u32> {
    ensure!(n.is_ascii(), "non-ascii in number");
    ensure!(n.starts_with("$"), "expected hex to start with $");
    return Ok(u32::from_str_radix(&n[1..], 16)?);
}

pub fn maybe_hex<T>(n: &str) -> Fallible<T>
where
    T: ::num_traits::Num + ::std::str::FromStr,
    <T as ::num_traits::Num>::FromStrRadixErr: 'static + ::std::error::Error + Send + Sync,
    <T as ::std::str::FromStr>::Err: 'static + ::std::error::Error + Send + Sync,
{
    ensure!(n.is_ascii(), "non-ascii in number");
    return Ok(if n.starts_with('$') {
        T::from_str_radix(&n[1..], 16)?
    } else {
        n.parse::<T>()?
    });
}

pub trait TryConvert<T>
where
    Self: Sized,
{
    type Error;
    fn try_from(value: T) -> Result<Self, Self::Error>;
}

impl<T> TryConvert<T> for T {
    type Error = ::failure::Error;
    fn try_from(value: T) -> Fallible<T> {
        Ok(value)
    }
}

impl TryConvert<u8> for bool {
    type Error = ::failure::Error;
    fn try_from(value: u8) -> Fallible<bool> {
        Ok(value != 0)
    }
}

impl TryConvert<u16> for f32 {
    type Error = ::failure::Error;
    fn try_from(value: u16) -> Fallible<f32> {
        Ok(value as f32)
    }
}

impl TryConvert<u16> for usize {
    type Error = ::failure::Error;
    fn try_from(value: u16) -> Fallible<usize> {
        Ok(value as usize)
    }
}

impl TryConvert<u32> for usize {
    type Error = ::failure::Error;
    fn try_from(value: u32) -> Fallible<usize> {
        Ok(value as usize)
    }
}

pub enum Repr {
    Dec,
    Hex,
    Car,
}

#[macro_export]
macro_rules! make_consume_field {
    (
        ($repr:ident : ($ty1:ty, $ty2:ty)),
        $gname:expr,
        $v:ident,
        $pointers:ident,
        $lines:ident[$offset:ident]
    ) => {{
        check_num_type::<$ty1>($offset, $gname, &$lines[$offset])
            .or_else(|_| check_num_type::<$ty2>($offset, $gname, &$lines[$offset]))?;
        let v = match parse_one::<$ty1>($offset, Repr::$repr, &$lines[$offset]) {
            Ok(value) => value as $ty2,
            Err(_) => parse_one::<$ty2>($offset, Repr::$repr, &$lines[$offset])?,
        };
        $offset += 1;
        v
    }};
    (
        ($repr:ident : $parse_ty:ty),
        $gname:expr,
        $v:ident,
        $pointers:ident,
        $lines:ident[$offset:ident]
    ) => {{
        check_num_type::<$parse_ty>($offset, $gname, &$lines[$offset])?;
        let v = parse_one::<$parse_ty>($offset, Repr::$repr, &$lines[$offset])?;
        $offset += 1;
        v
    }};
    (
        ([$repr1:ident, $repr2:ident]: $parse_ty:ty),
        $gname:expr,
        $v:ident,
        $pointers:ident,
        $lines:ident[$offset:ident]
    ) => {{
        check_num_type::<$parse_ty>($offset, $gname, &$lines[$offset])?;
        let v = parse_one::<$parse_ty>($offset, Repr::$repr1, &$lines[$offset])
            .or_else(|_| parse_one::<$parse_ty>($offset, Repr::$repr2, &$lines[$offset]))?;
        $offset += 1;
        v
    }};
    (
        ([$repr1:ident, $repr2:ident, $repr3:ident]: $parse_ty:ty),
        $gname:expr,
        $v:ident,
        $pointers:ident,
        $lines:ident[$offset:ident]
    ) => {{
        check_num_type::<$parse_ty>($offset, $gname, &$lines[$offset])?;
        let v = parse_one::<$parse_ty>($offset, Repr::$repr1, &$lines[$offset])
            .or_else(|_| parse_one::<$parse_ty>($offset, Repr::$repr2, &$lines[$offset]))
            .or_else(|_| parse_one::<$parse_ty>($offset, Repr::$repr3, &$lines[$offset]))?;
        $offset += 1;
        v
    }};
    ([Vec3: $parse_ty:ty], $gname:expr, $v:ident, $pointers:ident, $lines:ident[$offset:ident]) => {{
        for i in 0..3 {
            check_num_type::<$parse_ty>($offset + i, $gname, &$lines[$offset + i])?;
        }
        let x = parse_one::<$parse_ty>($offset + 0, Repr::Dec, &$lines[$offset + 0])?;
        let y = parse_one::<$parse_ty>($offset + 1, Repr::Dec, &$lines[$offset + 1])?;
        let z = parse_one::<$parse_ty>($offset + 2, Repr::Dec, &$lines[$offset + 2])?;
        $offset += 3;
        [x, y, z]
    }};
    (ObjClass, $gname:expr, $v:ident, $pointers:ident, $lines:ident[$offset:ident]) => {{
        let tmp = consume_obj_class(&$lines[$offset])?;
        $offset += 1;
        tmp
    }};
    (Ptr, $gname:expr, $v:ident, $pointers:ident, $lines:ident[$offset:ident]) => {{
        let tmp = consume_ptr($offset, $gname, &$lines[$offset], $pointers)?;
        $offset += 1;
        tmp
    }};
    (Symbol, $gname:expr, $v:ident, $pointers:ident, $lines:ident[$offset:ident]) => {{
        let tmp = $lines[$offset].1;
        $offset += 1;
        tmp
    }};
}

#[macro_export]
macro_rules! make_type_struct {
    ($structname:ident($parent:ident: $parent_ty:ty, version: $version_ty:ident) {
        $( ($name:ident, $store_ty:path, $gname:expr, $parse_ty:tt, $vsup:ident, $default:expr) ),*
    }) => {
        #[allow(dead_code)]
        pub struct $structname {
            $parent: $parent_ty,

            $( $name: $store_ty ),*
        }

        impl $structname {
            pub fn from_lines($parent: $parent_ty, lines: &Vec<&str>, pointers: &HashMap<&str, Vec<&str>>) -> Fallible<Self> {
                let lines = parse::partition_lines(&lines)?;
                println!("LEN: {}", lines.len());
                for (i, l) in lines.iter().enumerate() {
                    println!("{}: {:?}", i, l);
                }
                let mut offset = 0;
                let file_version = $version_ty::from_len(lines.len())?;
                $(
                    let field_version = $version_ty::$vsup;
                    let $name: $store_ty = if field_version <= file_version {
                        println!("AT FIELD: {:?}", lines[offset]);
                        let tmp = make_consume_field!($parse_ty, $gname, version, pointers, lines[offset]);
                        <$store_ty>::try_from(tmp)?
                    } else {
                        $default
                    };
                 );*
                //ensure!(offset == lines.len(), "did not consume all lines");
                return Ok(Self {
                    $parent,
                    $(
                        $name
                    ),*
                });
            }
        }
    }
}

// Note: this has to copy in all cases to return because USNF bakes data in
// inline. We could probably get away with Cow<Vec<Cow<str>>>, but there aren't
// enough instances to bother.
pub fn consume_ptr<'a>(
    offset: usize,
    comment: &'static str,
    actual: &(FieldType, &'a str, Option<&'a str>),
    pointers: &'a HashMap<&'a str, Vec<&'a str>>,
) -> Fallible<Vec<String>> {
    // Normally this will be Ptr. In cases where there is no value to point to,
    // this will instead be set to dword 0.
    if actual.0 == FieldType::DWord {
        ensure!(actual.1 == "0", "dword in pointer with non-null value");
        return Ok(Vec::new());
    }
    // In USNF, some pointer table data was stored inline. These are tagged as
    // byte, even though there are a bunch of bytes here.
    if actual.0 == FieldType::Byte {
        let mut acc = Vec::new();
        for s in actual.1.split(' ') {
            let n = s.parse::<u8>()?;
            acc.push(n as char);
        }
        let sym = acc.drain(..).collect::<String>();
        return Ok(vec![sym]);
    }
    // Otherwise, go look in the pointers table for the given sym name.
    ensure!(
        actual.0 == FieldType::Ptr,
        "expected field type pointer in follow_pointer at line {} ({})",
        offset,
        comment
    );
    let tblref = pointers
        .get(actual.1)
        .ok_or_else(|| err_msg(format!("no pointer named {} in pointers", actual.1)))?;
    let copy = tblref
        .iter()
        .map(|&s| s.to_owned())
        .collect::<Vec<String>>();
    return Ok(copy);
}

// The file provides us with a string giving the type, a value, and possible a line comment.
// In theory we know the type and the field based on the offset. Make sure our expectations
// match the reality.
pub fn check_num_type<T>(
    offset: usize,
    comment: &'static str,
    actual: &(FieldType, &str, Option<&str>),
) -> Fallible<()>
where
    T: Num + FromStr + GetFieldType,
{
    let expect_type = T::field_type();
    ensure!(
        expect_type == actual.0,
        "expected {:?}, but found {:?} at line {} of section, {}",
        expect_type,
        actual.0,
        offset,
        comment
    );
    if let Some(c) = actual.2 {
        if comment.len() > 0 {
            ensure!(
                c.starts_with(comment),
                "expected {}, but found {} at line {} of section",
                comment,
                c,
                offset
            );
        }
    }
    return Ok(());
}

// The file provides us with a string giving the type, a value, and possible a line comment.
// In theory we know the type and the field based on the offset. Make sure our expectations
// match the reality and use that to parse and return the value.
pub fn parse_one<T>(
    offset: usize,
    repr: Repr,
    actual: &(FieldType, &str, Option<&str>),
) -> Fallible<T>
where
    T: Num + FromStr + GetFieldType,
    <T as FromStr>::Err: ::std::error::Error + 'static + Send + Sync,
    <T as ::num_traits::Num>::FromStrRadixErr: ::std::error::Error + 'static + Send + Sync,
{
    return Ok(match repr {
        Repr::Dec => actual.1.parse::<T>()?,
        Repr::Hex => {
            ensure!(
                actual.1.starts_with('$'),
                "expected a hex number at line {} of section, but got {}",
                offset,
                actual.1
            );
            T::from_str_radix(&actual.1[1..], 16)?
        }
        Repr::Car => {
            ensure!(
                actual.1.starts_with('^'),
                "expected a caret number at line {} of section, but got {}",
                offset,
                actual.1
            );
            actual.1[1..].parse::<T>()?
        }
    });
}

// The obj_class field is sometimes written as 32 bits, sign extended. We can drop the top half.
pub fn consume_obj_class(actual: &(FieldType, &str, Option<&str>)) -> Fallible<u16> {
    ensure!(
        actual.0 == FieldType::Word,
        "obj_class should have word type"
    );
    if let Some(c) = actual.2 {
        ensure!(c == "obj_class", "obj_class not where we expected it");
    }
    return Ok(if actual.1.starts_with('$') {
        u32::from_str_radix(&actual.1[1..], 16)? as u16
    } else {
        actual.1.parse::<i32>()? as u32 as u16
    });
}

pub fn byte(line: &str) -> Fallible<u8> {
    let parts = line.split_whitespace().collect::<Vec<&str>>();
    ensure!(parts.len() == 2, "expected 2 parts");
    ensure!(parts[0] == "byte", "expected byte type");
    return Ok(match parts[1].parse::<u8>() {
        Ok(n) => n,
        Err(_) => hex(parts[1])? as u8,
    });
}

pub fn word(line: &str) -> Fallible<i16> {
    let parts = line.split_whitespace().collect::<Vec<&str>>();
    ensure!(parts.len() == 2, "expected 2 parts");
    ensure!(parts[0] == "word", "expected word type");
    return Ok(match parts[1].parse::<i16>() {
        Ok(n) => n,
        Err(_) => hex(parts[1])? as u16 as i16,
    });
}

pub fn dword(line: &str) -> Fallible<u32> {
    let parts = line.split_whitespace().collect::<Vec<&str>>();
    ensure!(parts.len() == 2, "expected 2 parts");
    ensure!(parts[0] == "dword", "expected dword type");
    return Ok(match parts[1].parse::<u32>() {
        Ok(n) => n,
        Err(_) => {
            if parts[1].starts_with("$") {
                hex(parts[1])?
            } else {
                assert!(parts[1].starts_with("^"));
                // FIXME: ^ is meaningful
                //   ^0 => 0
                //   ^250_000 => 64_000_000
                // units of 256?
                parts[1][1..].parse::<u32>()?
            }
        }
    });
}

pub fn string(line: &str) -> Fallible<String> {
    let parts = line.splitn(2, " ").collect::<Vec<&str>>();
    ensure!(parts.len() == 2, "expected 2 parts");
    ensure!(parts[0] == "string", "expected string type");
    ensure!(parts[1].starts_with("\""), "expected string to be quoted");
    ensure!(parts[1].ends_with("\""), "expected string to be quoted");
    let unquoted = parts[1]
        .chars()
        .skip(1)
        .take(parts[1].len() - 2)
        .collect::<String>();
    return Ok(unquoted);
}

pub fn ptr(line: &str) -> Fallible<&str> {
    let parts = line.split_whitespace().collect::<Vec<&str>>();
    ensure!(parts.len() == 2, "expected 2 parts");
    ensure!(parts[0] == "ptr", "expected ptr type");
    return Ok(parts[1]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_byte() {
        assert_eq!(byte("byte 0").unwrap(), 0);
        assert_eq!(byte("byte 255").unwrap(), 255);
        assert!(byte("-1").is_err());
    }

    #[test]
    fn parse_word() {
        assert_eq!(word("word 0").unwrap(), 0);
        assert_eq!(word("word -0").unwrap(), 0);
        assert_eq!(word("word -32768").unwrap(), -32768);
        assert_eq!(word("word 32767").unwrap(), 32767);
        assert_eq!(word("word $0000").unwrap(), 0);
        assert_eq!(word("word $FFFF").unwrap(), -1);
        assert_eq!(word("word $7FFF").unwrap(), 32767);
        assert_eq!(word("word $8000").unwrap(), -32768);
        assert_eq!(word("word $ffff8000").unwrap(), -32768);
        assert!(word("word -32769").is_err());
        assert!(word("word 32768").is_err());
    }

    #[test]
    fn parse_dword() {
        assert_eq!(dword("dword 0").unwrap(), 0);
        assert_eq!(dword("dword $0").unwrap(), 0);
        assert_eq!(dword("dword ^0").unwrap(), 0);
        assert_eq!(dword("dword $FFFFFFFF").unwrap(), u32::max_value());
        assert_eq!(string("string \"\"").unwrap(), "");
        assert_eq!(string("string \"foo\"").unwrap(), "foo");
        assert_eq!(string("string \"foo bar baz\"").unwrap(), "foo bar baz");
        assert_eq!(string("string \"foo\"bar\"baz\"").unwrap(), "foo\"bar\"baz");
        assert!(string("string \"foo").is_err());
        assert!(string("string foo\"").is_err());
        assert!(string("string foo").is_err());
    }
}
