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
extern crate bitflags;
extern crate entity;
#[macro_use]
extern crate failure;

use entity::{parse, TypeTag};
use failure::Error;
use std::mem;
use std::collections::HashMap;


pub struct Shape {}
impl Shape {
    fn new(_: &str) -> Result<Self, Error> {
        Ok(Shape {})
    }
}
pub struct Sound {}

#[derive(Debug)]
enum ObjectKind {
    Fighter    = 0b1000_0000_0000_0000,
    Bomber     = 0b0100_0000_0000_0000,
    Ship       = 0b0010_0000_0000_0000,
    SAM        = 0b0001_0000_0000_0000,
    AAA        = 0b0000_1000_0000_0000,
    Tank       = 0b0000_0100_0000_0000,
    Vehicle    = 0b0000_0010_0000_0000,
    Structure1  = 0b0000_0001_0000_0000,
    Projectile = 0b0000_0000_1000_0000,
    Structure2 = 0b0000_0000_0100_0000,
}

impl ObjectKind {
    fn new(x: u16) -> Result<Self, Error> {
        return match x {
            0b1000_0000_0000_0000 => Ok(ObjectKind::Fighter),
            0b0100_0000_0000_0000 => Ok(ObjectKind::Bomber),
            0b0010_0000_0000_0000 => Ok(ObjectKind::Ship),
            0b0001_0000_0000_0000 => Ok(ObjectKind::SAM),
            0b0000_1000_0000_0000 => Ok(ObjectKind::AAA),
            0b0000_0100_0000_0000 => Ok(ObjectKind::Tank),
            0b0000_0010_0000_0000 => Ok(ObjectKind::Vehicle),
            0b0000_0001_0000_0000 => Ok(ObjectKind::Structure1),
            0b0000_0000_1000_0000 => Ok(ObjectKind::Projectile),
            0b0000_0000_0100_0000 => Ok(ObjectKind::Structure2),
            _ => bail!("unknown ObjectKind {}", x)
        };
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
    fn new(s: &str) -> Result<ProcKind, Error> {
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
            _ => bail!("Unexpected proc kind: {}", parts[1]),
        });
    }
}

bitflags! {
    struct ObjectFlags : u32 {
        const UNK0     = 0b0000_1000_0000_0000_0000_0000_0000_0000;
        const UNK1     = 0b0000_0100_0000_0000_0000_0000_0000_0000;
        const UNK2     = 0b0000_0010_0000_0000_0000_0000_0000_0000;
        const UNK3     = 0b0000_0001_0000_0000_0000_0000_0000_0000;
        const FLYABLE  = 0b0000_0000_0000_0000_0100_0000_0000_0000;
        const UNK4     = 0b0000_0000_0000_0000_0010_0000_0000_0000;
        const UNK5     = 0b0000_0000_0000_0000_0000_1000_0000_0000;
        const UNK6     = 0b0000_0000_0000_0000_0000_0010_0000_0000;
        const UNK7     = 0b0000_0000_0000_0000_0000_0001_0000_0000;
        const UNK8     = 0b0000_0000_0000_0000_0000_0000_1000_0000;
        const UNK9     = 0b0000_0000_0000_0000_0000_0000_0100_0000;
        const UNK10    = 0b0000_0000_0000_0000_0000_0000_0010_0000;
        const UNK11    = 0b0000_0000_0000_0000_0000_0000_0001_0000;
        const UNK12    = 0b0000_0000_0000_0000_0000_0000_0000_0010;
        const UNK13    = 0b0000_0000_0000_0000_0000_0000_0000_0001;
    }
}

impl ObjectFlags {
    fn from_u32(f: u32) -> ObjectFlags {
        unsafe { mem::transmute(f) }
    }
}

#[allow(dead_code)]
pub struct ObjectType {
    //;---------------- general info ----------------
    type_tag: TypeTag,
    unk_type_size: i16,
    unk_instance_size: i16,
    short_name: String,
    long_name: String,
    file_name: String,
    flags: ObjectFlags,
    kind: ObjectKind,
    shape: Option<Shape>,
    shadow_shape: Option<Shape>,
    unk8: u32,
    unk9: u32,
    unk_damage_debris_pos: [i16; 3],
    unk13: u32,
    unk14: u32,
    unk_destination_debris_pos: [i16; 3],
    unk_damage_type: u32,
    year_available: u32,
    unk_max_visual_distance: i16,
    unk_camera_distance: i16,
    unk22: i16,
    unk_laser_signature: i16,
    unk_ir_signature: i16,
    unk_radar_signature: i16,
    unk26: i16,
    unk_health: i16,
    unk_damage_on_planes: i16,
    unk_damage_on_ships: i16,
    unk_damage_on_structures: i16,
    unk_damage_on_armor: i16,
    unk_damage_on_other: i16,
    unk_explosion_type: u8,
    unk_crater_size_ft: u8,
    unk_empty_weight: u32,
    unk_command_buffer_size: i16,

    //;---------------- movement info ----------------
    unk_turn_rate: i16,
    unk_bank_rate: i16, // degrees per second / 182?
    unk_max_climb: i16,
    unk_max_dive: i16,
    unk_max_bank: i16,
    unk_min_speed: i16,
    unk_corner_speed: i16,
    unk_max_speed: i16,
    unk_acceleration: u32,
    unk_deceleration: u32,
    unk_min_altitude: u32, // in feet?
    unk_max_altitude: u32,
    util_proc: ProcKind,

