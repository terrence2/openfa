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
pub mod parse;

pub use crate::parse::{parse_string, FieldRow, FieldType, FromRow, Repr};
use absolute_unit::{PoundsWeight, Weight};
use anyhow::{bail, ensure, Result};
use bitflags::bitflags;
use nalgebra::Point3;
use std::{collections::HashMap, fmt, mem};

#[derive(Clone, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum TypeTag {
    Object = 1,
    Npc = 3,
    Plane = 5,
    Projectile = 7,
}

impl TypeTag {
    pub fn from_byte(n: u8) -> Result<TypeTag> {
        if n != 1 && n != 3 && n != 5 && n != 7 {
            bail!("unknown TypeTag {}", n);
        }
        Ok(unsafe { mem::transmute(n) })
    }
}

impl FromRow for TypeTag {
    type Produces = TypeTag;
    fn from_row(field: &FieldRow, _pointers: &HashMap<&str, Vec<&str>>) -> Result<Self::Produces> {
        TypeTag::from_byte(field.value().numeric()?.byte()?)
    }
}

impl fmt::Display for TypeTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ObjectKind {
    Fighter = 0b1000_0000_0000_0000,
    Bomber = 0b0100_0000_0000_0000,
    Ship = 0b0010_0000_0000_0000,
    Sam = 0b0001_0000_0000_0000,
    Aaa = 0b0000_1000_0000_0000,
    Tank = 0b0000_0100_0000_0000,
    Vehicle = 0b0000_0010_0000_0000,
    Structure1 = 0b0000_0001_0000_0000,
    Projectile = 0b0000_0000_1000_0000,
    Structure2 = 0b0000_0000_0100_0000,
}

impl ObjectKind {
    fn from_word(x: u16) -> Result<Self> {
        match x {
            0b1000_0000_0000_0000 => Ok(ObjectKind::Fighter),
            0b0100_0000_0000_0000 => Ok(ObjectKind::Bomber),
            0b0010_0000_0000_0000 => Ok(ObjectKind::Ship),
            0b0001_0000_0000_0000 => Ok(ObjectKind::Sam),
            0b0000_1000_0000_0000 => Ok(ObjectKind::Aaa),
            0b0000_0100_0000_0000 => Ok(ObjectKind::Tank),
            0b0000_0010_0000_0000 => Ok(ObjectKind::Vehicle),
            0b0000_0001_0000_0000 => Ok(ObjectKind::Structure1),
            0b0000_0000_1000_0000 => Ok(ObjectKind::Projectile),
            0b0000_0000_0100_0000 => Ok(ObjectKind::Structure2),
            // There is a mistaken 0 entry in $BLDR.JT when it was first introduced in ATF Nato Fighters.
            0b0000_0000_0000_0000 => Ok(ObjectKind::Projectile),
            _ => bail!("unknown ObjectKind {}", x),
        }
    }
}

impl fmt::Display for ObjectKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl FromRow for ObjectKind {
    type Produces = ObjectKind;
    fn from_row(field: &FieldRow, _pointers: &HashMap<&str, Vec<&str>>) -> Result<Self::Produces> {
        ObjectKind::from_word(field.value().numeric()?.word()?)
    }
}

#[allow(clippy::upper_case_acronyms)]
#[derive(Clone, Debug, Eq, PartialEq)]
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
    fn from_symbol(s: &str) -> Result<ProcKind> {
        Ok(match s {
            "_OBJProc" => ProcKind::OBJ,
            "_PLANEProc" => ProcKind::PLANE,
            "_CARRIERProc" => ProcKind::CARRIER,
            "_GVProc" => ProcKind::GV,
            "_PROJProc" => ProcKind::PROJ,
            "_EJECTProc" => ProcKind::EJECT,
            "_STRIPProc" => ProcKind::STRIP,
            "_CATGUYProc" => ProcKind::CATGUY,
            _ => bail!("Unexpected proc kind: {}", s),
        })
    }
}

