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
#[macro_use]
extern crate failure;

use failure::Error;
use std::mem;
use std::collections::HashMap;

pub fn find_pointers<'a>(lines: &Vec<&'a str>) -> Result<HashMap<&'a str, Vec<&'a str>>, Error> {
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
            .take_while(|&l| !l.starts_with(":"))
            .map(|l| l.trim())
            .filter(|l| l.len() != 0)
            .filter(|l| !l.starts_with(";"))
            .collect::<Vec<&str>>();
        pointers.insert(pointer_name, pointer_data);
    }
    return Ok(pointers);
}

pub fn find_section<'a>(lines: &Vec<&'a str>, section_tag: &str) -> Result<Vec<&'a str>, Error> {
    let start_pattern = format!("START OF {}", section_tag);
    let end_pattern = format!("END OF {}", section_tag);
    return Ok(
        lines
            .iter()
            .skip_while(|&l| l.find(&start_pattern).is_none())
            .take_while(|&l| l.find(&end_pattern).is_none())
            .map(|&l| l.trim())
            .filter(|&l| l.len() != 0 && !l.starts_with(";"))
            .collect::<Vec<&str>>());
}

pub mod parse {
    use super::Error;

    fn hex(n: &str) -> Result<u32, Error> {
        ensure!(n.is_ascii(), "non-ascii in number");
        ensure!(n.starts_with("$"), "expected hex to start with $");
        return Ok(u32::from_str_radix(&n[1..], 16)?);
    }

    pub fn byte(line: &str) -> Result<u8, Error> {
        let parts = line.split_whitespace().collect::<Vec<&str>>();
        ensure!(parts.len() == 2, "expected 2 parts");
        ensure!(parts[0] == "byte", "expected byte type");
        return Ok(parts[1].parse::<u8>()?);
    }

    pub fn word(line: &str) -> Result<i16, Error> {
        let parts = line.split_whitespace().collect::<Vec<&str>>();
        ensure!(parts.len() == 2, "expected 2 parts");
        ensure!(parts[0] == "word", "expected word type");
        return Ok(match parts[1].parse::<i16>() {
            Ok(n) => n,
            Err(_) => hex(parts[1])? as u16 as i16
        });
    }

    pub fn dword(line: &str) -> Result<u32, Error> {
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
                    parts[1][1..].parse::<u32>()?
                }
            }
        });
    }

    pub fn string(line: &str) -> Result<String, Error> {
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
}

#[derive(Debug)]
#[repr(u8)]
pub enum TypeTag {
    Object = 1,
    NPC = 3,
    Plane = 5,
    Projectile = 7,
}

impl TypeTag {
    pub fn new(n: u8) -> Result<TypeTag, Error> {
        if n != 1 && n != 3 && n != 5 && n != 7 {
            bail!("unknown TypeTag {}", n);
        }
        return Ok(unsafe { mem::transmute(n) });
    }
}

pub struct NpcType {
    // dword $0
    unk0: u32,
    // dword 0
    unk1: u32,
    // byte 20
    unk2: u8,
    // byte 60
    unk3: u8,
    // byte 40
    unk4: u8,
    // word 32767
    unk5: i16,
    // word 0
    unk6: i16,
    // byte 1
    unk7: u8,
    // ptr hards
    unk8: Vec<HardPoint>,
}

pub struct HardPoint {}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::prelude::*;
    use super::*;

    #[test]
    fn parse_byte() {
        assert_eq!(parse::byte("byte 0").unwrap(), 0);
        assert_eq!(parse::byte("byte 255").unwrap(), 255);
        assert!(parse::byte("-1").is_err());
    }

    #[test]
    fn parse_word() {
        assert_eq!(parse::word("word 0").unwrap(), 0);
        assert_eq!(parse::word("word -0").unwrap(), 0);
        assert_eq!(parse::word("word -32768").unwrap(), -32768);
        assert_eq!(parse::word("word 32767").unwrap(), 32767);
        assert_eq!(parse::word("word $0000").unwrap(), 0);
        assert_eq!(parse::word("word $FFFF").unwrap(), -1);
        assert_eq!(parse::word("word $7FFF").unwrap(), 32767);
        assert_eq!(parse::word("word $8000").unwrap(), -32768);
        assert_eq!(parse::word("word $ffff8000").unwrap(), -32768);
        assert!(parse::word("word -32769").is_err());
        assert!(parse::word("word 32768").is_err());
    }

    #[test]
    fn parse_dword() {
        assert_eq!(parse::dword("dword 0").unwrap(), 0);
        assert_eq!(parse::dword("dword $0").unwrap(), 0);
        assert_eq!(parse::dword("dword ^0").unwrap(), 0);
        assert_eq!(parse::dword("dword $FFFFFFFF").unwrap(), u32::max_value());
        assert_eq!(parse::dword("dword ^100").unwrap(), 100);
    }

    #[test]
    fn parse_string() {
        assert_eq!(parse::string("string \"\"").unwrap(), "");
        assert_eq!(parse::string("string \"foo\"").unwrap(), "foo");
        assert_eq!(parse::string("string \"foo bar baz\"").unwrap(), "foo bar baz");
        assert_eq!(parse::string("string \"foo\"bar\"baz\"").unwrap(), "foo\"bar\"baz");
        assert!(parse::string("string \"foo").is_err());
        assert!(parse::string("string foo\"").is_err());
        assert!(parse::string("string foo").is_err());
    }
}
