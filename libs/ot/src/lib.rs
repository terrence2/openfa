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
#[macro_use]
extern crate failure;
extern crate nalgebra;
extern crate num_traits;
extern crate sh;

#[macro_use]
pub mod parse;

use failure::Fallible;
use nalgebra::Point3;
use num_traits::Num;
pub use parse::{
    check_num_type, consume_obj_class, consume_ptr, parse_one, FieldType, Repr, Resource,
    TryConvert,
};
//use sh::CpuShape as Shape;
use std::{collections::HashMap, mem};

#[derive(Debug)]
#[repr(u8)]
pub enum TypeTag {
    Object = 1,
    NPC = 3,
    Plane = 5,
    Projectile = 7,
}

impl TypeTag {
    pub fn new(n: u8) -> Fallible<TypeTag> {
        if n != 1 && n != 3 && n != 5 && n != 7 {
            bail!("unknown TypeTag {}", n);
        }
        return Ok(unsafe { mem::transmute(n) });
    }
}

pub struct Shape {}
impl Resource for Shape {
    fn from_file(_: &str) -> Fallible<Self> {
        Ok(Shape {})
    }
}
pub struct HUD {}
impl Resource for HUD {
    fn from_file(_: &str) -> Fallible<Self> {
        Ok(HUD {})
    }
}
pub struct Sound {}
impl Resource for Sound {
    fn from_file(_: &str) -> Fallible<Self> {
        Ok(Sound {})
    }
}

#[derive(Debug)]
enum ObjectKind {
    Fighter = 0b1000_0000_0000_0000,
    Bomber = 0b0100_0000_0000_0000,
    Ship = 0b0010_0000_0000_0000,
    SAM = 0b0001_0000_0000_0000,
    AAA = 0b0000_1000_0000_0000,
    Tank = 0b0000_0100_0000_0000,
    Vehicle = 0b0000_0010_0000_0000,
    Structure1 = 0b0000_0001_0000_0000,
    Projectile = 0b0000_0000_1000_0000,
    Structure2 = 0b0000_0000_0100_0000,
}