impl fmt::Display for ProcKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl FromRow for ProcKind {
    type Produces = ProcKind;
    fn from_row(field: &FieldRow, _pointers: &HashMap<&str, Vec<&str>>) -> Result<Self::Produces> {
        ProcKind::from_symbol(&field.value().symbol()?)
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

#[derive(Debug, Eq, PartialEq)]
pub struct ObjectNames {
    pub short_name: String,
    pub long_name: String,
    pub file_name: String,
}

impl FromRow for ObjectNames {
    type Produces = ObjectNames;
    fn from_row(field: &FieldRow, _pointers: &HashMap<&str, Vec<&str>>) -> Result<ObjectNames> {
        let (name, values) = field.value().pointer()?;
        ensure!(name == "ot_names", "expected pointer to ot_names");
        Ok(ObjectNames {
            short_name: parse::parse_string(&values[0])?,
            long_name: parse::parse_string(&values[1])?,
            file_name: parse::parse_string(&values[2])?,
        })
    }
}

impl fmt::Display for ObjectNames {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} ({})[{}]",
            self.short_name, self.long_name, self.file_name
        )
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
    fn from_len(n: usize) -> Result<Self> {
        Ok(match n {
            49 => ObjectTypeVersion::V0,
            51 => ObjectTypeVersion::V1,
            63 => ObjectTypeVersion::V2,
            64 => ObjectTypeVersion::V3,
            _ => bail!("unknown object type version for length: {}", n),
        })
    }
}

