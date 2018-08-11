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
extern crate entity;
#[macro_use]
extern crate failure;
extern crate ot;

use entity::{parse, Resource};
use failure::Fallible;
use ot::ObjectType;
use std::collections::HashMap;

// placeholder
pub struct Sound {}
impl Resource for Sound {
    fn from_file(_: &str) -> Fallible<Self> {
        Ok(Sound {})
    }
}

#[allow(dead_code)]
pub struct ProjectileType {
    obj: ObjectType,

    unk0: u32,              // $2a06f
    unk1: i16,              // 1
    unk2: u8,               // 10
    pub short_name: String, // ptr si_names
    pub long_name: String,
    pub file_name: String,
    unk4: i16,         // 0
    unk5: u8,          // $0
    unk6: u8,          // 3
    unk7: u8,          // $0
    unk8: u8,          // 0
    unk9: u8,          // 0
    unk10: u8,         // 0
    unk11: u8,         // 0
    unk12: u8,         // 0
    unk13: i16,        // 8190
    unk14: i16,        // 8190
    unk15: u32,        // ^0
    unk16: u32,        // ^360000
    unk17: u32,        // $80000000
    unk18: u32,        // $7fffffff
    unk19: i16,        // 8190
    unk20: i16,        // 8190
    unk21: u32,        // ^500
    unk22: u32,        // ^360000
    unk23: u32,        // $80000000
    unk24: u32,        // $7fffffff
    unk25: u8,         // 100
    unk26: u8,         // 100
    unk27: u8,         // 40
    unk28: u8,         // 5
    unk29: u8,         // 0
    unk30: i16,        // 0
    unk31: i16,        // 0
    unk32: i16,        // 0
    unk33: i16,        // 0
    unk34: u8,         // 1
    unk35: u8,         // 1
    unk36: u8,         // 1
    unk37: u8,         // 0
    unk38: u8,         // 80
    unk39: u8,         // 0
    unk40: u8,         // 0
    unk41: u8,         // 0
    unk42: u8,         // 0
    unk43: u8,         // 0
    unk44: u8,         // 31
    unk45: i16,        // 0
    unk46: i16,        // 1026
    unk47: i16,        // 8
    unk48: i16,        // 480
    unk49: i16,        // 480
    unk50: i16,        // 10920
    unk51: i16,        // 8190
    unk52: u8,         // 80
    unk53: u8,         // 100
    unk54: u8,         // 78
    unk55: u8,         // 4
    unk56: u8,         // 20
    unk57: u8,         // 12
    unk58: i16,        // 0
    unk59: i16,        // 1
    unk60: i16,        // 16
    unk61: u8,         // 100
    unk62: u8,         // 8
    unk63: u8,         // 12
    unk64: u8,         // 2
    unk65: u8,         // 15
    unk66: u8,         // 50
    unk67: u8,         // 90
    unk68: u8,         // 90
    unk69: u8,         // 67
    unk70: u8,         // 0
    unk71: u8,         // 0
    unk72: u8,         // 0
    unk73: u8,         // 0
    unk74: u8,         // 100
    unk75: u8,         // 0
    unk76: u8,         // 0
    unk77: u8,         // 0
    unk78: u8,         // 0
    unk79: u8,         // 3
    unk80: i16,        // 4
    unk81: i16,        // 100
    unk82: u8,         // 0
    unk83: u8,         // 21
    unk84: u8,         // 34
    fire_sound: Sound, // ptr fireSound
    unk86: i16,        // 6000
    unk87: i16,        // 0
    unk88: i16,        // 1000
    unk89: i16,        // 100
}

impl ProjectileType {
    pub fn from_str(data: &str) -> Fallible<Self> {
        let lines = data.lines().collect::<Vec<&str>>();
        ensure!(
            lines[0] == "[brent's_relocatable_format]",
            "not a type file"
        );
        let pointers = parse::find_pointers(&lines)?;
        return Self::from_lines(&lines, &pointers);
    }

