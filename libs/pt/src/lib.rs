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
mod envelope;

use crate::envelope::Envelope;
use asset::AssetLoader;
use failure::{bail, ensure, Fallible};
use nt::NpcType;
use ot::{
    make_consume_fields, make_storage_type, make_type_struct, make_validate_field_repr,
    make_validate_field_type, parse,
    parse::{FieldRow, FromRow, FromRows},
    ObjectType,
};
use std::collections::HashMap;

#[derive(Debug, Ord, PartialOrd, Eq, PartialEq)]
enum PlaneTypeVersion {
    V0, // USNF
    V1, // ATFGOLD (and all others?)
}

impl PlaneTypeVersion {
    fn from_len(cnt: usize) -> Fallible<Self> {
        Ok(match cnt {
            146 => PlaneTypeVersion::V1,
            130 => PlaneTypeVersion::V0,
            x => bail!("unknown pt version with {} lines", x),
        })
    }
}

// Wrap Vec<HP> so that we can impl FromRow.
pub struct Envelopes {
    #[allow(dead_code)]
    all: Vec<Envelope>,
}

impl FromRow for Envelopes {
    type Produces = Envelopes;
    fn from_row(
        row: &FieldRow,
        pointers: &HashMap<&str, Vec<&str>>,
        assets: &AssetLoader,
    ) -> Fallible<Self::Produces> {
        let (_name, lines) = row.value().pointer()?;
        let mut off = 0usize;
        let mut envs = Vec::new();

        ensure!(lines.len() % 44 == 0, "expected 44 lines per envelope");
        while off < lines.len() {
            let lns = lines[off..off + 44]
                .iter()
                .map(|v| v.as_ref())
                .collect::<Vec<_>>();
            let env = Envelope::from_lines((), &lns, pointers, assets)?;
            envs.push(env);
            off += 44;
        }
        return Ok(Envelopes { all: envs });
    }
}

#[allow(dead_code)]
struct SystemDamage {
    damage_limit: [u8; 45],
}

impl FromRows for SystemDamage {
    type Produces = SystemDamage;

    fn from_rows(
        rows: &[FieldRow],
        _pointers: &HashMap<&str, Vec<&str>>,
        _assets: &AssetLoader,
    ) -> Fallible<(Self::Produces, usize)> {
        let mut damage_limit = [0; 45];
        for (i, row) in rows[..45].iter().enumerate() {
            damage_limit[i] = row.value().numeric()?.byte()?;
        }
        Ok((Self { damage_limit }, 45))
    }
}

#[allow(dead_code)]
struct PhysBounds {
    min: f32,
    max: f32,
    acc: f32,
    dacc: f32,
}

impl FromRows for PhysBounds {
    type Produces = PhysBounds;

    fn from_rows(
        rows: &[FieldRow],
        _pointers: &HashMap<&str, Vec<&str>>,
        _assets: &AssetLoader,
    ) -> Fallible<(Self::Produces, usize)> {
        Ok((
            Self {
                min: rows[0].value().numeric()?.word()? as i16 as f32,
                max: rows[1].value().numeric()?.word()? as i16 as f32,
                acc: rows[2].value().numeric()?.word()? as i16 as f32,
                dacc: rows[3].value().numeric()?.word()? as i16 as f32,
            },
            4,
        ))
    }
}

impl Default for PhysBounds {
    fn default() -> Self {
        Self {
            min: 0f32,
            max: 0f32,
            acc: 0f32,
            dacc: 0f32,
        }
    }
}

