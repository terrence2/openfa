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
pub use anyhow::{anyhow, bail, ensure, Result};
pub use std::any::TypeId;
use std::{collections::HashMap, str};

#[derive(Debug, Eq, PartialEq)]
pub enum FieldType {
    Byte,
    Word,
    DWord,
    XWord, // word or dword
    Ptr,
    Symbol,
}

impl FieldType {
    pub fn from_kind(s: &str) -> Result<Self> {
        Ok(match s {
            "byte" => FieldType::Byte,
            "word" => FieldType::Word,
            "dword" => FieldType::DWord,
            "ptr" => FieldType::Ptr,
            "symbol" => FieldType::Symbol,
            _ => bail!("unknown field type {}", s),
        })
    }

    pub fn is_numeric(&self) -> bool {
        matches!(self, FieldType::Byte | FieldType::Word | FieldType::DWord)
    }
}

#[derive(Clone, Copy, Debug)]
pub enum FieldNumber {
    Byte(u8),
    Word(u16),
    DWord(u32),
}

impl FieldNumber {
    fn from_kind_and_str(kind: &FieldType, s: &str) -> Result<(Repr, Self)> {
        let (repr, v32) = Self::parse_numeric(s)?;
        let num = match kind {
            FieldType::Byte => FieldNumber::Byte(v32 as u8),
            FieldType::Word => FieldNumber::Word(v32 as u16),
            FieldType::DWord => FieldNumber::DWord(v32 as u32),
            _ => bail!("not a number"),
        };
        Ok((repr, num))
    }

    // Note: some instances are marked as one size, but represented as 32 bits
    // anyway. The assumption appears to be that truncation will happen. At
    // least one of these instances implies sign extension as well.
    fn parse_numeric(vs: &str) -> Result<(Repr, u32)> {
        let tpl = if let Some(hex) = vs.strip_prefix('$') {
            (Repr::Hex, u32::from_str_radix(hex, 16)?)
        } else if let Some(short) = vs.strip_prefix('^') {
            (Repr::Car, short.parse::<u32>()? * 256)
        } else {
            (Repr::Dec, vs.parse::<i32>()? as u32)
        };
        Ok(tpl)
    }

    pub fn byte(self) -> Result<u8> {
        match self {
            FieldNumber::Byte(b) => Ok(b),
            _ => bail!("not a byte"),
        }
    }

    pub fn word(self) -> Result<u16> {
        match self {
            FieldNumber::Word(w) => Ok(w),
            _ => bail!("not a word"),
        }
    }

    pub fn dword(self) -> Result<u32> {
        match self {
            FieldNumber::DWord(dw) => Ok(dw),
            _ => bail!("not a dword"),
        }
    }

    pub fn unsigned(self) -> Result<u32> {
        match self {
            FieldNumber::DWord(dw) => Ok(dw),
            FieldNumber::Word(w) => Ok(u32::from(w)),
            FieldNumber::Byte(b) => Ok(u32::from(b)),
        }
    }

