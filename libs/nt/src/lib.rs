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
use asset::AssetLoader;
use failure::{ensure, Fallible};
use ot::{make_type_struct, make_validate_field_repr, make_validate_field_type, make_storage_type, make_consume_fields, parse, parse::{FieldRow, FromField}, ObjectType};
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

#[derive(Debug, Ord, PartialOrd, Eq, PartialEq)]
enum HardpointTypeVersion {
    V0,
}

impl HardpointTypeVersion {
    fn from_len(_: usize) -> Fallible<Self> {
        Ok(HardpointTypeVersion::V0)
    }
}

 struct Fueltank(String);
// impl Resource for Fueltank {
//     fn from_file(filename: &str) -> Fallible<Self> {
//         println!("Fueltank: {}", filename);
//         return Ok(Fueltank(filename.to_owned()));
//     }
// }

 struct Sensor(String);
// impl Resource for Sensor {
//     fn from_file(filename: &str) -> Fallible<Self> {
//         println!("Sensor: {}", filename);
//         return Ok(Sensor(filename.to_owned()));
//     }
// }

 struct Ecm(String);
// impl Resource for Ecm {
//     fn from_file(filename: &str) -> Fallible<Self> {
//         println!("Ecm: {}", filename);
//         return Ok(Ecm(filename.to_owned()));
//     }
// }

 struct ProjectileType(String);
// impl Resource for ProjectileType {
//     fn from_file(filename: &str) -> Fallible<Self> {
//         println!("Projectile: {}", filename);
//         return Ok(ProjectileType(filename.to_owned()));
//     }
// }

enum Loadout {
    GAS(Fueltank),
    SEE(Sensor),
    ECM(Ecm),
    JT(ProjectileType),
}

// impl Resource for Loadout {
//     fn from_file(filename: &str) -> Fallible<Self> {
//         let parts = filename.rsplit(".").collect::<Vec<&str>>();
//         return match parts[0] {
//             "SEE" => Ok(Loadout::SEE(Sensor::from_file(filename)?)),
//             "ECM" => Ok(Loadout::ECM(Ecm::from_file(filename)?)),
//             "JT" => Ok(Loadout::JT(ProjectileType::from_file(filename)?)),
//             "GAS" => Ok(Loadout::GAS(Fueltank::from_file(filename)?)),
//             _ => bail!("unknown loadout type: {}", parts[0]),
//         };
//     }
// }

/*
#[allow(dead_code)]
pub struct Hardpoint {
    unk_flags: i16,
    unk1: i16,
    unk2: i16,
    unk3: i16,
    unk4: i16,
    unk5: i16,
    unk6: i16,
    unk7: i16,
    default_loadout: Option<Loadout>,
    unk9: u8,
    unk10: i16,
    unk11: u8,
}

impl Hardpoint {
    pub fn from_lines(lines: &[&str] /*, pointers: &HashMap<&str, Vec<&str>>*/) -> Fallible<Self> {
        return Ok(Hardpoint {
            unk_flags: parse::word(lines[0])?,
            unk1: parse::word(lines[1])?,
            unk2: parse::word(lines[2])?,
            unk3: parse::word(lines[3])?,
            unk4: parse::word(lines[4])?,
            unk5: parse::word(lines[5])?,
            unk6: parse::word(lines[6])?,
            unk7: parse::word(lines[7])?,
            //default_loadout: parse::maybe_load_resource(lines[8], pointers)?,
            default_loadout: None,
            unk9: parse::byte(lines[9])?,
            unk10: parse::word(lines[10])?,
            unk11: parse::byte(lines[11])?,
        });
    }
}
*/

// Wrap Vec<HP> so that we can impl FromField.
pub struct Hardpoints {
    all: Vec<HardpointType>
}

impl FromField for Hardpoints {
    type Produces = Hardpoints;
    fn from_field(field: &FieldRow, pointers: &HashMap<&str, Vec<&str>>, assets: &AssetLoader) -> Fallible<Self::Produces> {
        let (name, lines) = field.value().pointer()?;
        let mut off = 0usize;
        let mut hards = Vec::new();
        ensure!(lines.len() % 12 == 0, "expected 12 lines per hardpoint");
        while off < lines.len() {
            let lns = lines[off..off + 12].iter().map(|v| v.as_ref()).collect::<Vec<_>>();
            let ht = HardpointType::from_lines((), &lns, pointers, assets)?;
            hards.push(ht);
            off += 12;
        }
        return Ok(Hardpoints { all: hards });
        /*
        ensure!(name == "hards", "expected pointer to hards");
        for line in values {
            let row = FieldRow::from_line(&line, pointers)?;
            println!("Got ROW: {:?}", row);
        }
        unimplemented!();
        */
    }
}

pub struct HardpointDefault {
    name: Option<String>
}

impl FromField for HardpointDefault {
    type Produces = HardpointDefault;
    fn from_field(field: &FieldRow, _pointers: &HashMap<&str, Vec<&str>>, _assets: &AssetLoader) -> Fallible<Self::Produces> {
        if !field.value().pointer().is_ok() {
            ensure!(
                field.value().numeric()?.dword()? == 0u32,
                "null pointer must be dword 0"
            );
            Ok(HardpointDefault { name: None })
        } else {
            let (sym, values) = field.value().pointer()?;
            ensure!(sym.starts_with("defaultTypeName"), "expected defaultTypeName in ptr name");
            let name = ot::parse::string(&values[0])?.to_uppercase();
            Ok(HardpointDefault { name: Some(name) })
        }
    }
}

make_type_struct![
HardpointType(parent: (), version: HardpointTypeVersion) {
    (Word, [Hex], "", Unsigned, flags,           u16,    V0, panic!()), // word $8
    (Word, [Dec], "", Unsigned, unk1,            u16,    V0, panic!()), // word 0
    (Word, [Dec], "", Unsigned, unk2,            u16,    V0, panic!()), // word 30
    (Word, [Dec], "", Unsigned, unk3,            u16,    V0, panic!()), // word 0
    (Word, [Dec], "", Unsigned, unk4,            u16,    V0, panic!()), // word 0
    (Word, [Dec], "", Unsigned, unk5,            u16,    V0, panic!()), // word 0
    (Word, [Dec], "", Unsigned, unk6,            u16,    V0, panic!()), // word 0
    (Word, [Dec], "", Unsigned, unk7,            u16,    V0, panic!()), // word 16380
    (Ptr,  [Dec, Sym], "", Struct,   default_loadout, HardpointDefault, V0, panic!()), // ptr defaultTypeName0
    (Byte, [Dec], "", Unsigned, unk9,            u8,     V0, panic!()), // byte 0
    (Word, [Dec], "", Unsigned, unk10,           u16,    V0, panic!()), // word 32767
    (Byte, [Dec], "", Unsigned, unk11,           u8,     V0, panic!())  // byte 0
}];

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
    pub fn from_str(
        data: &str,
        assets: &AssetLoader,
    ) -> Fallible<Self> {
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
            println!("At: {}:{:13} @ {}", game, name, omni.path(game, name).or::<Error>(Ok("<none>".to_string()))?);
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
