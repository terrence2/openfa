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
#[macro_use] extern crate error_chain;

use std::mem;
use std::collections::HashMap;

mod errors {
    error_chain! {}
}
use ::errors::{Result, ResultExt};

pub struct Type {
    tag: TypeTag,
    object: ObjectType,
}

impl Type {
    pub fn new(data: &str) -> Result<Self> {
        let lines = data.lines().collect::<Vec<&str>>();
        ensure!(lines[0] == "[brent's_relocatable_format]", "not an type file");

        // Extract all pointer sections.
        let mut pointers = HashMap::new();
        let pointer_names = lines.iter().filter(|&l| l.starts_with(":")).map(|&l| l).collect::<Vec<&str>>();
        for pointer_name in pointer_names {
            let pointer_data = lines.iter()
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

        let obj_section = lines.iter()
            .skip_while(|&l| l.find("START OF OBJ_TYPE").is_none())
            .take_while(|&l| l.find("END OF OBJ_TYPE").is_none())
            .map(|&l| l.trim())
            .filter(|&l| l.len() != 0 && !l.starts_with(";"))
            .collect::<Vec<&str>>();

        let tag = TypeTag::new(Type::byte(obj_section[0]).chain_err(|| "obj section 0")?).chain_err(|| "type tag")?;

        let object = ObjectType::new(obj_section, &pointers).chain_err(|| "ObjectType::new")?;
        return Ok(Type { tag, object });
    }

    fn hex(n: &str) -> Result<u32> {
        ensure!(n.is_ascii(), "non-ascii in number");
        return Ok(u32::from_str_radix(&n[1..], 16).chain_err(|| "from str radix")?);
    }

    fn byte(line: &str) -> Result<u8> {
        let parts = line.split_whitespace().collect::<Vec<&str>>();
        ensure!(parts.len() == 2, "expected 2 parts");
        ensure!(parts[0] == "byte", "expected byte type");
        return Ok(parts[1].parse::<u8>().chain_err(|| "parse u8")?);
    }

    fn word(line: &str) -> Result<i16> {
        let parts = line.split_whitespace().collect::<Vec<&str>>();
        ensure!(parts.len() == 2, "expected 2 parts");
        ensure!(parts[0] == "word", "expected word type");
        return Ok(match parts[1].parse::<i16>() {
            Ok(n) => n,
            Err(_) => {
                let dw = Self::hex(parts[1]).chain_err(|| "parse i16")?;
                let uw = dw as u16;
                unsafe { mem::transmute(uw) }
            }
        });
    }

    fn dword(line: &str) -> Result<u32> {
        let parts = line.split_whitespace().collect::<Vec<&str>>();
        ensure!(parts.len() == 2, "expected 2 parts");
        ensure!(parts[0] == "dword", "expected dword type");
        return Ok(match parts[1].parse::<u32>() {
            Ok(n) => n,
            Err(_) => Self::hex(parts[1]).chain_err(|| "parse i16")?
        });
    }

    fn string(line: &str) -> Result<String> {
        let parts = line.splitn(2, " ").collect::<Vec<&str>>();
        ensure!(parts.len() == 2, "expected 2 parts");
        ensure!(parts[0] == "string", "expected string type");
        ensure!(parts[1].starts_with("\""), "expected string to be quoted");
        ensure!(parts[1].ends_with("\""), "expected string to be quoted");
        let unquoted = parts[1].chars().skip(1).take(parts[1].len() - 2).collect::<String>();
        return Ok(unquoted);
    }
}

// placeholder
pub struct Shape {}
impl Shape { fn new(_: &str) -> Result<Self> { Ok(Shape{}) } }
pub struct Sound {}

#[derive(Debug)]
#[repr(u8)]
pub enum TypeTag {
    Object = 1,
    NPC = 3,
    Plane = 5,
    Projectile = 7,
}

impl TypeTag {
    fn new(n: u8) -> Result<TypeTag> {
        if n != 1 && n != 3 && n != 5 && n != 7 {
            bail!("unknown TypeTag {}", n);
        }
        return Ok(unsafe { mem::transmute(n) });
    }
}

pub enum ProcKind {
    OBJ,
    PLANE,
    CARRIER,
    GV,
    PROJ,
    EJECT,
    STRIP,
    CATGUY,
}

impl ProcKind {
    fn new(s: &str) -> Result<ProcKind> {
        let parts = s.split_whitespace().collect::<Vec<&str>>();
        ensure!(parts[0] == "symbol", "expected 'symbol'");
        return Ok(match parts[1] {
            "_OBJProc" => ProcKind::OBJ,
            "_PLANEProc" => ProcKind::PLANE,
            "_CARRIERProc" => ProcKind::CARRIER,
            "_GVProc" => ProcKind::GV,
            "_PROJProc" => ProcKind::PROJ,
            "_EJECTProc" => ProcKind::EJECT,
            "_STRIPProc" => ProcKind::STRIP,
            "_CATGUYProc" => ProcKind::CATGUY,
            _ => bail!("Unexpected proc kind: {}", parts[1])
        });
    }
}

pub struct ObjectType {
    //;---------------- general info ----------------
    unk1: i16,
    unk2: i16,
    short_name: String,
    long_name: String,
    file_name: String,
    unk4: u32,
    unk5: i16,
    shape: Option<Shape>,
    shadow_shape: Option<Shape>,
    unk8: u32,
    unk9: u32,
    unk10: i16,
    unk11: i16,
    unk12: i16,
    unk13: u32,
    unk14: u32,
    unk15: i16,
    unk16: i16,
    unk17: i16,
    unk18: u32,
    unk19: u32,
    unk20: i16,
    unk21: i16,
    unk22: i16,
    unk23: i16,
    unk24: i16,
    unk25: i16,
    unk26: i16,
    unk27: i16,
    unk28: i16,
    unk29: i16,
    unk30: i16,
    unk31: i16,
    unk32: i16,
    unk33: u8,
    unk34: u8,
    unk35: u32,
    unk36: i16,

