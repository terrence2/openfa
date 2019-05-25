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
use failure::{bail, ensure, Fallible};
use nt::NpcType;
use ot::{
    make_type_struct, parse,
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
    fn from_row(row: &FieldRow, pointers: &HashMap<&str, Vec<&str>>) -> Fallible<Self::Produces> {
        let (_name, lines) = row.value().pointer()?;
        let mut off = 0usize;
        let mut envs = Vec::new();

        ensure!(lines.len() % 44 == 0, "expected 44 lines per envelope");
        while off < lines.len() {
            let lns = lines[off..off + 44]
                .iter()
                .map(std::convert::AsRef::as_ref)
                .collect::<Vec<_>>();
            let env = Envelope::from_lines((), &lns, pointers)?;
            envs.push(env);
            off += 44;
        }
        Ok(Envelopes { all: envs })
    }
}

#[allow(dead_code)]
pub struct SystemDamage {
    damage_limit: [u8; 45],
}

impl FromRows for SystemDamage {
    type Produces = SystemDamage;

    fn from_rows(
        rows: &[FieldRow],
        _pointers: &HashMap<&str, Vec<&str>>,
    ) -> Fallible<(Self::Produces, usize)> {
        let mut damage_limit = [0; 45];
        for (i, row) in rows[..45].iter().enumerate() {
            damage_limit[i] = row.value().numeric()?.byte()?;
        }
        Ok((Self { damage_limit }, 45))
    }
}