make_type_struct![
PlaneType(nt: NpcType, version: PlaneTypeVersion) { // CMCHE.PT
(Num,   [Dec, Hex],        "flags",Unsigned, flags, u32, V0, panic!()), // dword $2d ; flags
(Ptr,   [Sym],               "env",  Custom, envelopes,      Envelopes, V0, panic!()),  // ptr env
(Word,  [Dec],            "envMin",  Signed, envMin,               i16, V0, panic!()), // word -1 ; envMin -- num negative g envelopes
(Word,  [Dec],            "envMax",  Signed, envMax,               i16, V0, panic!()), // word 4 ; envMax -- num positive g envelopes
(Word,  [Dec],     "structure [0]",Unsigned, max_speed_sea_level,  u16, V0, panic!()), // word 1182 ; structure [0] -- Max Speed @ Sea-Level (Mph)
(Word,  [Dec],     "structure [1]",Unsigned, max_speed_36a,        u16, V0, panic!()), // word 1735 ; structure [1] -- Max Speed @ 36K Feet (Mph)
(Word,  [Dec],            "_bv.x.", CustomN, bv_x,          PhysBounds, V0, panic!()),
(Word,  [Dec],            "_bv.y.", CustomN, bv_y,          PhysBounds, V0, panic!()),
(Word,  [Dec],            "_bv.z.", CustomN, bv_z,          PhysBounds, V0, panic!()),
(Word,  [Dec],           "_brv.x.", CustomN, brv_x,         PhysBounds, V0, panic!()),
(Word,  [Dec],           "_brv.y.", CustomN, brv_y,         PhysBounds, V0, panic!()),
(Word,  [Dec],           "_brv.z.", CustomN, brv_z,         PhysBounds, V0, panic!()),
(Word,  [Dec],          "gpullAOA",  Signed, gpullAOA,             i16, V0, panic!()), // word 20 ; gpullAOA
(Word,  [Dec],       "lowAOASpeed",  Signed, lowAOASpeed,          i16, V0, panic!()), // word 70 ; lowAOASpeed
(Word,  [Dec],       "lowAOAPitch",  Signed, lowAOAPitch,          i16, V0, panic!()), // word 15 ; lowAOAPitch
(Word,  [Dec], "turbulencePercent",  Signed, turbulencePercent,    i16, V1, 0),        // word 149 ; turbulencePercent
(Word,  [Dec],     "rudderYaw.min",  Signed, rudderYaw_min,        i16, V0, panic!()), // word -1 ; rudderYaw.min
(Word,  [Dec],     "rudderYaw.max",  Signed, rudderYaw_max,        i16, V0, panic!()), // word 1 ; rudderYaw.max
(Word,  [Dec],     "rudderYaw.acc",  Signed, rudderYaw_acc,        i16, V0, panic!()), // word 1 ; rudderYaw.acc
(Word,  [Dec],    "rudderYaw.dacc",  Signed, rudderYaw_dacc,       i16, V0, panic!()), // word 3 ; rudderYaw.dacc
(Word,  [Dec],        "rudderSlip",  Signed, rudderSlip,           i16, V0, panic!()), // word 10 ; rudderSlip
(Word,  [Dec],        "rudderDrag",  Signed, rudderDrag,           i16, V0, panic!()), // word 128 ; rudderDrag
(Word,  [Dec],        "rudderBank",  Signed, rudderBank,           i16, V0, panic!()), // word 5 ; rudderBank
(Word,  [Dec],        "puffRot.x.", CustomN, puffRot_x,     PhysBounds, V1, Default::default()),
(Word,  [Dec],        "puffRot.y.", CustomN, puffRot_y,     PhysBounds, V1, Default::default()),
(Word,  [Dec],        "puffRot.z.", CustomN, puffRot_z,     PhysBounds, V1, Default::default()),
(Word,  [Dec], "stallWarningDelay",  Signed, stallWarningDelay,    i16, V0, panic!()), // word 512 ; stallWarningDelay
(Word,  [Dec],        "stallDelay",  Signed, stallDelay,           i16, V0, panic!()), // word 512 ; stallDelay
(Word,  [Dec],     "stallSeverity",  Signed, stallSeverity,        i16, V0, panic!()), // word 256 ; stallSeverity
(Word,  [Dec],    "stallPitchDown",  Signed, stallPitchDown,       i16, V0, panic!()), // word 30 ; stallPitchDown
(Word,  [Dec],         "spinEntry",  Signed, spinEntry,            i16, V0, panic!()), // word 2 ; spinEntry
(Word,  [Dec],          "spinExit",  Signed, spinExit,             i16, V0, panic!()), // word -2 ; spinExit
(Word,  [Dec],        "spinYawLow",  Signed, spinYawLow,           i16, V0, panic!()), // word 120 ; spinYawLow
(Word,  [Dec],       "spinYawHigh",  Signed, spinYawHigh,          i16, V0, panic!()), // word 180 ; spinYawHigh
(Word,  [Dec],        "spinAOALow",  Signed, spinAOALow,           i16, V0, panic!()), // word 30 ; spinAOALow
(Word,  [Dec],       "spinAOAHigh",  Signed, spinAOAHigh,          i16, V0, panic!()), // word 70 ; spinAOAHigh
(Word,  [Dec],       "spinBankLow",  Signed, spinBankLow,          i16, V0, panic!()), // word 15 ; spinBankLow
(Word,  [Dec],      "spinBankHigh",  Signed, spinBankHigh,         i16, V0, panic!()), // word 5 ; spinBankHigh
(Word,  [Dec],         "gearPitch",  Signed, gearPitch,            i16, V0, panic!()), // word 0 ; gearPitch
(Word,  [Dec], "crashSpeedForward",  Signed, crashSpeedForward,    i16, V0, panic!()), // word 330 ; crashSpeedForward
(Word,  [Dec],    "crashSpeedSide",  Signed, crashSpeedSide,       i16, V0, panic!()), // word 51 ; crashSpeedSide
(Word,  [Dec],"crashSpeedVertical",  Signed, crashSpeedVertical,   i16, V0, panic!()), // word 95 ; crashSpeedVertical
(Word,  [Dec],        "crashPitch",  Signed, crashPitch,           i16, V0, panic!()), // word 25 ; crashPitch
(Word,  [Dec],         "crashRoll",  Signed, crashRoll,            i16, V0, panic!()), // word 10 ; crashRoll
(Byte,  [Dec],           "engines",Unsigned, engines,               u8, V0, panic!()), // byte 1 ; engines
(Word,  [Dec],         "negGLimit",  Signed, negGLimit,            i16, V0, panic!()), // word 2560 ; negGLimit
(DWord, [Dec],            "thrust",Unsigned, thrust,               u32, V0, panic!()), // dword 17687 ; thrust
(DWord, [Dec],         "aftThrust",Unsigned, aftThrust,            u32, V0, panic!()), // dword 0 ; aftThrust
(Word,  [Dec],       "throttleAcc",  Signed, throttleAcc,          i16, V0, panic!()), // word 40 ; throttleAcc
(Word,  [Dec],      "throttleDacc",  Signed, throttleDacc,         i16, V0, panic!()), // word 60 ; throttleDacc
(Word,  [Dec],         "vtLimitUp",  Signed, vtLimitUp,            i16, V1, 0),        // word -60 ; vtLimitUp
(Word,  [Dec],       "vtLimitDown",  Signed, vtLimitDown,          i16, V1, 0),        // word -120 ; vtLimitDown
(Word,  [Dec],           "vtSpeed",  Signed, vtSpeed,              i16, V1, 0),        // word 100 ; vtSpeed
(Word,  [Dec],   "fuelConsumption",  Signed, fuelConsumption,      i16, V0, panic!()), // word 1 ; fuelConsumption
(Word,  [Dec],"aftFuelConsumption",  Signed, aftFuelConsumption,   i16, V0, panic!()), // word 0 ; aftFuelConsumption
(DWord, [Dec],      "internalFuel",Unsigned, internalFuel,         u32, V0, panic!()), // dword 6200 ; internalFuel
(Word,  [Dec],          "coefDrag",  Signed, coefDrag,             i16, V0, panic!()), // word 256 ; coefDrag
(Word,  [Dec],        "_gpullDrag",  Signed, _gpullDrag,           i16, V0, panic!()), // word 12 ; _gpullDrag
(Word,  [Dec],     "airBrakesDrag",  Signed, airBrakesDrag,        i16, V0, panic!()), // word 256 ; airBrakesDrag
(Word,  [Dec],   "wheelBrakesDrag",  Signed, wheelBrakesDrag,      i16, V0, panic!()), // word 102 ; wheelBrakesDrag
(Word,  [Dec],         "flapsDrag",  Signed, flapsDrag,            i16, V0, panic!()), // word 0 ; flapsDrag
(Word,  [Dec],          "gearDrag",  Signed, gearDrag,             i16, V0, panic!()), // word 23 ; gearDrag
(Word,  [Dec],           "bayDrag",  Signed, bayDrag,              i16, V0, panic!()), // word 0 ; bayDrag
(Word,  [Dec],         "flapsLift",  Signed, flapsLift,            i16, V0, panic!()), // word 0 ; flapsLift
(Word,  [Dec],        "loadedDrag",  Signed, loadedDrag,           i16, V0, panic!()), // word 30 ; loadedDrag
(Word,  [Dec],   "loadedGpullDrag",  Signed, loadedGpullDrag,      i16, V0, panic!()), // word 13 ; loadedGpullDrag
(Word,  [Dec],    "loadedElevator",  Signed, loadedElevator,       i16, V0, panic!()), // word 40 ; loadedElevator
(Word,  [Dec],     "loadedAileron",  Signed, loadedAileron,        i16, V0, panic!()), // word 40 ; loadedAileron
(Word,  [Dec],      "loadedRudder",  Signed, loadedRudder,         i16, V0, panic!()), // word 40 ; loadedRudder
(Word,  [Dec],"structureWarnLimit",  Signed, structureWarnLimit,   i16, V0, panic!()), // word 2560 ; structureWarnLimit
(Word,  [Dec],    "structureLimit",  Signed, structureLimit,       i16, V0, panic!()), // word 5120 ; structureLimit
(Byte,  [Dec],  "systemDamage [i]", CustomN, systemDamage,SystemDamage, V0, panic!()), // byte 20 ; systemDamage [i] ...
(Word,  [Dec],     "miscPerFlight",  Signed, miscPerFlight,        i16, V0, panic!()), // word 10 ; miscPerFlight
(Word,  [Dec],  "repairMultiplier",  Signed, repairMultiplier,     i16, V0, panic!()), // word 10 ; repairMultiplier
(DWord, [Dec],  "maxTakeoffWeight",Unsigned, maxTakeoffWeight,     u32, V0, panic!())  // dword 16000 ; maxTakeoffWeight
}];