    //;---------------- movement info ----------------
    unk37: i16,
    unk38: i16,
    unk39: i16,
    unk40: i16,
    unk41: i16,
    unk42: i16,
    unk43: i16,
    unk44: i16,
    unk45: u32,
    unk46: u32,
    unk47: u32,
    unk48: u32,
    util_proc: ProcKind,

    //;---------------- sound info ----------------
    loop_sound: Sound,
    second_sound: Sound,
    engine_on_sound: Sound,
    engine_off_sound: Sound,
    unk54: u8,
    unk55: i16,
    unk56: i16,
    unk57: i16,
    unk58: i16,
    unk59: i16,
    unk60: i16,
    unk61: i16,
    unk62: i16,
    hud_name: String,
}

impl ObjectType {
    fn new(lines: Vec<&str>, pointers: &HashMap<&str, Vec<&str>>) -> Result<ObjectType> {
        fn name_at(i: usize, pointers: &HashMap<&str, Vec<&str>>) -> Result<String> {
            return match pointers[":ot_names"].get(i) {
                None => bail!("expected a name at position {}", i),
                Some(s) => Ok(Type::string(s).chain_err(|| "parse name")?)
            }
        }

        return Ok(ObjectType {
            unk1: Type::word(lines[1]).chain_err(|| "line 1")?,
            unk2: Type::word(lines[2]).chain_err(|| "line 2")?,
            short_name: name_at(0, pointers).chain_err(|| "name at 0")?,
            long_name: name_at(1, pointers).chain_err(|| "name at 1")?,
            file_name: name_at(2, pointers).chain_err(|| "name at 2")?,
            unk4: Type::dword(lines[4]).chain_err(|| "line 4")?,
            unk5: Type::word(lines[5]).chain_err(|| "line 5")?,
            shape: pointers.get(":shape").and_then(|l| Shape::new(l[0]).ok()),
            shadow_shape: pointers.get(":shadowShape").and_then(|l| Shape::new(l[0]).ok()),
            unk8: Type::dword(lines[8]).chain_err(|| "line 8")?,
            unk9: Type::dword(lines[9]).chain_err(|| "line 9")?,
            unk10: Type::word(lines[10]).chain_err(|| "line 10")?,
            unk11: Type::word(lines[11]).chain_err(|| "line 11")?,
            unk12: Type::word(lines[12]).chain_err(|| "line 12")?,
            unk13: Type::dword(lines[13]).chain_err(|| "line 13")?,
            unk14: Type::dword(lines[14]).chain_err(|| "line 14")?,
            unk15: Type::word(lines[15]).chain_err(|| "line 15")?,
            unk16: Type::word(lines[16]).chain_err(|| "line 16")?,
            unk17: Type::word(lines[17]).chain_err(|| "line 17")?,
            unk18: Type::dword(lines[18]).chain_err(|| "line 18")?,
            unk19: Type::dword(lines[19]).chain_err(|| "line 19")?,
            unk20: Type::word(lines[20]).chain_err(|| "line 20")?,
            unk21: Type::word(lines[21]).chain_err(|| "line 21")?,
            unk22: Type::word(lines[22]).chain_err(|| "line 22")?,
            unk23: Type::word(lines[23]).chain_err(|| "line 23")?,
            unk24: Type::word(lines[24]).chain_err(|| "line 24")?,
            unk25: Type::word(lines[25]).chain_err(|| "line 25")?,
            unk26: Type::word(lines[26]).chain_err(|| "line 26")?,
            unk27: Type::word(lines[27]).chain_err(|| "line 27")?,
            unk28: Type::word(lines[28]).chain_err(|| "line 28")?,
            unk29: Type::word(lines[29]).chain_err(|| "line 29")?,
            unk30: Type::word(lines[30]).chain_err(|| "line 30")?,
            unk31: Type::word(lines[31]).chain_err(|| "line 31")?,
            unk32: Type::word(lines[32]).chain_err(|| "line 32")?,
            unk33: Type::byte(lines[33]).chain_err(|| "line 33")?,
            unk34: Type::byte(lines[34]).chain_err(|| "line 34")?,
            unk35: Type::dword(lines[35]).chain_err(|| "line 35")?,
            unk36: Type::word(lines[36]).chain_err(|| "line 36")?,
            //;---------------- movement info ----------------
            unk37: Type::word(lines[37]).chain_err(|| "line 37")?,
            unk38: Type::word(lines[38]).chain_err(|| "line 38")?,
            unk39: Type::word(lines[39]).chain_err(|| "line 39")?,
            unk40: Type::word(lines[40]).chain_err(|| "line 40")?,
            unk41: Type::word(lines[41]).chain_err(|| "line 41")?,
            unk42: Type::word(lines[42]).chain_err(|| "line 42")?,
            unk43: Type::word(lines[43]).chain_err(|| "line 43")?,
            unk44: Type::word(lines[44]).chain_err(|| "line 44")?,
            unk45: Type::dword(lines[45]).chain_err(|| "line 45")?,
            unk46: Type::dword(lines[46]).chain_err(|| "line 46")?,
            unk47: Type::dword(lines[47]).chain_err(|| "line 47")?,
            unk48: Type::dword(lines[48]).chain_err(|| "line 48")?,
            util_proc: ProcKind::new(lines[49]).chain_err(|| "line 49")?,

            //;---------------- sound info ----------------
            loop_sound: Sound{},
            second_sound: Sound{},
            engine_on_sound: Sound{},
            engine_off_sound: Sound{},
            unk54: Type::byte(lines[54]).chain_err(|| "line 54")?,
            unk55: Type::word(lines[55]).chain_err(|| "line 55")?,
            unk56: Type::word(lines[56]).chain_err(|| "line 56")?,
            unk57: Type::word(lines[57]).chain_err(|| "line 57")?,
            unk58: Type::word(lines[58]).chain_err(|| "line 58")?,
            unk59: Type::word(lines[59]).chain_err(|| "line 59")?,
            unk60: Type::word(lines[60]).chain_err(|| "line 60")?,
            unk61: Type::word(lines[61]).chain_err(|| "line 61")?,
            unk62: Type::word(lines[62]).chain_err(|| "line 62")?,
            hud_name: String::new(),
        });
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
    unk8: Vec<HardPoint>
}

pub struct HardPoint {}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::prelude::*;
    use super::*;

    #[test]
    fn it_works() {
        let mut rv = vec![];
        let paths = fs::read_dir("./test_data").unwrap();
        for i in paths {
            let entry = i.unwrap();
            let path = format!("{}", entry.path().display());
            let mut fp = fs::File::open(entry.path()).unwrap();
            let mut contents = String::new();
            fp.read_to_string(&mut contents).unwrap();
            println!("At: {}", path);
            let t = Type::new(&contents).unwrap();
            assert_eq!(format!("./test_data/{}", t.object.file_name), path);
            rv.push(format!("{:?} <> {}", t.object.long_name, path));
        }
        rv.sort();
        for v in rv {
            println!("{}", v);
        }
    }
}