impl ObjectKind {
    fn new(x: u16) -> Fallible<Self> {
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
            // There is a mistaken 0 entry in $BLDR.JT when it was first introduced in ATF Nato Fighters.
            0b0000_0000_0000_0000 => Ok(ObjectKind::Projectile),
            _ => bail!("unknown ObjectKind {}", x),
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
    fn new(s: &str) -> Fallible<ProcKind> {
        return Ok(match s {
            "_OBJProc" => ProcKind::OBJ,
            "_PLANEProc" => ProcKind::PLANE,
            "_CARRIERProc" => ProcKind::CARRIER,
            "_GVProc" => ProcKind::GV,
            "_PROJProc" => ProcKind::PROJ,
            "_EJECTProc" => ProcKind::EJECT,
            "_STRIPProc" => ProcKind::STRIP,
            "_CATGUYProc" => ProcKind::CATGUY,
            _ => bail!("Unexpected proc kind: {}", s),
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

struct ObjectNames {
    short_name: String,
    long_name: String,
    file_name: String,
}
impl TryConvert<u8> for TypeTag {
    type Error = failure::Error;
    fn try_from(value: u8) -> Fallible<TypeTag> {
        TypeTag::new(value)
    }
}

impl TryConvert<u16> for ObjectKind {
    type Error = failure::Error;
    fn try_from(value: u16) -> Fallible<ObjectKind> {
        ObjectKind::new(value)
    }
}

impl<'a> TryConvert<Vec<String>> for ObjectNames {
    type Error = failure::Error;
    fn try_from(value: Vec<String>) -> Fallible<ObjectNames> {
        ensure!(value.len() == 3, "expected 3 names in ot_names");
        return Ok(ObjectNames {
            short_name: parse::string(&value[0])?,
            long_name: parse::string(&value[1])?,
            file_name: parse::string(&value[2])?,
        });
    }
}

impl<'a> TryConvert<Vec<String>> for Option<Shape> {
    type Error = failure::Error;
    fn try_from(value: Vec<String>) -> Fallible<Option<Shape>> {
        ensure!(value.len() <= 1, "expected 0 or 1 names in shape");
        if value.len() > 0 {
            return Ok(Some(Shape::from_file(&value[0])?));
        }
        return Ok(None);
    }
}

impl<'a> TryConvert<Vec<String>> for Option<Sound> {
    type Error = failure::Error;
    fn try_from(value: Vec<String>) -> Fallible<Option<Sound>> {
        ensure!(value.len() <= 1, "expected 0 or 1 names in sound");
        if value.len() > 0 {
            return Ok(Some(Sound::from_file(&value[0])?));
        }
        return Ok(None);
    }
}

impl<'a> TryConvert<Vec<String>> for Option<HUD> {
    type Error = failure::Error;
    fn try_from(value: Vec<String>) -> Fallible<Option<HUD>> {
        ensure!(value.len() <= 1, "expected 0 or 1 names in sound");
        if value.len() > 0 {
            return Ok(Some(HUD::from_file(&value[0])?));
        }
        return Ok(None);
    }
}

impl TryConvert<[i16; 3]> for Point3<f32> {
    type Error = failure::Error;
    fn try_from(value: [i16; 3]) -> Fallible<Point3<f32>> {
        return Ok(Point3::new(
            value[0] as f32,
            value[1] as f32,
            value[2] as f32,
        ));
    }
}

impl<'a> TryConvert<&'a str> for ProcKind {
    type Error = failure::Error;
    fn try_from(value: &'a str) -> Fallible<ProcKind> {
        return ProcKind::new(value);
    }
}

// We can detect the version by the number of lines.
#[derive(Debug, Ord, PartialOrd, Eq, PartialEq)]
enum ObjectTypeVersion {
    V0 = 49, // USNF only
    V1 = 51, // MF only
    V2 = 63, // ATF & USNF97
    V3 = 64, // Nato, Gold, & FA
}

impl ObjectTypeVersion {
    fn from_len(n: usize) -> Fallible<Self> {
        return Ok(match n {
            49 => ObjectTypeVersion::V0,
            51 => ObjectTypeVersion::V1,
            63 => ObjectTypeVersion::V2,
            64 => ObjectTypeVersion::V3,
            _ => bail!("unknown object type version for length: {}", n),
        });
    }
}

make_type_struct![
ObjectType(parent: (), version: ObjectTypeVersion) {
    (struct_type,           TypeTag, "structType",           (Dec: u8), V0, panic!()),
    (type_size,               usize, "typeSize",            (Dec: u16), V0, panic!()),
    (instance_size,           usize, "instanceSize",        (Dec: u16), V0, panic!()),
    (ot_names,          ObjectNames, "ot_names",                   Ptr, V0, panic!()),
    (flags,                     u32, "flags",          ([Dec,Hex]:u32), V0, panic!()),
    (obj_class,          ObjectKind, "obj_class",             ObjClass, V0, panic!()),
    (shape,           Option<Shape>, "shape",                      Ptr, V0, panic!()),
    (shadow_shape,    Option<Shape>, "shadowShape",                Ptr, V0, panic!()),
    (unk8,                      u32, "",                    (Dec: u32), V2, 0),
    (unk9,                      u32, "",                    (Dec: u32), V2, 0),
    (dmg_debris_pos,    Point3<f32>, "dmgDebrisPos.",      [Vec3: i16], V2, Point3::new(0f32, 0f32, 0f32)),
    (unk13,                     u32, "",                    (Dec: u32), V2, 0),
    (unk14,                     u32, "",                    (Dec: u32), V2, 0),
    (dst_debris_pos,    Point3<f32>, "dstDebrisPos.",      [Vec3: i16], V2, Point3::new(0f32, 0f32, 0f32)),
    (dmg_type,                usize, "dmgType",             (Dec: u32), V2, 0),
    (year_available,          usize, "year",                (Dec: u32), V3, usize::max_value()),
    (max_vis_dist,              f32, "maxVisDist",          (Dec: u16), V0, panic!()),
    (camera_dist,               f32, "cameraDist",          (Dec: u16), V0, panic!()),
    (unk_sig_22,                u16, "sigs [i]",            (Dec: u16), V0, panic!()),
    (unk_sig_laser,             u16, "sigs [i]",            (Dec: u16), V0, panic!()),
    (unk_sig_ir,                u16, "sigs [i]",            (Dec: u16), V0, panic!()),
    (unk_sig_radar,             u16, "sigs [i]",            (Dec: u16), V0, panic!()),
    (unk_sig_26,                u16, "sigs [i]",            (Dec: u16), V0, panic!()),
    (hit_points,                u16, "hitPoints",           (Dec: u16), V0, panic!()),
    (damage_on_planes,          u16, "damage [i]",          (Dec: u16), V0, panic!()),
    (damage_on_ships,           u16, "damage [i]",          (Dec: u16), V0, panic!()),
    (damage_on_structures,      u16, "damage [i]",          (Dec: u16), V0, panic!()),
    (damage_on_armor,           u16, "damage [i]",          (Dec: u16), V0, panic!()),
    (damage_on_other,           u16, "damage [i]",          (Dec: u16), V0, panic!()),
    (explosion_type,             u8, "expType",             (Dec:  u8), V0, panic!()),
    (crater_size,/*ft?*/         u8, "craterSize",          (Dec:  u8), V0, panic!()),
    (empty_weight,              u32, "weight",              (Dec: u32), V0, panic!()),
    (cmd_buf_size,              u16, "cmdBufSize",          (Dec: u16), V0, panic!()),
    // // Movement Info
    (turn_rate,                 u16, "_turnRate",           (Dec: u16), V0, panic!()),
    (bank_rate,                 u16, "_bankRate",           (Dec: u16), V0, panic!()), // degrees per second / 182?
    (max_climb,                 i16, "maxClimb",            (Dec: i16), V0, panic!()),
    (max_dive,                  i16, "maxDive",             (Dec: i16), V0, panic!()),
    (max_bank,                  i16, "maxBank",             (Dec: i16), V0, panic!()),
    (min_speed,                 u16, "_minSpeed",           (Dec: u16), V0, panic!()),
    (corner_speed,              u16, "_cornerSpeed",        (Dec: u16), V0, panic!()),
    (max_speed,                 u16, "_maxSpeed",           (Dec: u16), V0, panic!()),
    (acceleration,              u32, "_acc",           ([Dec,Car]:u32), V0, panic!()),
    (deceleration,              u32, "_dacc",          ([Dec,Car]:u32), V0, panic!()),
    (min_altitude,              i32, "minAlt",         ([Dec,Car]:i32), V0, panic!()), // in feet?
    (max_altitude,              i32, "maxAlt",     ([Dec,Hex,Car]:i32), V0, panic!()),
    (util_proc,            ProcKind, "utilProc",                Symbol, V0, panic!()),
    // Sound Info
    (loop_sound,      Option<Sound>, "loopSound",                  Ptr, V0, panic!()),
    (second_sound,    Option<Sound>, "secondSound",                Ptr, V0, panic!()),
    (engine_on_sound, Option<Sound>, "engineOnSound",              Ptr, V1, None), // TODO: figure out what the default was in USNF.
    (engine_off_sound,Option<Sound>, "engineOffSound",             Ptr, V1, None),
    (do_doppler,               bool, "doDoppler",           (Dec:  u8), V0, panic!()),
    (max_snd_dist,              u16, "maxSndDist",          (Dec: u16), V0, panic!()), // in feet?
    (max_plus_doppler_pitch,    i16, "maxPlusDopplerPitch", (Dec: i16), V0, panic!()),
    (max_minus_doppler_pitch,   i16, "maxMinusDopplerPitch",(Dec: i16), V0, panic!()),
    (min_doppler_speed,         i16, "minDopplerSpeed",     (Dec: i16), V0, panic!()),
    (max_doppler_speed,         i16, "maxDopplerSpeed",     (Dec: i16), V0, panic!()),
    (unk_rear_view_pos, Point3<f32>, "viewOffset.",         [Vec3:i16], V0, panic!()),
    // FIXME: looks like we need to specialize the hud source somehow... it is
    // not set in the older games for some of the main planes; it's probably
    // assuming the $name.HUD.
    (hud,               Option<HUD>,             "hudName", Ptr,        V2, None)
}];

impl ObjectType {
    pub fn from_str(data: &str) -> Fallible<Self> {
        let lines = data.lines().collect::<Vec<&str>>();
        ensure!(
            lines[0] == "[brent's_relocatable_format]",
            "not a type file"
        );
        let pointers = parse::find_pointers(&lines)?;
        let obj_lines = parse::find_section(&lines, "OBJ_TYPE")?;
        return Self::from_lines((), &obj_lines, &pointers);
    }

