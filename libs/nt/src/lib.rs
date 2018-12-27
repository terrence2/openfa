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
mod hardpoint;

use crate::hardpoint::HardpointType;
use asset::AssetLoader;
use failure::{ensure, Fallible};
use ot::{
    make_consume_fields, make_storage_type, make_type_struct, make_validate_field_repr,
    make_validate_field_type, parse,
    parse::{FieldRow, FromField},
    ObjectType,
};
use std::collections::HashMap;

// We can detect the version by the number of lines.
#[derive(Debug, Ord, PartialOrd, Eq, PartialEq)]
enum NpcTypeVersion {
    V0,
}

impl NpcTypeVersion {
    fn from_len(_: usize) -> Fallible<Self> {
        Ok(NpcTypeVersion::V0)
    }
}

// Wrap Vec<HP> so that we can impl FromField.
pub struct Hardpoints {
    #[allow(dead_code)]
    all: Vec<HardpointType>,
}

impl FromField for Hardpoints {
    type Produces = Hardpoints;
    fn from_field(
        field: &FieldRow,
        pointers: &HashMap<&str, Vec<&str>>,
        assets: &AssetLoader,
    ) -> Fallible<Self::Produces> {
        let (_name, lines) = field.value().pointer()?;
        let mut off = 0usize;
        let mut hards = Vec::new();
        ensure!(lines.len() % 12 == 0, "expected 12 lines per hardpoint");
        while off < lines.len() {
            let lns = lines[off..off + 12]
                .iter()
                .map(|v| v.as_ref())
                .collect::<Vec<_>>();
            let ht = HardpointType::from_lines((), &lns, pointers, assets)?;
            hards.push(ht);
            off += 12;
        }
        return Ok(Hardpoints { all: hards });
    }
}

make_type_struct![
NpcType(ot: ObjectType, version: NpcTypeVersion) {    // SARAN.NT
    (DWord, [Hex],            "flags", Unsigned, flags,             u32, V0, panic!()), // dword $0 ; flags
    (Ptr,   [Dec, Sym],            "",       AI, ct_name,            AI, V0, panic!()), // ptr ctName
    (Byte,  [Dec], "searchFrequencyT", Unsigned, search_frequency_t, u8, V0, panic!()), // byte 40 ; searchFrequencyT
    (Byte,  [Dec],   "unreadyAttackT", Unsigned, unready_attack_t,   u8, V0, panic!()), // byte 100 ; unreadyAttackT
    (Byte,  [Dec],          "attackT", Unsigned, attack_t,           u8, V0, panic!()), // byte 80 ; attackT
    (Word,  [Dec],        "retargetT", Unsigned, retarget_t,        u16, V0, panic!()), // word 32767 ; retargetT
    (Word,  [Dec],         "zoneDist", Unsigned, zone_dist,         u16, V0, panic!()), // word 0 ; zoneDist
    (Byte,  [Dec],         "numHards", Unsigned, num_hards,          u8, V0, panic!()), // byte 3 ; numHards
	(Ptr,   [Sym],                 "",   Struct, hards,      Hardpoints, V0, panic!())  // ptr hards
}];

impl NpcType {
    pub fn from_str(data: &str, assets: &AssetLoader) -> Fallible<Self> {
        let lines = data.lines().collect::<Vec<&str>>();
        ensure!(
            lines[0] == "[brent's_relocatable_format]",
            "not a type file"
        );
        let pointers = parse::find_pointers(&lines)?;
        let obj_lines = parse::find_section(&lines, "OBJ_TYPE")?;
        let obj = ObjectType::from_lines((), &obj_lines, &pointers, assets)?;
        let npc_lines = parse::find_section(&lines, "NPC_TYPE")?;
        let npc = Self::from_lines(obj, &npc_lines, &pointers, assets)?;
        return Ok(npc);
    }

    // pub fn from_lines(
    //     obj: ObjectType,
    //     lines: &Vec<&str>,
    //     pointers: &HashMap<&str, Vec<&str>>,
    // ) -> Fallible<Self> {
    //     let hardpoint_count = parse::byte(lines[7])? as usize;
    //     let hardpoint_lines = parse::follow_pointer(lines[8], pointers)?;
    //     let mut hardpoints = Vec::new();
    //     for chunk in hardpoint_lines.chunks(12) {
    //         hardpoints.push(Hardpoint::from_lines(chunk, pointers)?);
    //     }
    //     ensure!(
    //         hardpoint_count == hardpoints.len(),
    //         "wrong number of hardpoints"
    //     );

    //     return Ok(NpcType {
    //         obj,
    //         unk0: parse::dword(lines[0])?,
    //         behavior: parse::maybe_load_resource(lines[1], pointers)?,
    //         unk_search_frequency_time: parse::byte(lines[2])?,
    //         unk_unready_attack_time: parse::byte(lines[3])?,
    //         unk_attack_time: parse::byte(lines[4])?,
    //         unk_retarget_time: parse::word(lines[5])?,
    //         unk_zone_distance: parse::word(lines[6])?,
    //         hardpoints,
    //     });
    // }
}

pub struct HardPoint {}

#[cfg(test)]
extern crate omnilib;

#[cfg(test)]
mod tests {
    use super::*;
    use failure::Error;
    use omnilib::OmniLib;

    #[test]
    fn can_parse_all_npc_types() -> Fallible<()> {
        let omni = OmniLib::new_for_test_in_games(vec!["FA"])?;
        for (game, name) in omni.find_matching("*.[NP]T")?.iter() {
            println!(
                "At: {}:{:13} @ {}",
                game,
                name,
                omni.path(game, name)
                    .or::<Error>(Ok("<none>".to_string()))?
            );
            let lib = omni.library(game);
            let assets = AssetLoader::new(lib)?;
            let contents = omni.library(game).load_text(name)?;
            let nt = NpcType::from_str(&contents, &assets)?;
            assert_eq!(nt.ot.file_name(), *name);
            println!(
                "{}:{:13}> {:?} <> {}",
                game,
                name,
                0, //nt.hardpoints.len(),
                nt.ot.long_name(),
            );
        }
        return Ok(());
    }
}
