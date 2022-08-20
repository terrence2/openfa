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

pub use crate::envelope::{Envelope, EnvelopeIntersection};

use absolute_unit::{
    Angle, Degrees, Force, Hours, Length, Mass, MassRate, Meters, Miles, PoundsForce, PoundsMass,
    Seconds, Velocity,
};
use anyhow::{bail, ensure, Result};
use nt::NpcType;
use ot::{
    make_type_struct, parse,
    parse::{FieldRow, FromRow, FromRows},
    ObjectType,
};
use std::fmt::Formatter;
use std::{collections::HashMap, fmt, slice::Iter};

#[derive(Debug, Ord, PartialOrd, Eq, PartialEq)]
enum PlaneTypeVersion {
    V0, // USNF
    V1, // ATFGOLD (and all others?)
}

impl PlaneTypeVersion {
    fn from_len(cnt: usize) -> Result<Self> {
        Ok(match cnt {
            146 => PlaneTypeVersion::V1,
            130 => PlaneTypeVersion::V0,
            x => bail!("unknown pt version with {} lines", x),
        })
    }
}

#[derive(Copy, Clone, Debug)]
pub enum GloadExtrema {
    Inside(f64),
    Stall(f64),
    OverSpeed(f64),
    LiftFail(f64),
}

impl fmt::Display for GloadExtrema {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        // For display in the envelope window
        match self {
            Self::Inside(v) => {
                fmt::Display::fmt(v, f)?;
                write!(f, "G")
            }
            Self::Stall(_) => write!(f, "stall"),
            Self::OverSpeed(_) => write!(f, "max-q"),
            Self::LiftFail(_) => write!(f, "lift-fail"),
        }
    }
}

impl GloadExtrema {
    pub fn max_g_load(&self) -> f64 {
        *match self {
            Self::Inside(f) => f,
            Self::Stall(f) => f,
            Self::OverSpeed(f) => f,
            Self::LiftFail(f) => f,
        }
    }
}

// Wrap Vec<HP> so that we can impl FromRow.
#[derive(Debug)]
pub struct Envelopes {
    all: Vec<Envelope>,
    min_g: i16,
    max_g: i16,
}

impl FromRow for Envelopes {
    type Produces = Envelopes;
    fn from_row(row: &FieldRow, pointers: &HashMap<&str, Vec<&str>>) -> Result<Self::Produces> {
        let (_name, lines) = row.value().pointer()?;
        let mut off = 0usize;
        let mut envs = Vec::new();

        ensure!(lines.len() % 44 == 0, "expected 44 lines per envelope");
        let mut min_g = 1_000;
        let mut max_g = -1_000;
        while off < lines.len() {
            let lns = lines[off..off + 44]
                .iter()
                .map(std::convert::AsRef::as_ref)
                .collect::<Vec<_>>();
            let env = Envelope::from_lines((), &lns, pointers)?;
            if env.gload > max_g {
                max_g = env.gload;
            }
            if env.gload < min_g {
                min_g = env.gload;
            }
            envs.push(env);
            off += 44;
        }
        envs.sort_by_cached_key(|envelope| envelope.gload);
        Ok(Envelopes {
            all: envs,
            min_g,
            max_g,
        })
    }
}

impl Envelopes {
    pub fn iter(&self) -> Iter<Envelope> {
        self.all.iter()
    }

    pub fn min_g_load(&self) -> i16 {
        self.min_g
    }

    pub fn max_g_load(&self) -> i16 {
        self.max_g
    }

    pub fn find_min_lift_speed_at(
        &self,
        altitude: Length<Meters>,
    ) -> Option<Velocity<Meters, Seconds>> {
        for envelope in self.all.iter() {
            if envelope.gload == 1 {
                return envelope.find_min_lift_speed_at(altitude);
            }
        }
        None
    }

