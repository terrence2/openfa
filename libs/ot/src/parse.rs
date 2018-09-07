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
use num_traits::{cast::AsPrimitive, Num};
use std::{collections::HashMap, marker, str, str::FromStr};
pub use std::any::TypeId;

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

    pub fn is_numeric(&self) -> bool {
        match self {
            FieldType::Byte | FieldType::Word | FieldType::DWord => true,
            _ => false,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum FieldNumber {
    Byte(u8),
    Word(u16),
    DWord(u32),
}

impl FieldNumber {
    fn from_kind_and_str(kind: &FieldType, s: &str) -> Fallible<(Repr, Self)> {
        let (repr, v32) = Self::parse_numeric(s)?;
        let num = match kind {
            FieldType::Byte => FieldNumber::Byte(v32 as u8),
            FieldType::Word => FieldNumber::Word(v32 as u16),
            FieldType::DWord => FieldNumber::DWord(v32 as u32),
            _ => bail!("not a number"),
        };
        return Ok((repr, num));
    }

    // Note: some instances are marked as one size, but represented as 32 bits
    // anyway. The assumption appears to be that truncation will happen. At
    // least one of these instances implies sign extension as well.
    fn parse_numeric(vs: &str) -> Fallible<(Repr, u32)> {
        println!("parsing: {}", vs);
        let tpl = if vs.starts_with('$') {
            (Repr::Hex, u32::from_str_radix(&vs[1..], 16)?)
        } else if vs.starts_with('^') {
            (Repr::Car, vs[1..].parse::<u32>()? * 256)
        } else {
            (Repr::Dec, vs.parse::<i32>()? as u32)
        };
        return Ok(tpl);
    }

    pub fn byte(&self) -> Fallible<u8> {
        match self {
            FieldNumber::Byte(b) => Ok(*b),
            _ => bail!("not a byte")
        }
    }

    pub fn word(&self) -> Fallible<u16> {
        match self {
            FieldNumber::Word(w) => Ok(*w),
            _ => bail!("not a word")
        }
    }

    pub fn dword(&self) -> Fallible<u32> {
        match self {
            FieldNumber::DWord(dw) => Ok(*dw),
            _ => bail!("not a dword")
        }
    }
}

#[derive(Debug)]
pub enum FieldValue {
    Numeric((Repr, FieldNumber)),
    Ptr(String, Vec<String>),
    Symbol(String),
}

impl FieldValue {
    fn from_kind_and_str(kind: &FieldType, s: &str, pointers: &HashMap<&str, Vec<&str>>) -> Fallible<Self> {
        let value = match kind {
            FieldType::Byte => FieldValue::Numeric(FieldNumber::from_kind_and_str(kind, s)?),
            FieldType::Word => FieldValue::Numeric(FieldNumber::from_kind_and_str(kind, s)?),
            FieldType::DWord => FieldValue::Numeric(FieldNumber::from_kind_and_str(kind, s)?),
            FieldType::Ptr => {
                let values = pointers[s].iter().map(|&v| v.to_owned()).collect::<Vec<String>>();
                FieldValue::Ptr(s.to_owned(), values) 
            }
            FieldType::Symbol => FieldValue::Symbol(s.to_owned()),
        };
        return Ok(value);
    }

    pub fn numeric(&self) -> Fallible<FieldNumber> {
        match self {
            FieldValue::Numeric((_, num)) => Ok(*num),
            _ => bail!("not a number")
        }
    }

    pub fn pointer(&self) -> Fallible<(String, Vec<String>)> {
        match self {
            FieldValue::Ptr(s, v) => Ok((s.clone(), v.clone())),
            _ => bail!("not a pointer")
        }
    }

    pub fn symbol(&self) -> Fallible<String> {
        match self {
            FieldValue::Symbol(s) => Ok(s.clone()),
            _ => bail!("not a symbol field")
        }
    }

    pub fn is_pointer(&self) -> bool {
        if let FieldValue::Ptr(_, _) = self {
            return true;
        }
        return false;
    }
}

#[derive(Debug)]
pub struct FieldRow {
    kind: FieldType,
    value: FieldValue,
    comment: Option<String>
}

impl FieldRow {
    pub fn from_line(line: &str, pointers: &HashMap<&str, Vec<&str>>) -> Fallible<Self> {
        let mut a = line.splitn(2, ';');
        let mut b = a.next()
            .ok_or_else(|| err_msg("empty line"))?
            .splitn(2, ' ');
        let comment = a.next().map(|s| s.trim().to_owned());
        let kind = FieldType::from_str(
            b.next()
                .ok_or_else(|| err_msg("missing or incorrect field kind"))?
                .trim(),
        )?;
        let value = FieldValue::from_kind_and_str(
            &kind,
            b.next()
                .ok_or_else(|| err_msg("missing or incorrect field value"))?
                .trim(),
            pointers,
        )?;
        return Ok(FieldRow {
            kind,
            value,
            comment
        });
    }

    pub fn value(&self) -> &FieldValue {
        &self.value
    }

    pub fn comment(&self) -> Option<&str> {
        if let Some(ref c) = self.comment {
            return Some(c as &str);
        }
        return None;
    }
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

// pub fn maybe_resource_filename<'a>(
//     line: &'a str,
//     pointers: &'a HashMap<&'a str, Vec<&'a str>>,
// ) -> Fallible<Option<String>> {
//     let maybe_value = ptr(line)
//         .ok()
//         .and_then(|ptr_name| pointers.get(ptr_name))
//         .and_then(|values| values.get(0));
//     if let Some(value) = maybe_value {
//         return Ok(Some(string(value)?));
//     }
//     return Ok(None);
// }

// pub fn maybe_load_resource<'a, T>(
//     line: &'a str,
//     pointers: &'a HashMap<&'a str, Vec<&'a str>>,
// ) -> Fallible<Option<T>>
// where
//     T: Resource,
// {
//     let maybe_value = ptr(line)
//         .ok()
//         .and_then(|ptr_name| pointers.get(ptr_name))
//         .and_then(|values| values.get(0));
//     if let Some(value) = maybe_value {
//         let filename = string(value)?;
//         let resource = T::from_file(&filename)?;
//         return Ok(Some(resource));
//     }
//     return Ok(None);
// }

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

#[derive(Debug)]
pub enum Repr {
    Dec,
    Hex,
    Car,
}

// #[macro_export]
// macro_rules! make_consume_field {
//     (
//         Resource,CpuShape,
//         $comment:expr,
//         $pointers:ident,
//         $resman:ident,
//         $texman:ident,
//         $lines:ident[$offset:ident]
//     ) => {{
//         let names = $crate::parse::consume_ptr($offset, $comment, &$lines[$offset], $pointers)?;
//         let resource_name = $crate::parse::unpack_name(names)?;
//         $resman.load_sh(&resource_name)?
//     }};
//     (
//         ObjClass,
//         $field_type:path,
//         $comment:expr,
//         $pointers:ident,
//         $resman:ident,
//         $texman:ident,
//         $lines:ident[$offset:ident]
//     ) => {{
//         $crate::parse::check_num_type::<u16>($offset, $comment, &$lines[$offset])?;
//         let v =
//             $crate::parse::parse_one::<u16>($offset, $crate::parse::Repr::$repr, &$lines[$offset])?;
//         ObjectKind::new(v)?
//     }};
//     (
//         ($repr:ident : ($ty1:ty, $ty2:ty)),
//         $field_type:path,
//         $comment:expr,
//         $pointers:ident,
//         $resman:ident,
//         $texman:ident,
//         $lines:ident[$offset:ident]
//     ) => {{
//         $crate::parse::check_num_type::<$ty1>($offset, $comment, &$lines[$offset]).or_else(|_| {
//             $crate::parse::check_num_type::<$ty2>($offset, $comment, &$lines[$offset])
//         })?;
//         let v = match $crate::parse::parse_one::<$ty1>(
//             $offset,
//             $crate::parse::Repr::$repr,
//             &$lines[$offset],
//         ) {
//             Ok(value) => value as $ty2,
//             Err(_) => $crate::parse::parse_one::<$ty2>(
//                 $offset,
//                 $crate::parse::Repr::$repr,
//                 &$lines[$offset],
//             )?,
//         };
//         (v, 1)
//     }};
//     (
//         ($repr:ident : $parse_ty:ty),
//         $field_type:path,
//         $comment:expr,
//         $pointers:ident,
//         $resman:ident,
//         $texman:ident,
//         $lines:ident[$offset:ident]
//     ) => {{
//         $crate::parse::check_num_type::<$parse_ty>($offset, $comment, &$lines[$offset])?;
//         let v = $crate::parse::parse_one::<$parse_ty>(
//             $offset,
//             $crate::parse::Repr::$repr,
//             &$lines[$offset],
//         )?;
//         (v, 1)
//     }};
//     (
//         ([$repr1:ident, $repr2:ident]: $parse_ty:ty),
//         $field_type:path,
//         $comment:expr,
//         $pointers:ident,
//         $resman:ident,
//         $texman:ident,
//         $lines:ident[$offset:ident]
//     ) => {{
//         $crate::parse::check_num_type::<$parse_ty>($offset, $comment, &$lines[$offset])?;
//         let v = $crate::parse::parse_one::<$parse_ty>(
//             $offset,
//             $crate::parse::Repr::$repr1,
//             &$lines[$offset],
//         ).or_else(|_| {
//             $crate::parse::parse_one::<$parse_ty>(
//                 $offset,
//                 $crate::parse::Repr::$repr2,
//                 &$lines[$offset],
//             )
//         })?;
//         (v, 1)
//     }};
//     (
//         ([$repr1:ident, $repr2:ident, $repr3:ident]: $parse_ty:ty),
//         $field_type:path,
//         $comment:expr,
//         $pointers:ident,
//         $resman:ident,
//         $texman:ident,
//         $lines:ident[$offset:ident]
//     ) => {{
//         $crate::parse::check_num_type::<$parse_ty>($offset, $comment, &$lines[$offset])?;
//         let v = $crate::parse::parse_one::<$parse_ty>(
//             $offset,
//             $crate::parse::Repr::$repr1,
//             &$lines[$offset],
//         ).or_else(|_| {
//             $crate::parse::parse_one::<$parse_ty>(
//                 $offset,
//                 $crate::parse::Repr::$repr2,
//                 &$lines[$offset],
//             )
//         })
//             .or_else(|_| {
//                 $crate::parse::parse_one::<$parse_ty>(
//                     $offset,
//                     $crate::parse::Repr::$repr3,
//                     &$lines[$offset],
//                 )
//             })?;
//         (v, 1)
//     }};
//     (
//         Altitude,
//         $field_type:path,
//         $comment:expr,
//         $pointers:ident,
//         $resman:ident,
//         $texman:ident,
//         $lines:ident[$offset:ident]
//     ) => {{
//         // Every combination of repr and sign is used here, including frankly insane
//         // ones like hex signed 32 bit values.
//         $crate::parse::check_num_type::<u32>($offset, $comment, &$lines[$offset])?;
//         let v = match $crate::parse::parse_one::<u32>(
//             $offset,
//             $crate::parse::Repr::Car,
//             &$lines[$offset],
//         ) {
//             Ok(v) => Ok(v as i32),
//             Err(_) => match $crate::parse::parse_one::<u32>(
//                 $offset,
//                 $crate::parse::Repr::Hex,
//                 &$lines[$offset],
//             ) {
//                 Ok(v) => Ok(v as i32),
//                 Err(_) => $crate::parse::parse_one::<i32>(
//                     $offset,
//                     $crate::parse::Repr::Dec,
//                     &$lines[$offset],
//                 ),
//             },
//         }?;
//         (v, 1)
//     }};
//     (
//         [Vec3: $parse_ty:ty],
//         $field_type:path,
//         $comment:expr,
//         $pointers:ident,
//         $resman:ident,
//         $texman:ident,
//         $lines:ident[$offset:ident]
//     ) => {{
//         for i in 0..3 {
//             $crate::parse::check_num_type::<$parse_ty>(
//                 $offset + i,
//                 $comment,
//                 &$lines[$offset + i],
//             )?;
//         }
//         let x = $crate::parse::parse_one::<$parse_ty>(
//             $offset + 0,
//             $crate::parse::Repr::Dec,
//             &$lines[$offset + 0],
//         )?;
//         let y = $crate::parse::parse_one::<$parse_ty>(
//             $offset + 1,
//             $crate::parse::Repr::Dec,
//             &$lines[$offset + 1],
//         )?;
//         let z = $crate::parse::parse_one::<$parse_ty>(
//             $offset + 2,
//             $crate::parse::Repr::Dec,
//             &$lines[$offset + 2],
//         )?;
//         ([x, y, z], 3)
//     }};
//     (
//         ObjClass,
//         $field_type:path,
//         $comment:expr,
//         $pointers:ident,
//         $resman:ident,
//         $texman:ident,
//         $lines:ident[$offset:ident]
//     ) => {{
//         let tmp = consume_obj_class(&$lines[$offset])?;
//         (tmp, 1)
//     }};
//     (
//         Symbol,
//         $field_type:path,
//         $comment:expr,
//         $pointers:ident,
//         $resman:ident,
//         $texman:ident,
//         $lines:ident[$offset:ident]
//     ) => {{
//         let tmp = $lines[$offset].1;
//         (tmp, 1)
//     }};
//     (
//         $repr_package:tt,
//         $field_type:path,
//         $comment:expr,
//         $pointers:ident,
//         $resman:ident,
//         $texman:ident,
//         $lines:ident[$offset:ident]
//     ) => {{
//         panic!();
//     }};
// }

macro_rules! make_storage_type {
    (Shape, $field_type:path) => {
        std::option::Option<std::rc::Rc<std::boxed::Box<$field_type>>>
    };
    (Sound, $field_type:path) => {
        std::option::Option<std::rc::Rc<std::boxed::Box<$field_type>>>
    };
    (HUD, $field_type:path) => {
        std::option::Option<std::rc::Rc<std::boxed::Box<$field_type>>>
    };
    ($_:ident, $field_type:path) => {
        $field_type
    };
}

pub trait FromField {
    type Produces;
    fn from_field(field: &FieldRow) -> Fallible<Self::Produces>;
}

macro_rules! make_consume_fields {
    (Byte, Bool, $field_type:path, $rows:expr, $_p:ident, $_r:ident) => {
        ($rows[0].value().numeric()?.byte()? != 0, 1)
    };

    (Byte, Unsigned, $field_type:path, $rows:expr, $_p:ident, $_r:ident) => {
        ($rows[0].value().numeric()?.byte()? as $field_type, 1)
    };
    (Word, Unsigned, $field_type:path, $rows:expr, $_p:ident, $_r:ident) => {
        ($rows[0].value().numeric()?.word()? as $field_type, 1)
    };
    (DWord, Unsigned, $field_type:path, $rows:expr, $_p:ident, $_r:ident) => {
        ($rows[0].value().numeric()?.dword()? as $field_type, 1)
    };

    (Byte, Signed, $field_type:path, $rows:expr, $_p:ident, $_r:ident) => {
        ($rows[0].value().numeric()?.byte()? as i8 as $field_type, 1)
    };
    (Word, Signed, $field_type:path, $rows:expr, $_p:ident, $_r:ident) => {
        ($rows[0].value().numeric()?.word()? as i16 as $field_type, 1)
    };
    (DWord, Signed, $field_type:path, $rows:expr, $_p:ident, $_r:ident) => {
        ($rows[0].value().numeric()?.dword()? as i32 as $field_type, 1)
    };

    ($_t:ident, Struct, $field_type:path, $rows:expr, $_p:ident, $_r:ident) => {
        (<$field_type as $crate::parse::FromField>::from_field(&$rows[0])?, 1)
    };

    (Word, Vec3, $field_type:path, $rows:expr, $_p:ident, $_r:ident) => {
        {
            let x = $rows[0].value().numeric()?.word()? as i16 as f32;
            let y = $rows[1].value().numeric()?.word()? as i16 as f32;
            let z = $rows[2].value().numeric()?.word()? as i16 as f32;
            let p = Point3::new(x, y, z);
            (p, 3)
        }
    };

    // FIXME: fix up consume_ptr and just call that for all of these types
    (Ptr, Shape, $field_type:path, $rows:expr, $pointers:ident, $resman:ident) => {
        // Null ptr is represented as `DWord 0`.
        if !$rows[0].value().is_pointer() {
            ensure!($rows[0].value().numeric()?.dword()? == 0u32, "null pointer must be dword 0");
            (None, 1)
        } else {
            let (sym, values) = $rows[0].value().pointer()?;
            ensure!(sym.ends_with("hape"), "expected shape in ptr name");
            let name = $crate::parse::string(&values[0])?.to_uppercase();
            (Some($resman.load_sh(&name)?), 1)
        }
    };

    (Ptr, Sound, $field_type:path, $rows:expr, $pointers:ident, $resman:ident) => {
        // Null ptr is represented as `DWord 0`.
        if !$rows[0].value().is_pointer() {
            ensure!($rows[0].value().numeric()?.dword()? == 0u32, "null pointer must be dword 0");
            (None, 1)
        } else {
            let (sym, values) = $rows[0].value().pointer()?;
            ensure!(sym.ends_with("ound"), "expected sound in ptr name");
            let name = $crate::parse::string(&values[0])?.to_uppercase();
            (Some($resman.load_sound(&name)?), 1)
        }
    };

    (Ptr, HUD, $field_type:path, $rows:expr, $pointers:ident, $resman:ident) => {
        // Null ptr is represented as `DWord 0`.
        if !$rows[0].value().is_pointer() {
            ensure!($rows[0].value().numeric()?.dword()? == 0u32, "null pointer must be dword 0");
            (None, 1)
        } else {
            let (sym, values) = $rows[0].value().pointer()?;
            ensure!(sym == "hudName", "expected hud in ptr name");
            let name = $crate::parse::string(&values[0])?.to_uppercase();
            (Some($resman.load_hud(&name)?), 1)
        }
    };
}

#[macro_export]
macro_rules! make_type_struct {
    ($structname:ident($parent:ident: $parent_ty:ty, version: $version_ty:ident) {
        $( ($row_type:ident, [ $( $row_format:ident ),* ], $comment:expr, $parse_type:ident, $field_name:ident, $field_type:path, $version_supported:ident, $default_value:expr) ),*
    }) => {
        #[allow(dead_code)]
        pub struct $structname {
            $parent: $parent_ty,

            $(
                $field_name: make_storage_type!($parse_type, $field_type)
            ),*
        }

        impl $structname {
            pub fn from_lines(
                $parent: $parent_ty,
                lines: &Vec<&str>,
                pointers: &HashMap<&str, Vec<&str>>,
                resman: &ResourceManager,
                texman: &TextureManager
            ) -> Fallible<Self> {
                let file_version = $version_ty::from_len(lines.len())?;

                // Tokenize all rows and parse to value, capturing repr and size.
                let mut rows = Vec::new();
                for line in lines {
                    let row = $crate::parse::FieldRow::from_line(line, pointers)?;
                    rows.push(row);
                }

                // Group rows into fields.
                // Validate reprs and comment against field.
                // Build and assign fields.

                let mut offset = 0;
                $(
                    ensure!(rows[offset].comment().is_none() || rows[offset].comment() == Some($comment), "non-matching comment");
                    let field_version = $version_ty::$version_supported;
                    let $field_name = if field_version <= file_version {
                        println!("AT FIELD: {:?}", rows[offset]);

                        let (intermediate, count) = make_consume_fields!($row_type, $parse_type, $field_type, &rows[offset..], pointers, resman);
                        offset += count;
                        intermediate
                    } else {
                        $default_value
                    };
                );*
                
                return Ok(Self {
                    $parent,
                    $(
                        $field_name
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

// The pointer follow above gets us a list of names. Many users expect exactly
// one name. This function encodes that expectation and returns the name.
pub fn unpack_name(names: Vec<String>) -> Fallible<String> {
    ensure!(
        names.len() == 1,
        "expected a single name under pointer, found {:?}",
        names
    );
    return Ok(names[0].clone());
}

// The file provides us with a string giving the type, a value, and possible a line comment.
// In theory we know the type and the field based on the offset. Make sure our expectations
// match the reality.
// pub fn check_num_type<T>(
//     offset: usize,
//     comment: &'static str,
//     actual: &(FieldType, &str, Option<&str>),
// ) -> Fallible<()>
// where
//     T: Num + FromStr + GetFieldType,
// {
//     let expect_type = T::field_type();
//     ensure!(
//         expect_type == actual.0,
//         "expected {:?}, but found {:?} at line {} of section, {}",
//         expect_type,
//         actual.0,
//         offset,
//         comment
//     );
//     if let Some(c) = actual.2 {
//         if comment.len() > 0 {
//             ensure!(
//                 c.starts_with(comment),
//                 "expected {}, but found {} at line {} of section",
//                 comment,
//                 c,
//                 offset
//             );
//         }
//     }
//     return Ok(());
// }

// The file provides us with a string giving the type, a value, and possible a line comment.
// In theory we know the type and the field based on the offset. Make sure our expectations
// match the reality and use that to parse and return the value.
// pub fn parse_one<T>(
//     offset: usize,
//     repr: Repr,
//     actual: &(FieldType, &str, Option<&str>),
// ) -> Fallible<T>
// where
//     T: Num + FromStr + GetFieldType + AsPrimitive<u32>,
//     u32: AsPrimitive<T>,
//     <T as FromStr>::Err: ::std::error::Error + 'static + Send + Sync,
//     <T as ::num_traits::Num>::FromStrRadixErr: ::std::error::Error + 'static + Send + Sync,
// {
//     return Ok(match repr {
//         Repr::Dec => actual.1.parse::<T>()?,
//         Repr::Hex => {
//             ensure!(
//                 actual.1.starts_with('$'),
//                 "expected a hex number at line {} of section, but got {}",
//                 offset,
//                 actual.1
//             );
//             T::from_str_radix(&actual.1[1..], 16)?
//         }
//         Repr::Car => {
//             ensure!(
//                 actual.1.starts_with('^'),
//                 "expected a caret number at line {} of section, but got {}",
//                 offset,
//                 actual.1
//             );
//             let v = actual.1[1..].parse::<T>()?;
//             let u: u32 = v.as_() * 256;
//             let t: T = u.as_();
//             t
//         }
//     });
// }

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