    pub fn field_type(self) -> FieldType {
        match self {
            FieldNumber::DWord(_) => FieldType::DWord,
            FieldNumber::Word(_) => FieldType::Word,
            FieldNumber::Byte(_) => FieldType::Byte,
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
    fn from_kind_and_str(
        kind: &FieldType,
        raw_values: Vec<&str>,
        pointers: &HashMap<&str, Vec<&str>>,
    ) -> Result<Self> {
        // In USNF, ptr names are represented inline as `byte NN NN NN NN NNN NN NN NNN`.
        // We want to upgrade these automatically to a FieldValue::Ptr when we see them.
        if raw_values.len() > 1 {
            ensure!(*kind == FieldType::Byte, "expected byte N N N");
            let mut ptr = String::new();
            for s in raw_values {
                let n = s.parse::<u8>()?;
                if n == 0 {
                    break;
                }
                ptr.push(n as char);
            }
            if ptr.is_empty() {
                return Ok(FieldValue::Numeric((Repr::Dec, FieldNumber::DWord(0))));
            }
            return Ok(FieldValue::Ptr(
                ":unknown".to_owned(),
                vec![format!("string \"{}\"", ptr)],
            ));
        }

        let s = raw_values
            .first()
            .ok_or_else(|| anyhow!("missing or incorrect field value"))?
            .trim();
        let value = match kind {
            FieldType::Byte => FieldValue::Numeric(FieldNumber::from_kind_and_str(kind, s)?),
            FieldType::Word => FieldValue::Numeric(FieldNumber::from_kind_and_str(kind, s)?),
            FieldType::DWord => FieldValue::Numeric(FieldNumber::from_kind_and_str(kind, s)?),
            FieldType::Ptr => {
                let values = pointers[s]
                    .iter()
                    .map(|&v| v.to_owned())
                    .collect::<Vec<String>>();
                FieldValue::Ptr(s.to_owned(), values)
            }
            FieldType::Symbol => FieldValue::Symbol(s.to_owned()),
            FieldType::XWord => bail!("xword is not a valid concrete type"),
        };
        Ok(value)
    }

    pub fn numeric(&self) -> Result<FieldNumber> {
        match self {
            FieldValue::Numeric((_, num)) => Ok(*num),
            _ => bail!("not a number"),
        }
    }

    pub fn pointer(&self) -> Result<(String, Vec<String>)> {
        match self {
            FieldValue::Ptr(s, v) => Ok((s.clone(), v.clone())),
            _ => bail!("not a pointer"),
        }
    }

    pub fn symbol(&self) -> Result<String> {
        match self {
            FieldValue::Symbol(s) => Ok(s.clone()),
            _ => bail!("not a symbol field"),
        }
    }

    pub fn repr(&self) -> Repr {
        match self {
            FieldValue::Numeric((r, _)) => *r,
            FieldValue::Symbol(_s) => Repr::Sym,
            FieldValue::Ptr(_s, _v) => Repr::Sym,
        }
    }

    pub fn field_type(&self) -> FieldType {
        match self {
            FieldValue::Numeric((_, num)) => num.field_type(),
            FieldValue::Symbol(_s) => FieldType::Symbol,
            FieldValue::Ptr(_s, _v) => FieldType::Ptr,
        }
    }
}

#[derive(Debug)]
pub struct FieldRow {
    _kind: FieldType,
    value: FieldValue,
    comment: Option<String>,
}

impl FieldRow {
    pub fn from_line(line: &str, pointers: &HashMap<&str, Vec<&str>>) -> Result<Self> {
        let mut parts = line.splitn(2, ';');
        let mut words = parts
            .next()
            .ok_or_else(|| anyhow!("empty line"))?
            .split(' ')
            .filter(|s| !s.is_empty());
        let comment = parts.next().map(|s| s.trim().to_owned());
        let kind = FieldType::from_kind(
            words
                .next()
                .ok_or_else(|| anyhow!("missing or incorrect field kind"))?
                .trim(),
        )?;
        let raw_values = words.collect::<Vec<&str>>();
        let value = FieldValue::from_kind_and_str(&kind, raw_values, pointers)?;
        Ok(FieldRow {
            _kind: kind,
            value,
            comment,
        })
    }

    pub fn value(&self) -> &FieldValue {
        &self.value
    }

    pub fn comment(&self) -> Option<&str> {
        if let Some(ref c) = self.comment {
            return Some(c as &str);
        }
        None
    }
}

pub fn find_pointers<'a>(lines: &[&'a str]) -> Result<HashMap<&'a str, Vec<&'a str>>> {
    let mut pointers = HashMap::new();
    let pointer_names = lines
        .iter()
        .filter(|&l| l.starts_with(':'))
        .cloned()
        .collect::<Vec<&str>>();
    for pointer_name in pointer_names {
        let pointer_data = lines
            .iter()
            .cloned()
            .skip_while(|&l| l != pointer_name)
            .skip(1)
            .take_while(|&l| !l.starts_with(':') && !l.ends_with("end"))
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .filter(|l| !l.starts_with(';'))
            .collect::<Vec<&str>>();
        pointers.insert(&pointer_name[1..], pointer_data);
    }
    pointers.insert("__empty__", Vec::new());
    Ok(pointers)
}

pub fn find_section<'a>(lines: &[&'a str], section_tag: &str) -> Result<Vec<&'a str>> {
    let start_pattern = format!("START OF {}", section_tag);
    let end_pattern = format!("END OF {}", section_tag);
    let out = lines
        .iter()
        .skip_while(|&l| !l.contains(&start_pattern))
        .take_while(|&l| !l.contains(&end_pattern))
        .map(|&l| l.trim())
        .filter(|&l| !l.is_empty() && !l.starts_with(';'))
        .collect::<Vec<&str>>();
    Ok(out)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Repr {
    Dec,
    Hex,
    Car,
    Sym,
}