make_type_struct![
ObjectType(parent: (), version: ObjectTypeVersion) {
    (Byte,  [Dec],          "structType",   Custom, struct_type,          TypeTag, V0, panic!()), // byte 1 ; structType
    (Word,  [Dec],            "typeSize", Unsigned, type_size,                u16, V0, panic!()), // word 166 ; typeSize
    (Word,  [Dec],        "instanceSize", Unsigned, instance_size,            u16, V0, panic!()), // word 0 ; instanceSize
    (Ptr,   [Sym],            "ot_names",   Custom, ot_names,         ObjectNames, V0, panic!()), // ptr ot_names
    (DWord, [Dec,Hex],           "flags", Unsigned, flags,                    u32, V0, panic!()), // dword $20c21 ; flags
    (Word,  [Dec,Hex],       "obj_class",   Custom, obj_class,         ObjectKind, V0, panic!()), // word $40 ; obj_class
    (Ptr,   [Dec,Sym],           "shape",   PtrStr, shape,                 String, V0, panic!()), // ptr shape
    (Ptr,   [Dec,Sym],     "shadowShape",   PtrStr, shadow_shape,          String, V0, panic!()), // dword 0
    (DWord, [Dec],                    "", Unsigned, unk8,                     u32, V2, 0),        // dword 0
    (DWord, [Dec],                    "", Unsigned, unk9,                     u32, V2, 0),        // dword 0
    (Word,  [Dec],       "dmgDebrisPos.",     Vec3, dmg_debris_pos,   Point3<f32>, V2, Point3::new(0f32, 0f32, 0f32)), // word 0 ; dmgDebrisPos.x
    (DWord, [Dec],                    "", Unsigned, unk13,                    u32, V2, 0),        // dword 0
    (DWord, [Dec],                    "", Unsigned, unk14,                    u32, V2, 0),        // dword 0
    (Word,  [Dec],       "dstDebrisPos.",     Vec3, dst_debris_pos,   Point3<f32>, V2, Point3::new(0f32, 0f32, 0f32)), // word 0 ; dstDebrisPos.x
    (DWord, [Dec],             "dmgType", Unsigned, dmg_type,                 u32, V2, 0),        // dword 0 ; dmgType
    (DWord, [Dec],                "year", Unsigned, year_available,           u32, V3, u32::max_value()), // dword 1956 ; year
    (Word,  [Dec],          "maxVisDist", Unsigned, max_vis_dist,             f32, V0, panic!()), // word 98 ; maxVisDist
    (Word,  [Dec],          "cameraDist", Unsigned, camera_dist,              f32, V0, panic!()), // word 0 ; cameraDist
    (Word,  [Dec],            "sigs [i]", Unsigned, unk_sig_22,               u16, V0, panic!()), // word 100 ; sigs [i]
    (Word,  [Dec],            "sigs [i]", Unsigned, unk_sig_laser,            u16, V0, panic!()), // word 100 ; sigs [i]
    (Word,  [Dec],            "sigs [i]", Unsigned, unk_sig_ir,               u16, V0, panic!()), // word 100 ; sigs [i]
    (Word,  [Dec],            "sigs [i]", Unsigned, unk_sig_radar,            u16, V0, panic!()), // word 100 ; sigs [i]
    (Word,  [Dec],            "sigs [i]", Unsigned, unk_sig_26,               u16, V0, panic!()), // word 0 ; sigs [i]
    (Word,  [Dec],           "hitPoints", Unsigned, hit_points,               u16, V0, panic!()), // word 50 ; hitPoints
    (Word,  [Dec],          "damage [i]", Unsigned, damage_on_planes,         u16, V0, panic!()), // word 0 ; damage [i]
    (Word,  [Dec],          "damage [i]", Unsigned, damage_on_ships,          u16, V0, panic!()), // word 0 ; damage [i]
    (Word,  [Dec],          "damage [i]", Unsigned, damage_on_structures,     u16, V0, panic!()), // word 0 ; damage [i]
    (Word,  [Dec],          "damage [i]", Unsigned, damage_on_armor,          u16, V0, panic!()), // word 0 ; damage [i]
    (Word,  [Dec],          "damage [i]", Unsigned, damage_on_other,          u16, V0, panic!()), // word 0 ; damage [i]
    (Byte,  [Dec],             "expType", Unsigned, explosion_type,            u8, V0, panic!()), // byte 15 ; expType
    (Byte,  [Dec],          "craterSize", Unsigned, crater_size,/*ft?*/        u8, V0, panic!()), // byte 0 ; craterSize
    (DWord, [Dec],              "weight", Unsigned, empty_weight,Weight<PoundsWeight>,V0,panic!()), // dword 0 ; weight
    (Word,  [Dec],          "cmdBufSize", Unsigned, cmd_buf_size,             u16, V0, panic!()), // word 0 ; cmdBufSize
    // Movement Info
    (Word,  [Dec],           "_turnRate", Unsigned, turn_rate,                u16, V0, panic!()), // word 0 ; _turnRate
    (Word,  [Dec],           "_bankRate", Unsigned, bank_rate,                u16, V0, panic!()), // degrees per second / 182? // word 0 ; _bankRate
    (Word,  [Dec],            "maxClimb",   Signed, max_climb,                i16, V0, panic!()), // word 0 ; maxClimb
    (Word,  [Dec],             "maxDive",   Signed, max_dive,                 i16, V0, panic!()), // word 0 ; maxDive
    (Word,  [Dec],             "maxBank",   Signed, max_bank,                 i16, V0, panic!()), // word 0 ; maxBank
    (Word,  [Dec],           "_minSpeed", Unsigned, min_speed,                u16, V0, panic!()), // word 0 ; _minSpeed
    (Word,  [Dec],        "_cornerSpeed", Unsigned, corner_speed,             u16, V0, panic!()), // word 0 ; _cornerSpeed
    (Word,  [Dec],           "_maxSpeed", Unsigned, max_speed,                u16, V0, panic!()), // word 0 ; _maxSpeed
    (DWord, [Dec,Car],            "_acc", Unsigned, acceleration,             u32, V0, panic!()), // dword ^0 ; _acc
    (DWord, [Dec,Car],           "_dacc", Unsigned, deceleration,             u32, V0, panic!()), // dword ^0 ; _dacc
    (DWord, [Dec,Hex,Car],      "minAlt",   Signed, min_altitude,             i32, V0, panic!()), // in feet? // dword ^0 ; minAlt
    (DWord, [Dec,Hex,Car],      "maxAlt",   Signed, max_altitude,             i32, V0, panic!()), // dword ^0 ; maxAlt
    (Symbol,[Sym],            "utilProc",   Custom, util_proc,           ProcKind, V0, panic!()), // symbol _OBJProc	; utilProc
    // Sound Info
    (Ptr,   [Dec,Sym],       "loopSound",   PtrStr, loop_sound,            String, V0, panic!()), // dword 0
    (Ptr,   [Dec,Sym],     "secondSound",   PtrStr, second_sound,          String, V0, panic!()), // dword 0
    (Ptr,   [Dec,Sym],   "engineOnSound",   PtrStr, engine_on_sound,       String, V1, None), // TODO: figure out what the default was in USNF. // dword 0
    (Ptr,   [Dec,Sym],  "engineOffSound",   PtrStr, engine_off_sound,      String, V1, None),     // dword 0
    (Byte,  [Dec],           "doDoppler",     Bool, do_doppler,              bool, V0, panic!()), // byte 1 ; doDoppler
    (Word,  [Dec],          "maxSndDist", Unsigned, max_snd_dist,             u16, V0, panic!()), // in feet? // word 7000 ; maxSndDist
    (Word,  [Dec], "maxPlusDopplerPitch",   Signed, max_plus_doppler_pitch,   i16, V0, panic!()), // word 25 ; maxPlusDopplerPitch
    (Word,  [Dec],"maxMinusDopplerPitch",   Signed, max_minus_doppler_pitch,  i16, V0, panic!()), // word 20 ; maxMinusDopplerPitch
    (Word,  [Dec],     "minDopplerSpeed",   Signed, min_doppler_speed,        i16, V0, panic!()), // word 20 ; minDopplerSpeed
    (Word,  [Dec],     "maxDopplerSpeed",   Signed, max_doppler_speed,        i16, V0, panic!()), // word 1000 ; maxDopplerSpeed
    (Word,  [Dec],         "viewOffset.",     Vec3, view_offset,      Point3<f32>, V0, panic!()), // word 0 ; viewOffset.x
    (Ptr,   [Dec,Sym],         "hudName",   PtrStr, hud,                   String, V2, None) // dword 0
}];