/*
#[allow(dead_code)]
pub struct PlaneType {
    pub npc: NpcType,

    // Awacs links, and thrust vectoring [$2011 (prefix of 20)- provides ATA link,
    // $4011 (prefix of 40)- provides ATG link, $6011 (prefix of 60)- provides ATA & ATG links,
    // $91 horizontal axis thrust vectoring, $591 horizontal and vertical thrust vectoring,
    // ex: $4591 - 3d thrust vectoring /w ATG link]
    unk_flags: u32,
    envelope: Vec<Envelope>,       //ptr env
    negative_envelopes: i16,       //word -4 ; Number of Negative G Envelopes
    positive_envelopes: i16,       //word 9 ; Number of Maximum G Envelopes
    max_speed_sea_level: i16,      //word 1340 ; Max Speed @ Sea-Level (Mph)
    max_speed_36a: i16,            //word 1934 ; Max Speed @ 36K Feet (Mph)
    unk6: i16,                     //word -73 ; _bv.x.min
    unk7: i16,                     //word 0 ; _bv.x.max
    unk8: i16,                     //word 73 ; Acceleration on Runway
    unk9: i16,                     //word 73 ; Deceleration on Runway(?)
    unk10: i16,                    //word -146 ; _bv.y.min
    unk11: i16,                    //word 146 ; _bv.y.max
    unk12: i16,                    //word 7 ; _bv.y.acc
    unk13: i16,                    //word 7 ; _bv.y.dacc
    unk14: i16,                    //word -146 ; _bv.z.min
    unk15: i16,                    //word 146 ; _bv.z.max
    unk16: i16,                    //word 73 ; _bv.z.acc
    unk17: i16,                    //word 73 ; _bv.z.dacc
    unk18: i16,                    //word -270 ; Roll Speed(?)
    unk19: i16,                    //word 270 ; Roll Speed(?)
    unk20: i16,                    //word 362 ; Pull Up or Down Rate(?)
    unk21: i16,                    //word 724 ; Pull Up or Down Rate(?)
    unk22: i16,                    //word 0 ; _brv.y.min
    unk23: i16,                    //word 0 ; _brv.y.max
    unk24: i16,                    //word 9 ; G-Pull Accelleration Rate(?)
    unk25: i16,                    //word 9 ; G-Pull Deceleration Rate(?)
    unk26: i16,                    //word -45 ; Yaw Speed On Runway
    unk27: i16,                    //word 45 ; Yaw Speed on Runway
    unk28: i16,                    //word 90 ; Yaw Accelleration on Runway
    unk29: i16,                    //word 90 ; Yaw Deceleration on Runway
    unk30: i16,                    //word 7 ; AoA G pull limit when plane exeeds 9 G's
    unk31: i16,                    //word 50 ; AoA speed limit when plane exeeds 9 G's
    unk32: i16,                    //word 10 ; AoA pitch limit when plane exeeds 9 G's
    unk33: i16,                    //word 115 ; Turbulence Percentage
    unk34: i16,                    //word -4 ; Rudder Min Yaw limit
    unk35: i16,                    //word 4 ; Rudder Max Yaw limit
    unk36: i16,                    //word 4 ; Degrees yaw/sec when rudder is fully deflected
    unk37: i16,                    //word 9 ; Degrees yaw/sec when rudder returns to neutral
    unk38: i16,                    //word 6 ; Rudder Slip deg/sec when rudder is fully deflected
    unk39: i16,                    //word 108 ; Rudder Drag when rudder is fully deflected
    unk40: i16,                    //word 60 ; Roll in deg/sec when rudder is fully deflected
    unk41: i16,                    //word -90 ; puffRot.x.min
    unk42: i16,                    //word 90 ; puffRot.x.max
    unk43: i16,                    //word 362 ; Pull Up or Down Rate II(?)
    unk44: i16,                    //word 724 ; Pull Up or Down Rate II(?)
    unk45: i16,                    //word -90 ; puffRot.y.min
    unk46: i16,                    //word 90 ; puffRot.y.max
    unk47: i16,                    //word 362 ; Pull Up or Down Rate III(?)
    unk48: i16,                    //word 724 ; Pull Up or Down Rate III(?)
    unk49: i16,                    //word -90 ; puffRot.z.min
    unk50: i16,                    //word 90 ; puffRot.z.max
    unk51: i16,                    //word 362 ; Pull Up or Down Speed IV(?)
    unk52: i16,                    //word 724 ; Pull Up or Down Speed IV(?)
    unk53: i16,                    //word 512 ; Stall Warning delay in clocks (1 clock = 1/256 sec)
    unk54: i16,                    //word 512 ; Stall Delay/Duration
    unk55: i16,                    //word 222 ; Stall Severity
    unk56: i16,                    //word 30 ; Stall Pitch-Down in deg/sec
    unk57: i16,                    //word 2 ; Ease of entry into Spin
    unk58: i16,                    //word -2 ; Ease of exit from Spin
    unk59: i16,                    //word 120 ; Sping yaw low
    unk60: i16,                    //word 180 ; Sping yaw high
    unk61: i16,                    //word 30 ; Spin AoA low
    unk62: i16,                    //word 70 ; Spin AoA high
    unk63: i16,                    //word 15 ; Spin bank low
    unk64: i16,                    //word 5 ; Spin bank high
    unk65: i16, //word 0 ; Gear Pitch (Pitch of plane when on ground, relative to the horizon.  Ex: a taildragger)
    unk66: i16, //word 325 ; Max safe landing speed
    unk67: i16, //word 31 ; Max landing side speed
    unk68: i16, //word 45 ; Max rate of decent on landing
    unk69: i16, //word 12 ; Max pitch on landing
    unk70: i16, //word 8 ; Max landing roll (distance plane rolls-out after touchdown)
    unk_engine_count: u8, //byte 1 ; Number of Engines
    unk72: i16, //word 0 ; negGLimit
    unk_thrust_100: u32, //dword 14800 ; Military Thrust in lbs
    unk_thrust_after: u32, //dword 23800 ; Afterburning Thrust in lbs
    unk75: i16, //word 20 ; Throttle Acceleration in percent/sec
    unk76: i16, //word 30 ; Throttle Deceleration in percent/sec
    unk77: i16, //word 0 ; Min Thrust Vectoring Angle (-60 = 60º)
    unk78: i16, //word 0 ; Max Thrust Vectoring down-angle (-90 = full downward, -180 = full forward)
    unk79: i16, //word 0 ; Thrust vectoring speed in degrees per second
    unk80: i16, //word 1 ; Fuel consumption @ Military Power
    unk81: i16, //word 14 ; Fuel consumption @ Afterburning power
    unk_fuel_capacity: u32, //dword 6972 ; Fuel capacity in lbs
    unk83: i16, //word 190 ; Aerodynamic Drag (bigger the number, more aerodyn. drag)
    unk84: i16, //word 96 ; G-pull drag (increase in drag due to G-pull)
    unk_air_brake_drag: i16, //word 125 ; Airbrake Drag
    unk_wheel_brake_drag: i16, //word 44 ; Wheel Brake Drag
    unk_flap_drag: i16, //word 76 ; Flap Drag
    unk_gear_drag: i16, //word 23 ; Gear Drag
    unk_bay_drag: i16, //word 0 ; Weapons-Bay Drag
    unk_flaps_lift: i16, //word 15 ; Flaps Lift
    unk_loadout_drag: i16, //word 60 ; Drag increase when fully loaded
    unk_loadout_g_drag: i16, //word 33 ; G-pull Drag increase when fully loaded
    unk93: i16, //word 50 ; Extra load on elevators when fully loaded
    unk94: i16, //word 50 ; Extra load on ailerons when fully loaded
    unk95: i16, //word 60 ; Extra load on rudder when fully loaded
    structural_speed_warning: i16, //word 2560 ; Structural Speed Limit Warning
    structural_speed_limit: i16, //word 5120 ; Structural Speed Limit
    unk_system_maintainence: [u8; 45],
    unk_maintainence_per_mission: i16, //word 10 ; miscPerFlight
    unk_maintainence_multiplier: i16,  //word 10 ; repairMultiplier
    unk_max_takeoff_weight: u32,       //dword 34500 ; MTOW (Max Take-Off Weight)
}
*/