#[macro_export]
macro_rules! make_storage_type {
    (PtrStr, $_ft:path) => {
        std::option::Option<std::string::String>
    };
    ($_:ident, $field_type:path) => {
        $field_type
    };
}

#[macro_export]
macro_rules! make_show_field {
    (PtrStr, $myself:ident, $field_name:ident) => {
        format!("{:?}", $myself.$field_name)
    };
    ($_:ident, $myself:ident, $field_name:ident) => {
        format!("{}", $myself.$field_name)
    };
}

pub trait FromRow {
    type Produces;
    fn from_row(row: &FieldRow, pointers: &HashMap<&str, Vec<&str>>) -> Result<Self::Produces>;
}

pub trait FromRows {
    type Produces;
    fn from_rows(
        rows: &[FieldRow],
        pointers: &HashMap<&str, Vec<&str>>,
    ) -> Result<(Self::Produces, usize)>;
}

#[macro_export]
macro_rules! make_consume_fields {
    (Byte, Bool, $field_type:path, $rows:expr, $_p:ident) => {
        ($rows[0].value().numeric()?.byte()? != 0, 1)
    };

    (Byte, Unsigned, $field_type:path, $rows:expr, $_p:ident) => {
        ($rows[0].value().numeric()?.byte()? as $field_type, 1)
    };
    (Word, Unsigned, $field_type:path, $rows:expr, $_p:ident) => {
        (<$field_type>::from($rows[0].value().numeric()?.word()?), 1)
    };
    (DWord, Unsigned, $field_type:path, $rows:expr, $_p:ident) => {
        (<$field_type>::from($rows[0].value().numeric()?.dword()?), 1)
    };
    (Num, Unsigned, $field_type:path, $rows:expr, $_p:ident) => {
        ($rows[0].value().numeric()?.unsigned()? as $field_type, 1)
    };

    (Byte, Signed, $field_type:path, $rows:expr, $_p:ident) => {
        ($rows[0].value().numeric()?.byte()? as i8 as $field_type, 1)
    };
    (Word, Signed, $field_type:path, $rows:expr, $_p:ident) => {
        (
            <$field_type>::from($rows[0].value().numeric()?.word()? as i16),
            1,
        )
    };
    (DWord, Signed, $field_type:path, $rows:expr, $_p:ident) => {
        (
            $rows[0].value().numeric()?.dword()? as i32 as $field_type,
            1,
        )
    };

    ($_t:ident, Custom, $field_type:path, $rows:expr, $pointers:ident) => {
        (
            <$field_type as $crate::parse::FromRow>::from_row(&$rows[0], $pointers)?,
            1,
        )
    };
    ($_t:ident, CustomN, $field_type:path, $rows:expr, $pointers:ident) => {
        <$field_type as $crate::parse::FromRows>::from_rows($rows, $pointers)?
    };

    (Word, Vec3, $field_type:path, $rows:expr, $_p:ident) => {{
        let x = f32::from($rows[0].value().numeric()?.word()? as i16);
        let y = f32::from($rows[1].value().numeric()?.word()? as i16);
        let z = f32::from($rows[2].value().numeric()?.word()? as i16);
        let p = Point3::new(x, y, z);
        (p, 3)
    }};

    (Ptr, PtrStr, $_ft:path, $rows:expr, $pointers:ident) => {
        // Null ptr is represented as `DWord 0`.
        if $rows[0].value().pointer().is_err() {
            ensure!(
                $rows[0].value().numeric()?.dword()? == 0u32,
                "null pointer must be dword 0"
            );
            (None, 1)
        } else {
            let (_sym, values) = $rows[0].value().pointer()?;
            let name = $crate::parse::parse_string(&values[0])?.to_uppercase();
            (Some(name), 1)
        }
    };
}

#[macro_export]
macro_rules! make_validate_field_repr {
    ([ $( $row_format:ident ),* ], $row:expr, $field_name:expr) => {
        let reprs = vec![$($crate::parse::Repr::$row_format),*];
        let valid = reprs.iter().map(|&r| r == $row.value().repr()).any(|v| v);
        ensure!(valid, "field {} repr of {:?} did not match any expected reprs: {:?}", $field_name, $row.value().repr(), reprs);
    };
}