impl ObjectType {
    pub fn from_text(data: &str) -> Result<Self> {
        let lines = data.lines().collect::<Vec<&str>>();
        ensure!(
            lines[0] == "[brent's_relocatable_format]",
            "not a type file"
        );
        let pointers = parse::find_pointers(&lines)?;
        let obj_lines = parse::find_section(&lines, "OBJ_TYPE")?;
        Self::from_lines((), &obj_lines, &pointers)
    }

    pub fn file_name(&self) -> &str {
        &self.ot_names.file_name
    }

    pub fn short_name(&self) -> &str {
        &self.ot_names.short_name
    }

    pub fn long_name(&self) -> &str {
        &self.ot_names.long_name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lib::{from_dos_string, Libs};

    #[test]
    fn can_parse_all_entity_types() -> Result<()> {
        let libs = Libs::for_testing()?;
        for (game, _palette, catalog) in libs.all() {
            for fid in catalog.find_glob("*.[OJNP]T")? {
                let meta = catalog.stat(fid)?;
                println!("At: {}:{:13} @ {}", game.test_dir, meta.name(), meta.path());
                let contents = from_dos_string(catalog.read(fid)?);
                let ot = ObjectType::from_text(&contents)?;
                // Only one misspelling in 2500 files.
                assert!(ot.file_name() == meta.name() || meta.name() == "SMALLARM.JT");
            }
        }
        Ok(())
    }
}