#[allow(dead_code)]
pub struct PhysBounds {
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
    ) -> Fallible<(Self::Produces, usize)> {
        Ok((
            Self {
                min: f32::from(rows[0].value().numeric()?.word()? as i16),
                max: f32::from(rows[1].value().numeric()?.word()? as i16),
                acc: f32::from(rows[2].value().numeric()?.word()? as i16),
                dacc: f32::from(rows[3].value().numeric()?.word()? as i16),
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
(Num,   [Dec, Hex],        "flags",Unsigned, flags,                u32, V0, panic!()), // dword $2d ; flags
(Ptr,   [Sym],               "env",  Custom, envelopes,      Envelopes, V0, panic!()),  // ptr env
(Word,  [Dec],            "envMin",  Signed, env_min,              i16, V0, panic!()), // word -1 ; envMin -- num negative g envelopes
(Word,  [Dec],            "envMax",  Signed, env_max,              i16, V0, panic!()), // word 4 ; envMax -- num positive g envelopes
(Word,  [Dec],     "structure [0]",Unsigned, max_speed_sea_level,  u16, V0, panic!()), // word 1182 ; structure [0] -- Max Speed @ Sea-Level (Mph)
(Word,  [Dec],     "structure [1]",Unsigned, max_speed_36a,        u16, V0, panic!()), // word 1735 ; structure [1] -- Max Speed @ 36K Feet (Mph)
(Word,  [Dec],            "_bv.x.", CustomN, bv_x,          PhysBounds, V0, panic!()),
(Word,  [Dec],            "_bv.y.", CustomN, bv_y,          PhysBounds, V0, panic!()),
(Word,  [Dec],            "_bv.z.", CustomN, bv_z,          PhysBounds, V0, panic!()),
(Word,  [Dec],           "_brv.x.", CustomN, brv_x,         PhysBounds, V0, panic!()),
(Word,  [Dec],           "_brv.y.", CustomN, brv_y,         PhysBounds, V0, panic!()),
(Word,  [Dec],           "_brv.z.", CustomN, brv_z,         PhysBounds, V0, panic!()),
(Word,  [Dec],          "gpullAOA",  Signed, gpull_aoa,            i16, V0, panic!()), // word 20 ; gpullAOA
(Word,  [Dec],       "lowAOASpeed",  Signed, low_aoa_speed,        i16, V0, panic!()), // word 70 ; lowAOASpeed
(Word,  [Dec],       "lowAOAPitch",  Signed, low_aoa_pitch,        i16, V0, panic!()), // word 15 ; lowAOAPitch
(Word,  [Dec], "turbulencePercent",  Signed, turbulence_percent,   i16, V1, 0),        // word 149 ; turbulencePercent
(Word,  [Dec],     "rudderYaw.min",  Signed, rudder_yaw_min,       i16, V0, panic!()), // word -1 ; rudderYaw.min
(Word,  [Dec],     "rudderYaw.max",  Signed, rudder_yaw_max,       i16, V0, panic!()), // word 1 ; rudderYaw.max
(Word,  [Dec],     "rudderYaw.acc",  Signed, rudder_yaw_acc,       i16, V0, panic!()), // word 1 ; rudderYaw.acc
(Word,  [Dec],    "rudderYaw.dacc",  Signed, rudder_yaw_dacc,      i16, V0, panic!()), // word 3 ; rudderYaw.dacc
(Word,  [Dec],        "rudderSlip",  Signed, rudder_slip,          i16, V0, panic!()), // word 10 ; rudderSlip
(Word,  [Dec],        "rudderDrag",  Signed, rudder_drag,          i16, V0, panic!()), // word 128 ; rudderDrag
(Word,  [Dec],        "rudderBank",  Signed, rudder_bank,          i16, V0, panic!()), // word 5 ; rudderBank
(Word,  [Dec],        "puffRot.x.", CustomN, puff_rot_x,    PhysBounds, V1, Default::default()),
(Word,  [Dec],        "puffRot.y.", CustomN, puff_rot_y,    PhysBounds, V1, Default::default()),
(Word,  [Dec],        "puffRot.z.", CustomN, puff_rot_z,    PhysBounds, V1, Default::default()),
(Word,  [Dec], "stallWarningDelay",  Signed, stall_warning_delay,  i16, V0, panic!()), // word 512 ; stallWarningDelay
(Word,  [Dec],        "stallDelay",  Signed, stall_delay,          i16, V0, panic!()), // word 512 ; stallDelay
(Word,  [Dec],     "stallSeverity",  Signed, stall_severity,       i16, V0, panic!()), // word 256 ; stallSeverity
(Word,  [Dec],    "stallPitchDown",  Signed, stall_pitch_down,     i16, V0, panic!()), // word 30 ; stallPitchDown
(Word,  [Dec],         "spinEntry",  Signed, spin_entry,           i16, V0, panic!()), // word 2 ; spinEntry
(Word,  [Dec],          "spinExit",  Signed, spin_exit,            i16, V0, panic!()), // word -2 ; spinExit
(Word,  [Dec],        "spinYawLow",  Signed, spin_yaw_low,         i16, V0, panic!()), // word 120 ; spinYawLow
(Word,  [Dec],       "spinYawHigh",  Signed, spin_yaw_high,        i16, V0, panic!()), // word 180 ; spinYawHigh
(Word,  [Dec],        "spinAOALow",  Signed, spin_aoa_low,         i16, V0, panic!()), // word 30 ; spinAOALow
(Word,  [Dec],       "spinAOAHigh",  Signed, spin_aoa_high,        i16, V0, panic!()), // word 70 ; spinAOAHigh
(Word,  [Dec],       "spinBankLow",  Signed, spin_bank_low,        i16, V0, panic!()), // word 15 ; spinBankLow
(Word,  [Dec],      "spinBankHigh",  Signed, spin_bank_high,       i16, V0, panic!()), // word 5 ; spinBankHigh
(Word,  [Dec],         "gearPitch",  Signed, gear_pitch,           i16, V0, panic!()), // word 0 ; gearPitch
(Word,  [Dec], "crashSpeedForward",  Signed, crash_speed_forward,  i16, V0, panic!()), // word 330 ; crashSpeedForward
(Word,  [Dec],    "crashSpeedSide",  Signed, crash_speed_side,     i16, V0, panic!()), // word 51 ; crashSpeedSide
(Word,  [Dec],"crashSpeedVertical",  Signed, crash_speed_vertical, i16, V0, panic!()), // word 95 ; crashSpeedVertical
(Word,  [Dec],        "crashPitch",  Signed, crash_pitch,          i16, V0, panic!()), // word 25 ; crashPitch
(Word,  [Dec],         "crashRoll",  Signed, crash_roll,           i16, V0, panic!()), // word 10 ; crashRoll
(Byte,  [Dec],           "engines",Unsigned, engines,               u8, V0, panic!()), // byte 1 ; engines
(Word,  [Dec],         "negGLimit",  Signed, neg_g_limit,          i16, V0, panic!()), // word 2560 ; negGLimit
(DWord, [Dec],            "thrust",Unsigned, thrust,               u32, V0, panic!()), // dword 17687 ; thrust
(DWord, [Dec],         "aftThrust",Unsigned, aft_thrust,           u32, V0, panic!()), // dword 0 ; aftThrust
(Word,  [Dec],       "throttleAcc",  Signed, throttle_acc,         i16, V0, panic!()), // word 40 ; throttleAcc
(Word,  [Dec],      "throttleDacc",  Signed, throttle_dacc,        i16, V0, panic!()), // word 60 ; throttleDacc
(Word,  [Dec],         "vtLimitUp",  Signed, vt_limit_up,          i16, V1, 0),        // word -60 ; vtLimitUp
(Word,  [Dec],       "vtLimitDown",  Signed, vt_limit_down,        i16, V1, 0),        // word -120 ; vtLimitDown
(Word,  [Dec],           "vtSpeed",  Signed, vt_speed,             i16, V1, 0),        // word 100 ; vtSpeed
(Word,  [Dec],   "fuelConsumption",  Signed, fuel_consumption,     i16, V0, panic!()), // word 1 ; fuelConsumption
(Word,  [Dec],"aftFuelConsumption",  Signed, aft_fuel_consumption, i16, V0, panic!()), // word 0 ; aftFuelConsumption
(DWord, [Dec],      "internalFuel",Unsigned, internal_fuel,        u32, V0, panic!()), // dword 6200 ; internalFuel
(Word,  [Dec],          "coefDrag",  Signed, coef_drag,            i16, V0, panic!()), // word 256 ; coefDrag
(Word,  [Dec],        "_gpullDrag",  Signed, _gpull_drag,          i16, V0, panic!()), // word 12 ; _gpullDrag
(Word,  [Dec],     "airBrakesDrag",  Signed, air_brakes_drag,      i16, V0, panic!()), // word 256 ; airBrakesDrag
(Word,  [Dec],   "wheelBrakesDrag",  Signed, wheel_brakes_drag,    i16, V0, panic!()), // word 102 ; wheelBrakesDrag
(Word,  [Dec],         "flapsDrag",  Signed, flaps_drag,           i16, V0, panic!()), // word 0 ; flapsDrag
(Word,  [Dec],          "gearDrag",  Signed, gear_drag,            i16, V0, panic!()), // word 23 ; gearDrag
(Word,  [Dec],           "bayDrag",  Signed, bay_drag,             i16, V0, panic!()), // word 0 ; bayDrag
(Word,  [Dec],         "flapsLift",  Signed, flaps_lift,           i16, V0, panic!()), // word 0 ; flapsLift
(Word,  [Dec],        "loadedDrag",  Signed, loaded_drag,          i16, V0, panic!()), // word 30 ; loadedDrag
(Word,  [Dec],   "loadedGpullDrag",  Signed, loaded_gpull_drag,    i16, V0, panic!()), // word 13 ; loadedGpullDrag
(Word,  [Dec],    "loadedElevator",  Signed, loaded_elevator,      i16, V0, panic!()), // word 40 ; loadedElevator
(Word,  [Dec],     "loadedAileron",  Signed, loaded_aileron,       i16, V0, panic!()), // word 40 ; loadedAileron
(Word,  [Dec],      "loadedRudder",  Signed, loaded_rudder,        i16, V0, panic!()), // word 40 ; loadedRudder
(Word,  [Dec],"structureWarnLimit",  Signed, structure_warn_limit, i16, V0, panic!()), // word 2560 ; structureWarnLimit
(Word,  [Dec],    "structureLimit",  Signed, structure_limit,      i16, V0, panic!()), // word 5120 ; structureLimit
(Byte,  [Dec],  "systemDamage [i]", CustomN, system_damage,SystemDamage,V0, panic!()), // byte 20 ; systemDamage [i] ...
(Word,  [Dec],     "miscPerFlight",  Signed, misc_per_flight,      i16, V0, panic!()), // word 10 ; miscPerFlight
(Word,  [Dec],  "repairMultiplier",  Signed, repair_multiplier,    i16, V0, panic!()), // word 10 ; repairMultiplier
(DWord, [Dec],  "maxTakeoffWeight",Unsigned, max_takeoff_weight,   u32, V0, panic!())  // dword 16000 ; maxTakeoffWeight
}];

impl PlaneType {
    pub fn from_text(data: &str) -> Fallible<Self> {
        let lines = data.lines().collect::<Vec<&str>>();
        ensure!(
            lines[0] == "[brent's_relocatable_format]",
            "not a type file"
        );
        let pointers = parse::find_pointers(&lines)?;
        let obj_lines = parse::find_section(&lines, "OBJ_TYPE")?;
        let obj = ObjectType::from_lines((), &obj_lines, &pointers)?;
        let npc_lines = parse::find_section(&lines, "NPC_TYPE")?;
        let npc = NpcType::from_lines(obj, &npc_lines, &pointers)?;

        // The :hards and :env pointer sections are inside of the PLANE_TYPE section
        // for some reason, so filter those out by finding the first :foo.
        let plane_lines = parse::find_section(&lines, "PLANE_TYPE")?
            .iter()
            .take_while(|&l| !l.starts_with(':'))
            .cloned()
            .collect::<Vec<_>>();
        let plane = Self::from_lines(npc, &plane_lines, &pointers)?;

        Ok(plane)
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
        let omni = OmniLib::new_for_test_in_games(&[
            "FA", "USNF97", "ATFGOLD", "ATFNATO", "ATF", "MF", "USNF",
        ])?;
        for (game, name) in omni.find_matching("*.PT")?.iter() {
            println!(
                "At: {}:{:13} @ {}",
                game,
                name,
                omni.path(game, name)
                    .or_else::<Error, _>(|_| Ok("<none>".to_string()))?
            );
            let lib = omni.library(game);
            let contents = lib.load_text(name)?;
            let pt = PlaneType::from_text(&contents)?;
            assert_eq!(pt.nt.ot.file_name(), *name);
            //println!("{}:{} - tow:{}, min:{}, max:{}, acc:{}, dacc:{}", game, name, pt.maxTakeoffWeight, pt.bv_y.min, pt.bv_y.max, pt.bv_y.acc, pt.bv_y.dacc);
            //            println!(
            //                "{}:{:13}> {:08X} <> {}",
            //                game, name, pt.unk_max_takeoff_weight, pt.npc.obj.long_name
            //            );
        }
        Ok(())
    }
}