#[macro_export]
macro_rules! make_validate_field_type {
    (Ptr, $row:expr, $field_name:expr) => {
        ensure!(
            $row.value().field_type() == $crate::parse::FieldType::Ptr
                || $row.value().field_type() == $crate::parse::FieldType::DWord,
            "expected {} to have ptr or dword field_type",
            $field_name
        );
    };
    (Num, $row:expr, $field_name:expr) => {
        ensure!(
            $row.value().field_type() == $crate::parse::FieldType::Word
                || $row.value().field_type() == $crate::parse::FieldType::DWord
                || $row.value().field_type() == $crate::parse::FieldType::Byte,
            "expected {} to have numeric field_type",
            $field_name
        );
    };
    ($row_type:ident, $row:expr, $field_name:expr) => {
        ensure!(
            $row.value().field_type() == $crate::parse::FieldType::$row_type,
            "expected {} to have {:?} field_type",
            $field_name,
            $crate::parse::FieldType::$row_type
        );
    };
}

#[macro_export]
macro_rules! make_type_struct {
    ($structname:ident($parent:ident: $parent_ty:ty, version: $version_ty:ident) {
        $( ($row_type:ident, [ $( $row_format:ident ),* ], $comment:expr, $parse_type:ident, $field_name:ident, $field_type:path, $version_supported:ident, $default_value:expr) ),*
    }) => {
        #[derive(Clone, Debug)]
        #[allow(dead_code)]
        pub struct $structname {
            pub $parent: $parent_ty,

            $(
                pub $field_name: $crate::make_storage_type!($parse_type, $field_type)
            ),*
        }

        impl $structname {
            #[allow(clippy::cast_lossless)]
            pub fn from_lines(
                $parent: $parent_ty,
                lines: &[&str],
                pointers: &HashMap<&str, Vec<&str>>
            ) -> Result<Self> {
                let file_version = $version_ty::from_len(lines.len())?;

                // Tokenize all rows and parse to value, capturing repr and size.
                let mut rows = Vec::new();
                for line in lines {
                    let row = $crate::parse::FieldRow::from_line(line, pointers)?;
                    rows.push(row);
                }

                let mut offset = 0;
                $(
                    // Take a field if it exists in this version of the format.
                    let field_version = $version_ty::$version_supported;
                    let $field_name = if field_version <= file_version {
                        if offset == rows.len() {
                            bail!("ran out of data before end")
                        }

                        // Validate comment only on present rows.
                        ensure!(rows[offset].comment().is_none() || rows[offset].comment().unwrap().starts_with($comment), "non-matching comment");

                        let (intermediate, count) = $crate::make_consume_fields!($row_type, $parse_type, $field_type, &rows[offset..], pointers);

                        // Validate all consumed fields
                        for i in 0..count {
                            $crate::make_validate_field_repr!([ $( $row_format ),* ], &rows[offset + i], stringify!($field_name));
                            $crate::make_validate_field_type!($row_type, &rows[offset + i], stringify!($field_name));
                        }

                        offset += count;
                        intermediate
                    } else {
                        $default_value
                    };
                )*
                ensure!(offset >= rows.len(), "did not read all rows");

                return Ok(Self {
                    $parent,
                    $(
                        $field_name
                    ),*
                });
            }

            pub fn fields() -> &'static [&'static str] {
                &[$(stringify!($field_name)),*]
            }

            pub fn get_field(&self, field: &'static str) -> String {
                match field {
                    $(
                        stringify!($field_name) => $crate::make_show_field!($parse_type, self, $field_name)
                    ),*,
                    _ => String::new()
                }
            }
        }
    }
}

pub fn parse_string(line: &str) -> Result<String> {
    let parts = line.splitn(2, ' ').collect::<Vec<&str>>();
    ensure!(parts.len() == 2, "expected 2 parts");
    ensure!(parts[0] == "string", "expected string type");
    ensure!(parts[1].starts_with('"'), "expected string to be quoted");
    ensure!(parts[1].ends_with('"'), "expected string to be quoted");
    let unquoted = parts[1]
        .chars()
        .skip(1)
        .take(parts[1].len() - 2)
        .collect::<String>();
    Ok(unquoted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_string() {
        assert_eq!(parse_string("string \"\"").unwrap(), "");
        assert_eq!(parse_string("string \"foo\"").unwrap(), "foo");
        assert_eq!(
            parse_string("string \"foo bar baz\"").unwrap(),
            "foo bar baz"
        );
        assert_eq!(
            parse_string("string \"foo\"bar\"baz\"").unwrap(),
            "foo\"bar\"baz"
        );
        assert!(parse_string("string \"foo").is_err());
        assert!(parse_string("string foo\"").is_err());
        assert!(parse_string("string foo").is_err());
    }
}