impl PlaneType {
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
        let npc = NpcType::from_lines(obj, &npc_lines, &pointers, assets)?;

        // The :hards and :env pointer sections are inside of the PLANE_TYPE section
        // for some reason, so filter those out by finding the first :foo.
        let plane_lines = parse::find_section(&lines, "PLANE_TYPE")?
            .iter()
            .map(|&l| l)
            .take_while(|&l| !l.starts_with(':'))
            .collect::<Vec<&str>>();
        println!("lineS: {}", plane_lines.len());
        let plane = Self::from_lines(npc, &plane_lines, &pointers, &assets)?;

        return Ok(plane);
    }
}

#[cfg(test)]
extern crate omnilib;

#[cfg(test)]
mod tests {
    use super::*;
    use failure::Error;
    use omnilib::OmniLib;

    #[test]
    fn it_can_parse_all_plane_files() -> Fallible<()> {
        let omni = OmniLib::new_for_test_in_games(vec![
            "FA", "USNF97", "ATFGOLD", "ATFNATO", "ATF", "MF", "USNF",
        ])?;
        for (game, name) in omni.find_matching("*.PT")?.iter() {
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
            let pt = PlaneType::from_str(&contents, &assets)?;
            assert_eq!(pt.nt.ot.file_name(), *name);
            //println!("{}:{} - tow:{}, min:{}, max:{}, acc:{}, dacc:{}", game, name, pt.maxTakeoffWeight, pt.bv_y.min, pt.bv_y.max, pt.bv_y.acc, pt.bv_y.dacc);
            //            println!(
            //                "{}:{:13}> {:08X} <> {}",
            //                game, name, pt.unk_max_takeoff_weight, pt.npc.obj.long_name
            //            );
        }
        return Ok(());
    }
}