    //;---------------- sound info ----------------
    loop_sound: Sound,
    second_sound: Sound,
    engine_on_sound: Sound,
    engine_off_sound: Sound,
    unk_do_doppler: u8,
    unk_sound_radius: i16, // in feet?
    unk_max_doppler_pitch_up: i16,
    max_doppler_pitch_down: i16,
    min_doppler_speed: i16,
    max_doppler_speed: i16,
    unk_rear_view_pos: [i16; 3],
    hud_name: String,
}

impl ObjectType {
    fn name_at(i: usize, pointers: &HashMap<&str, Vec<&str>>) -> Result<String, Error> {
        return match pointers[":ot_names"].get(i) {
            None => bail!("expected a name at position {}", i),
            Some(s) => Ok(parse::string(s)?),
        };
    }

    pub fn from_str(data: &str) -> Result<Self, Error> {
        let all_lines = data.lines().collect::<Vec<&str>>();
        ensure!(
            all_lines[0] == "[brent's_relocatable_format]",
            "not an type file"
        );

        let pointers = entity::find_pointers(&all_lines)?;
        let lines = entity::find_section(&all_lines, "OBJ_TYPE")?;

        return Ok(ObjectType {
            type_tag: TypeTag::new(parse::byte(lines[0])?)?,
            unk_type_size: parse::word(lines[1])?,
            unk_instance_size: parse::word(lines[2])?,
            short_name: Self::name_at(0, &pointers)?,
            long_name: Self::name_at(1, &pointers)?,
            file_name: Self::name_at(2, &pointers)?,
            flags: ObjectFlags::from_u32(parse::dword(lines[4])?),
            kind: ObjectKind::new(parse::word(lines[5])? as u16)?,
            shape: pointers.get(":shape").and_then(|l| Shape::new(l[0]).ok()),
            shadow_shape: pointers
                .get(":shadowShape")
                .and_then(|l| Shape::new(l[0]).ok()),
            unk8: parse::dword(lines[8])?,
            unk9: parse::dword(lines[9])?,
            unk_damage_debris_pos:
            [
                parse::word(lines[10])?,
                parse::word(lines[11])?,
                parse::word(lines[12])?,
            ],
            unk13: parse::dword(lines[13])?,
            unk14: parse::dword(lines[14])?,
            unk_destination_debris_pos: [
                parse::word(lines[15])?,
                parse::word(lines[16])?,
                parse::word(lines[17])?,
            ],
            unk_damage_type: parse::dword(lines[18])?,
            year_available: parse::dword(lines[19])?,
            unk_max_visual_distance: parse::word(lines[20])?,
            unk_camera_distance: parse::word(lines[21])?,
            unk22: parse::word(lines[22])?,
            unk_laser_signature: parse::word(lines[23])?,
            unk_ir_signature: parse::word(lines[24])?,
            unk_radar_signature: parse::word(lines[25])?,
            unk26: parse::word(lines[26])?,
            unk_health: parse::word(lines[27])?,
            unk_damage_on_planes: parse::word(lines[28])?,
            unk_damage_on_ships: parse::word(lines[29])?,
            unk_damage_on_structures: parse::word(lines[30])?,
            unk_damage_on_armor: parse::word(lines[31])?,
            unk_damage_on_other: parse::word(lines[32])?,
            unk_explosion_type: parse::byte(lines[33])?,
            unk_crater_size_ft: parse::byte(lines[34])?,
            unk_empty_weight: parse::dword(lines[35])?,
            unk_command_buffer_size: parse::word(lines[36])?,

            //;---------------- movement info ----------------
            unk_turn_rate: parse::word(lines[37])?,
            unk_bank_rate: parse::word(lines[38])?,
            unk_max_climb: parse::word(lines[39])?,
            unk_max_dive: parse::word(lines[40])?,
            unk_max_bank: parse::word(lines[41])?,
            unk_min_speed: parse::word(lines[42])?,
            unk_corner_speed: parse::word(lines[43])?,
            unk_max_speed: parse::word(lines[44])?,
            unk_acceleration: parse::dword(lines[45])?,
            unk_deceleration: parse::dword(lines[46])?,
            unk_min_altitude: parse::dword(lines[47])?,
            unk_max_altitude: parse::dword(lines[48])?,
            util_proc: ProcKind::new(lines[49])?,

            //;---------------- sound info ----------------
            loop_sound: Sound {},
            second_sound: Sound {},
            engine_on_sound: Sound {},
            engine_off_sound: Sound {},
            unk_do_doppler: parse::byte(lines[54])?,
            unk_sound_radius: parse::word(lines[55])?,
            unk_max_doppler_pitch_up: parse::word(lines[56])?,
            max_doppler_pitch_down: parse::word(lines[57])?,
            min_doppler_speed: parse::word(lines[58])?,
            max_doppler_speed: parse::word(lines[59])?,
            unk_rear_view_pos: [
                parse::word(lines[60])?,
                parse::word(lines[61])?,
                parse::word(lines[62])?,
            ],
            hud_name: String::new(),
        });
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::prelude::*;
    use super::*;

    #[test]
    fn can_parse_all_entity_types() {
        let mut rv = vec![];
        let paths = fs::read_dir("./test_data").unwrap();
        for i in paths {
            let entry = i.unwrap();
            let path = format!("{}", entry.path().display());
            let mut fp = fs::File::open(entry.path()).unwrap();
            let mut contents = String::new();
            fp.read_to_string(&mut contents).unwrap();
            println!("At: {}", path);
            let ot = ObjectType::from_str(&contents).unwrap();
            assert_eq!(format!("./test_data/{}", ot.file_name), path);
            rv.push(format!("{:?} <> {} <> {}",
                            ot.unk_explosion_type,
                            ot.long_name, path));
        }
        rv.sort();
        for v in rv {
            println!("{}", v);
        }
    }
}