    pub fn find_g_load_maxima(
        &self,
        speed: Velocity<Meters, Seconds>,
        altitude: Length<Meters>,
    ) -> GloadExtrema {
        // From inside (tightest envelope) outwards.
        let mut prior = None;
        for envelope in self.all.iter().rev() {
            // Check if we are fully in this envelope.
            let intersect = envelope.find_g_load_extrema(speed, altitude);
            if let EnvelopeIntersection::Inside {
                to_stall,
                to_over_speed,
                to_lift_fail,
            } = intersect
            {
                return GloadExtrema::Inside(match prior {
                    // If we are in the highest g-load envelope, that is our max.
                    None => envelope.gload as f64,
                    Some(EnvelopeIntersection::Stall(v)) => {
                        envelope.gload as f64 + (to_stall / (to_stall + v))
                    }
                    Some(EnvelopeIntersection::OverSpeed(v)) => {
                        envelope.gload as f64 + (to_over_speed / (to_over_speed + v))
                    }
                    Some(EnvelopeIntersection::LiftFail(v)) => {
                        envelope.gload as f64 + (to_lift_fail / (to_lift_fail + v))
                    }
                    Some(EnvelopeIntersection::Inside { .. }) => {
                        panic!("found non-returned intersection?")
                    }
                });
            } else {
                prior = Some(intersect);
            }

            // Our negative extrema is a different loop.
            if envelope.gload == 0 {
                break;
            }
        }

        // Inside no envelopes... map from the last failed envelope, which should be 0.
        match prior {
            None => panic!("empty envelope!"),
            Some(EnvelopeIntersection::Stall(v)) => GloadExtrema::Stall(v),
            Some(EnvelopeIntersection::OverSpeed(v)) => GloadExtrema::OverSpeed(v),
            Some(EnvelopeIntersection::LiftFail(v)) => GloadExtrema::LiftFail(v),
            // Broke after first envelope, therefore must be 0
            Some(EnvelopeIntersection::Inside { .. }) => GloadExtrema::Inside(0.),
        }
    }
}

impl fmt::Display for Envelopes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<Envelopes:{},{}>", self.min_g, self.max_g)
    }
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct SystemDamage {
    damage_limit: [u8; 45],
}

impl fmt::Debug for SystemDamage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = self
            .damage_limit
            .iter()
            .map(|&v| format!("{}", v))
            .collect::<Vec<String>>()
            .join(", ");
        write!(f, "SystemDamage {{ limits: {:?} }}", s)
    }
}

impl fmt::Display for SystemDamage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl FromRows for SystemDamage {
    type Produces = SystemDamage;

    fn from_rows(
        rows: &[FieldRow],
        _pointers: &HashMap<&str, Vec<&str>>,
    ) -> Result<(Self::Produces, usize)> {
        let mut damage_limit = [0; 45];
        for (i, row) in rows[..45].iter().enumerate() {
            damage_limit[i] = row.value().numeric()?.byte()?;
        }
        Ok((Self { damage_limit }, 45))
    }
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct PhysBounds {
    min: f32,
    max: f32,
    acc: f32,
    dacc: f32,
}

impl PhysBounds {
    pub fn min64(&self) -> f64 {
        self.min as f64
    }

    pub fn max64(&self) -> f64 {
        self.max as f64
    }

    pub fn acc64(&self) -> f64 {
        self.acc as f64
    }

    pub fn dacc64(&self) -> f64 {
        self.dacc as f64
    }
}

impl FromRows for PhysBounds {
    type Produces = PhysBounds;

    fn from_rows(
        rows: &[FieldRow],
        _pointers: &HashMap<&str, Vec<&str>>,
    ) -> Result<(Self::Produces, usize)> {
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

impl fmt::Display for PhysBounds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{:0.2}, {:0.2}], *[{:0.2}, {:0.2}]",
            self.min, self.max, self.acc, self.dacc
        )
    }
}