    fn from_lines(lines: &Vec<&str>, pointers: &HashMap<&str, Vec<&str>>) -> Fallible<Self> {
        let obj = ObjectType::from_lines(lines, pointers)?;
        let lines = parse::find_section(&lines, "PROJ_TYPE")?;

        let si_names = parse::follow_pointer(lines[3], pointers)?;

        return Ok(ProjectileType {
            obj,

            unk0: parse::dword(lines[0])?,           // $2a06f
            unk1: parse::word(lines[1])?,            // 1
            unk2: parse::byte(lines[2])?,            // 10
            short_name: parse::string(si_names[0])?, // ptr si_names
            long_name: parse::string(si_names[1])?,
            file_name: parse::string(si_names[2])?,
            unk4: parse::word(lines[4])?,    // 0
            unk5: parse::byte(lines[5])?,    // $0
            unk6: parse::byte(lines[6])?,    // 3
            unk7: parse::byte(lines[7])?,    // $0
            unk8: parse::byte(lines[8])?,    // 0
            unk9: parse::byte(lines[9])?,    // 0
            unk10: parse::byte(lines[10])?,  // 0
            unk11: parse::byte(lines[11])?,  // 0
            unk12: parse::byte(lines[12])?,  // 0
            unk13: parse::word(lines[13])?,  // 8190
            unk14: parse::word(lines[14])?,  // 8190
            unk15: parse::dword(lines[15])?, // ^0
            unk16: parse::dword(lines[16])?, // ^360000
            unk17: parse::dword(lines[17])?, // $80000000
            unk18: parse::dword(lines[18])?, // $7fffffff
            unk19: parse::word(lines[19])?,  // 8190
            unk20: parse::word(lines[20])?,  // 8190
            unk21: parse::dword(lines[21])?, // ^500
            unk22: parse::dword(lines[22])?, // ^360000
            unk23: parse::dword(lines[23])?, // $80000000
            unk24: parse::dword(lines[24])?, // $7fffffff
            unk25: parse::byte(lines[25])?,  // 100
            unk26: parse::byte(lines[26])?,  // 100
            unk27: parse::byte(lines[27])?,  // 40
            unk28: parse::byte(lines[28])?,  // 5
            unk29: parse::byte(lines[29])?,  // 0
            unk30: parse::word(lines[30])?,  // 0
            unk31: parse::word(lines[31])?,  // 0
            unk32: parse::word(lines[32])?,  // 0
            unk33: parse::word(lines[33])?,  // 0
            unk34: parse::byte(lines[34])?,  // 1
            unk35: parse::byte(lines[35])?,  // 1
            unk36: parse::byte(lines[36])?,  // 1
            unk37: parse::byte(lines[37])?,  // 0
            unk38: parse::byte(lines[38])?,  // 80
            unk39: parse::byte(lines[39])?,  // 0
            unk40: parse::byte(lines[40])?,  // 0
            unk41: parse::byte(lines[41])?,  // 0
            unk42: parse::byte(lines[42])?,  // 0
            unk43: parse::byte(lines[43])?,  // 0
            unk44: parse::byte(lines[44])?,  // 31
            unk45: parse::word(lines[45])?,  // 0
            unk46: parse::word(lines[46])?,  // 1026
            unk47: parse::word(lines[47])?,  // 8
            unk48: parse::word(lines[48])?,  // 480
            unk49: parse::word(lines[49])?,  // 480
            unk50: parse::word(lines[50])?,  // 10920
            unk51: parse::word(lines[51])?,  // 8190
            unk52: parse::byte(lines[52])?,  // 80
            unk53: parse::byte(lines[53])?,  // 100
            unk54: parse::byte(lines[54])?,  // 78
            unk55: parse::byte(lines[55])?,  // 4
            unk56: parse::byte(lines[56])?,  // 20
            unk57: parse::byte(lines[57])?,  // 12
            unk58: parse::word(lines[58])?,  // 0
            unk59: parse::word(lines[59])?,  // 1
            unk60: parse::word(lines[60])?,  // 16
            unk61: parse::byte(lines[61])?,  // 100
            unk62: parse::byte(lines[62])?,  // 8
            unk63: parse::byte(lines[63])?,  // 12
            unk64: parse::byte(lines[64])?,  // 2
            unk65: parse::byte(lines[65])?,  // 15
            unk66: parse::byte(lines[66])?,  // 50
            unk67: parse::byte(lines[67])?,  // 90
            unk68: parse::byte(lines[68])?,  // 90
            unk69: parse::byte(lines[69])?,  // 67
            unk70: parse::byte(lines[70])?,  // 0
            unk71: parse::byte(lines[71])?,  // 0
            unk72: parse::byte(lines[72])?,  // 0
            unk73: parse::byte(lines[73])?,  // 0
            unk74: parse::byte(lines[74])?,  // 100
            unk75: parse::byte(lines[75])?,  // 0
            unk76: parse::byte(lines[76])?,  // 0
            unk77: parse::byte(lines[77])?,  // 0
            unk78: parse::byte(lines[78])?,  // 0
            unk79: parse::byte(lines[79])?,  // 3
            unk80: parse::word(lines[80])?,  // 4
            unk81: parse::word(lines[81])?,  // 100
            unk82: parse::byte(lines[82])?,  // 0
            unk83: parse::byte(lines[83])?,  // 21
            unk84: parse::byte(lines[84])?,  // 34
            fire_sound: parse::maybe_load_resource(lines[85], pointers)?.unwrap(), // ptr fireSound
            unk86: parse::word(lines[86])?,  // 6000
            unk87: parse::word(lines[87])?,  // 0
            unk88: parse::word(lines[88])?,  // 1000
            unk89: parse::word(lines[89])?,  // 100
        });
    }
}

#[cfg(test)]
extern crate omnilib;

#[cfg(test)]
mod tests {
    use super::*;
    use omnilib::OmniLib;

    #[test]
    fn it_can_parse_all_projectile_files() -> Fallible<()> {
        let omni = OmniLib::new_for_test_in_games(vec!["FA"])?;
        for (game, name) in omni.find_matching("*.JT")?.iter() {
            let contents = omni.library(game).load_text(name)?;
            let jt = ProjectileType::from_str(&contents).unwrap();
            assert_eq!(&jt.obj.file_name, name);
            println!(
                "{}:{:13}> {:08X} <> {} <> {}",
                game, name, jt.unk0, jt.obj.long_name, name
            );
        }
        return Ok(());
    }
}
