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
struct AI(String);
impl Resource for AI {
    fn from_file(filename: &str) -> Fallible<Self> {
        return Ok(AI(filename.to_owned()));
    }
}

struct Fueltank(String);
impl Resource for Fueltank {
    fn from_file(filename: &str) -> Fallible<Self> {
        println!("Fueltank: {}", filename);
        return Ok(Fueltank(filename.to_owned()));
    }
}

struct Sensor(String);
impl Resource for Sensor {
    fn from_file(filename: &str) -> Fallible<Self> {
        println!("Sensor: {}", filename);
        return Ok(Sensor(filename.to_owned()));
    }
}

struct Ecm(String);
impl Resource for Ecm {
    fn from_file(filename: &str) -> Fallible<Self> {
        println!("Ecm: {}", filename);
        return Ok(Ecm(filename.to_owned()));
    }
}

struct ProjectileType(String);
impl Resource for ProjectileType {
    fn from_file(filename: &str) -> Fallible<Self> {
        println!("Projectile: {}", filename);
        return Ok(ProjectileType(filename.to_owned()));
    }
}

enum Loadout {
    GAS(Fueltank),
    SEE(Sensor),
    ECM(Ecm),
    JT(ProjectileType),
}

impl Resource for Loadout {
    fn from_file(filename: &str) -> Fallible<Self> {
        let parts = filename.rsplit(".").collect::<Vec<&str>>();
        return match parts[0] {
            "SEE" => Ok(Loadout::SEE(Sensor::from_file(filename)?)),
            "ECM" => Ok(Loadout::ECM(Ecm::from_file(filename)?)),
            "JT" => Ok(Loadout::JT(ProjectileType::from_file(filename)?)),
            "GAS" => Ok(Loadout::GAS(Fueltank::from_file(filename)?)),
            _ => bail!("unknown loadout type: {}", parts[0]),
        };
    }
}

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
    pub fn from_lines(lines: &[&str], pointers: &HashMap<&str, Vec<&str>>) -> Fallible<Self> {
        return Ok(Hardpoint {
            unk_flags: parse::word(lines[0])?,
            unk1: parse::word(lines[1])?,
            unk2: parse::word(lines[2])?,
            unk3: parse::word(lines[3])?,
            unk4: parse::word(lines[4])?,
            unk5: parse::word(lines[5])?,
            unk6: parse::word(lines[6])?,
            unk7: parse::word(lines[7])?,
            default_loadout: parse::maybe_load_resource(lines[8], pointers)?,
            unk9: parse::byte(lines[9])?,
            unk10: parse::word(lines[10])?,
            unk11: parse::byte(lines[11])?,
        });
    }
}

#[allow(dead_code)]
pub struct NpcType {
    pub obj: ObjectType,

    unk0: u32,
    behavior: Option<AI>,
    unk_search_frequency_time: u8,
    unk_unready_attack_time: u8,
    unk_attack_time: u8,
    unk_retarget_time: i16,
    unk_zone_distance: i16,
    hardpoints: Vec<Hardpoint>,
}

impl NpcType {
    pub fn from_str(data: &str) -> Fallible<Self> {
        let lines = data.lines().collect::<Vec<&str>>();
        ensure!(
            lines[0] == "[brent's_relocatable_format]",
            "not a type file"
        );
        let pointers = parse::find_pointers(&lines)?;
        return Self::from_lines(&lines, &pointers);
    }

    pub fn from_lines(lines: &Vec<&str>, pointers: &HashMap<&str, Vec<&str>>) -> Fallible<Self> {
        let obj = ObjectType::from_lines(lines, pointers)?;
        let lines = parse::find_section(&lines, "NPC_TYPE")?;

        let hardpoint_count = parse::byte(lines[7])? as usize;
        let hardpoint_lines = parse::follow_pointer(lines[8], pointers)?;
        let mut hardpoints = Vec::new();
        for chunk in hardpoint_lines.chunks(12) {
            hardpoints.push(Hardpoint::from_lines(chunk, pointers)?);
        }
        ensure!(
            hardpoint_count == hardpoints.len(),
            "wrong number of hardpoints"
        );

        return Ok(NpcType {
            obj,
            unk0: parse::dword(lines[0])?,
            behavior: parse::maybe_load_resource(lines[1], pointers)?,
            unk_search_frequency_time: parse::byte(lines[2])?,
            unk_unready_attack_time: parse::byte(lines[3])?,
            unk_attack_time: parse::byte(lines[4])?,
            unk_retarget_time: parse::word(lines[5])?,
            unk_zone_distance: parse::word(lines[6])?,
            hardpoints,
        });
    }
}

pub struct HardPoint {}

#[cfg(test)]
extern crate omnilib;

#[cfg(test)]
mod tests {
    use super::*;
    use omnilib::OmniLib;

    #[test]
    fn can_parse_all_npc_types() -> Fallible<()> {
        let omni = OmniLib::new_for_test_in_games(vec!["FA"])?;
        for (game, name) in omni.find_matching("*.[NP]T")?.iter() {
            let contents = omni.library(game).load_text(name)?;
            let nt = NpcType::from_str(&contents)?;
            assert_eq!(nt.obj.file_name, *name);
            println!(
                "{}:{:13}> {:?} <> {}",
                game,
                name,
                nt.hardpoints.len(),
                nt.obj.long_name,
            );
        }
        return Ok(());
    }
}