make_type_struct![
PlaneType(nt: NpcType, version: PlaneTypeVersion) { // CMCHE.PT
(Num,   [Dec, Hex],        "flags",Unsigned, flags,                u32, V0, panic!()), // dword $2d ; flags
// Pitch response appears to be largely based on envelope.
(Ptr,   [Sym],               "env",  Custom, envelopes,      Envelopes, V0, panic!()),  // ptr env
(Word,  [Dec],            "envMin",  Signed, env_min,              i16, V0, panic!()), // word -1 ; envMin -- num negative g envelopes
(Word,  [Dec],            "envMax",  Signed, env_max,              i16, V0, panic!()), // word 4 ; envMax -- num positive g envelopes
(Word,  [Dec],     "structure [0]",Unsigned, max_speed_sea_level,  u16, V0, panic!()), // word 1182 ; structure [0] -- Max Speed @ Sea-Level (Mph)
(Word,  [Dec],     "structure [1]",Unsigned, max_speed_36a,Velocity<Miles, Hours>, V0, panic!()), // word 1735 ; structure [1] -- Max Speed @ 36K Feet (Mph)
// Only blimp.pt has bv_* different from all others. Probably unused?
(Word,  [Dec],            "_bv.x.", CustomN, bv_x,          PhysBounds, V0, panic!()),
(Word,  [Dec],            "_bv.y.", CustomN, bv_y,          PhysBounds, V0, panic!()),
(Word,  [Dec],            "_bv.z.", CustomN, bv_z,          PhysBounds, V0, panic!()),
// Max roll rate left, then right; acc is rate towards inceptor away from neutral, dacc is rate towards roll inceptor towards neutral
(Word,  [Dec],           "_brv.x.", CustomN, brv_x,         PhysBounds, V0, panic!()),
// No apparent effects; most are [-0,0], [-small,small]; check correlation with rudder_yaw_*
(Word,  [Dec],           "_brv.y.", CustomN, brv_y,         PhysBounds, V0, panic!()),
// No apparent effects
(Word,  [Dec],           "_brv.z.", CustomN, brv_z,         PhysBounds, V0, panic!()),
(Word,  [Dec],          "gpullAOA",  Signed, gpull_aoa, Angle<Degrees>, V0, panic!()), // word 20 ; gpullAOA
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
// No apparent effect? Highly specialized acc/dacc
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
(DWord, [Dec],            "thrust",Unsigned, thrust,               Force<PoundsForce>, V0, panic!()), // dword 17687 ; thrust
(DWord, [Dec],         "aftThrust",Unsigned, aft_thrust,           Force<PoundsForce>, V0, panic!()), // dword 0 ; aftThrust
(Word,  [Dec],       "throttleAcc",  Signed, throttle_acc,         i16, V0, panic!()), // word 40 ; throttleAcc
(Word,  [Dec],      "throttleDacc",  Signed, throttle_dacc,        i16, V0, panic!()), // word 60 ; throttleDacc
(Word,  [Dec],         "vtLimitUp",  Signed, vt_limit_up,          i16, V1, 0),        // word -60 ; vtLimitUp
(Word,  [Dec],       "vtLimitDown",  Signed, vt_limit_down,        i16, V1, 0),        // word -120 ; vtLimitDown
(Word,  [Dec],           "vtSpeed",  Signed, vt_speed,             i16, V1, 0),        // word 100 ; vtSpeed
(Word,  [Dec],   "fuelConsumption",  Signed, fuel_consumption,     MassRate<PoundsMass,Seconds>, V0, panic!()), // word 1 ; fuelConsumption
(Word,  [Dec],"aftFuelConsumption",  Signed, aft_fuel_consumption, MassRate<PoundsMass,Seconds>, V0, panic!()), // word 0 ; aftFuelConsumption
(DWord, [Dec],      "internalFuel",Unsigned, internal_fuel,        Mass<PoundsMass>, V0, panic!()), // dword 6200 ; internalFuel
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
(DWord, [Dec],  "maxTakeoffWeight",Unsigned, max_takeoff_weight,   Mass<PoundsMass>, V0, panic!())  // dword 16000 ; maxTakeoffWeight
}];

impl PlaneType {
    pub fn from_text(data: &str) -> Result<Self> {
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
mod tests {
    use super::*;
    use lib::{from_dos_string, Libs};

    #[test]
    fn it_can_parse_all_plane_files() -> Result<()> {
        let libs = Libs::for_testing()?;
        let mut check_count = 0;
        for (game, _palette, catalog) in libs.all() {
            for fid in catalog.find_with_extension("PT")? {
                let meta = catalog.stat(fid)?;
                println!("At: {}:{:13} @ {}", game.test_dir, meta.name(), meta.path());
                let contents = from_dos_string(catalog.read(fid)?);
                let pt = PlaneType::from_text(contents.as_ref())?;
                if pt.puff_rot_x.acc != pt.brv_x.acc || pt.puff_rot_x.dacc != pt.brv_x.dacc {
                    check_count += 1;
                }
                assert_eq!(-pt.brv_x.min, pt.brv_x.max);
                assert_eq!(pt.brv_y.acc, pt.brv_y.dacc);
                assert_eq!(pt.nt.ot.file_name(), meta.name());
            }
        }
        // TODO: figure out why puff_rot != brv in only a handful of models
        assert!(check_count <= 77);

        Ok(())
    }
}