    fn load_shape(line: &str, pointers: &HashMap<&str, Vec<&str>>) -> Fallible<Option<Shape>> {
        let filename = parse::maybe_resource_filename(line, pointers)?;
        return Ok(match filename {
            None => None,
            Some(f) => {
                let resource = Shape::from_file(&f)?;
                Some(resource)
            }
        });
    }

    pub fn file_name(&self) -> &str {
        return &self.ot_names.file_name;
    }
}

#[cfg(test)]
extern crate omnilib;

#[cfg(test)]
mod tests {
    use super::*;
    use omnilib::OmniLib;

    #[test]
    fn can_parse_all_entity_types() -> Fallible<()> {
        let omni = OmniLib::new_for_test_in_games(vec![
            "FA", "ATF", "ATFGOLD", "ATFNATO", "USNF", "MF", "USNF97",
        ])?;
        for (game, name) in omni.find_matching("*.[OJNP]T")?.iter() {
            println!("At: {}:{:13} @ {}", game, name, omni.path(game, name)?);
            let contents = omni.library(game).load_text(name)?;
            let ot = ObjectType::from_str(&contents)?;
            // Only one misspelling in 2.5e3 files.
            assert!(ot.file_name() == *name || *name == "SMALLARM.JT");
            // println!(
            //     "{}:{:13}> {:?} <> {}",
            //     game, name, ot.explosion_type, ot.long_name
            // );
        }
        return Ok(());
    }
}
