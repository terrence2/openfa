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
use anyhow::{bail, ensure, Result};
use ot::{
    make_type_struct, parse,
    parse::{parse_string, FieldRow, FromRow},
    ObjectType,
};
use std::collections::HashMap;

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct ProjectileNames {
    pub short_name: String,
    pub long_name: String,
    pub file_name: Option<String>,
}

impl FromRow for ProjectileNames {
    type Produces = ProjectileNames;
    fn from_row(field: &FieldRow, _pointers: &HashMap<&str, Vec<&str>>) -> Result<Self::Produces> {
        let (name, values) = field.value().pointer()?;
        ensure!(name == "si_names", "expected pointer to si_names");
        ensure!(values.len() >= 2, "expected at least 2 names in si_names");
        let file_name = if values.len() == 3 {
            Some(parse_string(&values[2])?)
        } else {
            None
        };
        Ok(ProjectileNames {
            short_name: parse_string(&values[0])?,
            long_name: parse_string(&values[1])?,
            file_name,
        })
    }
}

// We can detect the version by the number of lines.
#[derive(Debug, Ord, PartialOrd, Eq, PartialEq)]
enum ProjectileTypeVersion {
    V0 = 81, // USNF only
    V1 = 84, // MF, ATF, Nato
    V2 = 90, // USNF97, Gold, & FA
}

impl ProjectileTypeVersion {
    fn from_len(n: usize) -> Result<Self> {
        Ok(match n {
            81 => ProjectileTypeVersion::V0,
            84 => ProjectileTypeVersion::V1,
            90 => ProjectileTypeVersion::V2,
            _ => bail!("unknown projectile type version for length: {}", n),
        })
    }
}

make_type_struct![
ProjectileType(ot: ObjectType, version: ProjectileTypeVersion) {                    // AA11.JT
    (DWord, [Dec,Hex],               "flags", Unsigned, flags0,                    u32, V0, panic!()), // dword $1204f ; flags
    (Num,   [Dec],              "projsInPod", Unsigned, projs_in_pod,              u32, V0, panic!()), // word 1 ; projsInPod // or byte 1
    (Byte,  [Dec],              "structType", Unsigned, struct_type,                u8, V0, panic!()), // byte 10 ; structType
    (Ptr,   [Sym],                "si_names",   Custom, si_names,      ProjectileNames, V0, panic!()), // ptr si_names
    (Word,  [Dec],                  "weight", Unsigned, weight,                    u16, V0, panic!()), // word 0 ; weight
    (Byte,  [Dec,Hex],               "flags", Unsigned, flags1,                     u8, V0, panic!()), // byte $0 ; flags
    (Byte,  [Dec],                     "sig", Unsigned, sig,                        u8, V0, panic!()), // byte 2 ; sig
    (Byte,  [Dec,Hex],               "flags", Unsigned, flags2,                     u8, V0, panic!()), // byte $0 ; flags
    (Byte,  [Dec],                "lookDown", Unsigned, look_down,                  u8, V0, panic!()), // byte 0 ; lookDown
    (Byte,  [Dec],       "dopplerSpeedAbove", Unsigned, doppler_speed_above,        u8, V0, panic!()), // byte 0 ; dopplerSpeedAbove
    (Byte,  [Dec],       "dopplerSpeedBelow", Unsigned, doppler_speed_below,        u8, V0, panic!()), // byte 0 ; dopplerSpeedBelow
    (Byte,  [Dec],         "dopplerMinRange", Unsigned, doppler_min_range,          u8, V0, panic!()), // byte 0 ; dopplerMinRange
    (Byte,  [Dec],               "allAspect", Unsigned, all_aspect,                 u8, V0, panic!()), // byte 30 ; allAspect
    (Word,  [Dec],                       "h", Unsigned, h0,                        u16, V0, panic!()), // word 14560 ; h
    (Word,  [Dec],                       "p", Unsigned, p0,                        u16, V0, panic!()), // word 14560 ; p
    (DWord, [Dec,Car],            "minRange", Unsigned, min_range0,                u32, V0, panic!()), // dword ^0 ; minRange
    (DWord, [Dec,Car],            "maxRange", Unsigned, max_range0,                u32, V0, panic!()), // dword ^60000 ; maxRange
    (DWord, [Dec,Car,Hex],          "minAlt",   Signed, min_alt0,                  i32, V0, panic!()), // dword $80000000 ; minAlt
    (DWord, [Dec,Car,Hex],          "maxAlt",   Signed, max_alt0,                  i32, V0, panic!()), // dword $7fffffff ; maxAlt
    (Word,  [Dec],                       "h", Unsigned, h1,                        u16, V0, panic!()), // word 14560 ; h
    (Word,  [Dec],                       "p", Unsigned, p1,                        u16, V0, panic!()), // word 14560 ; p
    (DWord, [Dec,Car],            "minRange", Unsigned, min_range1,                u32, V0, panic!()), // dword ^2000 ; minRange
    (DWord, [Dec,Car],            "maxRange", Unsigned, max_range1,                u32, V0, panic!()), // dword ^60000 ; maxRange
    (DWord, [Dec,Car,Hex],          "minAlt",   Signed, min_alt1,                  i32, V0, panic!()), // dword $80000000 ; minAlt
    (DWord, [Dec,Car,Hex],          "maxAlt",   Signed, max_alt1,                  i32, V0, panic!()), // dword $7fffffff ; maxAlt
    (Byte,  [Dec],        "chaffFlareChance", Unsigned, chaff_flare_chance,         u8, V0, panic!()), // byte 50 ; chaffFlareChance
    (Byte,  [Dec],         "deceptionChance", Unsigned, deception_chance,           u8, V0, panic!()), // byte 50 ; deceptionChance
    (Byte,  [Dec],                  "trackT", Unsigned, track_t,                    u8, V0, panic!()), // byte 12 ; trackT
    (Byte,  [Dec],               "trackMaxG", Unsigned, track_max_g,                u8, V0, panic!()), // byte 5 ; trackMaxG
    (Byte,  [Dec],         "targetSunChance", Unsigned, target_sun_chance,          u8, V0, panic!()), // byte 10 ; targetSunChance
    (Word,  [Dec],       "randomFirePercent", Unsigned, random_fire_percent,       u16, V2, 0),        // word 0 ; randomFirePercent
    (Word,  [Dec],       "offsetFirePercent", Unsigned, offset_fire_percent,       u16, V2, 0),        // word 0 ; offsetFirePercent
    (Word,  [Dec],             "offsetFireH", Unsigned, offset_fire_h,             u16, V2, 0),        // word 0 ; offsetFireH
    (Word,  [Dec],             "offsetFireP", Unsigned, offset_fire_p,             u16, V2, 0),        // word 0 ; offsetFireP
    (Byte,  [Dec],     "actualRoundsPerGame", Unsigned, actual_rounds_per_game,     u8, V2, 0),        // byte 1 ; actualRoundsPerGame
    (Byte,  [Dec],       "gameRoundsInBurst", Unsigned, game_rounds_in_burst,       u8, V0, panic!()), // byte 1 ; gameRoundsInBurst
    (Byte,  [Dec], "gameRoundsInCarpetBurst", Unsigned, game_rounds_in_carpet_burst,u8, V0, panic!()), // byte 1 ; gameRoundsInCarpetBurst
    (Byte,  [Dec],              "gameBurstT", Unsigned, game_burst_t,               u8, V0, panic!()), // byte 0 ; gameBurstT
    (Byte,  [Dec],                 "reloadT", Unsigned, reload_t,                   u8, V0, panic!()), // byte 24 ; reloadT
    (Byte,  [Dec],            "startupShots", Unsigned, startup_shots,              u8, V0, panic!()), // byte 0 ; startupShots
    (Byte,  [Dec],                  "hSines", Unsigned, h_sines,                    u8, V0, panic!()), // byte 0 ; hSines
    (Byte,  [Dec],            "hSineDegrees", Unsigned, h_sine_degrees,             u8, V0, panic!()), // byte 0 ; hSineDegrees
    (Byte,  [Dec],                  "vSines", Unsigned, v_sines,                    u8, V0, panic!()), // byte 0 ; vSines
    (Byte,  [Dec],            "vSineDegrees", Unsigned, v_sine_degrees,             u8, V2, 0),        // byte 0 ; vSineDegrees
    (Byte,  [Dec],                  "maxAON", Unsigned, max_aon,                    u8, V0, panic!()), // byte 31 ; maxAON
    (Word,  [Dec],            "initialSpeed", Unsigned, initial_speed,             u16, V0, panic!()), // word 0 ; initialSpeed
    (Word,  [Dec],              "finalSpeed", Unsigned, final_speed,               u16, V0, panic!()), // word 1026 ; finalSpeed
    (Word,  [Dec],                 "igniteT", Unsigned, ignite_t,                  u16, V0, panic!()), // word 0 ; igniteT
    (Word,  [Dec],                   "fuelT", Unsigned, fuel_t,                    u16, V0, panic!()), // word 104 ; fuelT
    (Word,  [Dec],                 "removeT", Unsigned, remove_t,                  u16, V0, panic!()), // word 208 ; removeT
    (Word,  [Dec],         "poweredTurnRate", Unsigned, powered_turn_rate,         u16, V0, panic!()), // word 21840 ; poweredTurnRate
    (Word,  [Dec],       "unpoweredTurnRate", Unsigned, unpowered_turn_rate,       u16, V0, panic!()), // word 16380 ; unpoweredTurnRate
    (Byte,  [Dec],          "performanceAt0", Unsigned, performance_at_0,           u8, V0, panic!()), // byte 75 ; performanceAt0
    (Byte,  [Dec],         "performanceAt20", Unsigned, performance_at_20,          u8, V0, panic!()), // byte 100 ; performanceAt20
    (Byte,  [Dec],             "cruise1Dist", Unsigned, cruise1_dist,               u8, V0, panic!()), // byte 0 ; cruise1Dist
    (Byte,  [Dec],              "cruise1Alt", Unsigned, cruise1_alt,                u8, V0, panic!()), // byte 0 ; cruise1Alt
    (Byte,  [Dec],             "cruise2Dist", Unsigned, cruise2_dist,               u8, V0, panic!()), // byte 0 ; cruise2Dist
    (Byte,  [Dec],              "cruise2Alt", Unsigned, cruise2_alt,                u8, V0, panic!()), // byte 0 ; cruise2Alt
    (Word,  [Dec],                "jinkSize", Unsigned, jink_size,                 u16, V0, panic!()), // word 546 ; jinkSize
    (Word,  [Dec],                   "jinkT", Unsigned, jink_t,                    u16, V0, panic!()), // word 1 ; jinkT
    (Word,  [Dec],              "totalJinkT", Unsigned, total_jink_t,              u16, V0, panic!()), // word 16 ; totalJinkT
    (Byte,  [Dec],            "launchRetard", Unsigned, launch_retard,              u8, V0, panic!()), // byte 100 ; launchRetard
    (Byte,  [Dec],               "smokeType", Unsigned, smoke_type,                 u8, V0, panic!()), // byte 8 ; smokeType
    (Byte,  [Dec],               "smokeFreq", Unsigned, smoke_freq,                 u8, V0, panic!()), // byte 12 ; smokeFreq
    (Byte,  [Dec],          "smokeExistTime", Unsigned, smoke_exist_time,           u8, V0, panic!()), // byte 2 ; smokeExistTime
    (Byte,  [Dec],          "smokeStartSize", Unsigned, smoke_start_size,           u8, V0, panic!()), // byte 15 ; smokeStartSize
    (Byte,  [Dec],            "smokeEndSize", Unsigned, smoke_end_size,             u8, V0, panic!()), // byte 50 ; smokeEndSize
    (Byte,  [Dec],             "chances [i]", Unsigned, chances_i_0,                u8, V0, panic!()), // byte 75 ; chances [i]
    (Byte,  [Dec],             "chances [i]", Unsigned, chances_i_1,                u8, V0, panic!()), // byte 75 ; chances [i]
    (Byte,  [Dec],             "chances [i]", Unsigned, chances_i_2,                u8, V0, panic!()), // byte 56 ; chances [i]
    (Byte,  [Dec],             "chances [i]", Unsigned, chances_i_3,                u8, V0, panic!()), // byte 0 ; chances [i]
    (Byte,  [Dec],            "taaHitChange", Unsigned, taa_hit_change,             u8, V0, panic!()), // byte 0 ; taaHitChange
    (Byte,  [Dec],          "climbHitChange", Unsigned, climb_hit_change,           u8, V0, panic!()), // byte 0 ; climbHitChange
    (Byte,  [Dec],              "gHitChange", Unsigned, g_hit_change,               u8, V0, panic!()), // byte 10 ; gHitChange
    (Byte,  [Dec],            "airHitChange", Unsigned, air_hit_change,             u8, V0, panic!()), // byte 0 ; airHitChange
    (Byte,  [Dec],          "speedHitChange", Unsigned, speed_hit_change,           u8, V0, panic!()), // byte 0 ; speedHitChange
    (Byte,  [Dec],             "speedHitMin", Unsigned, speed_hit_min,              u8, V0, panic!()), // byte 0 ; speedHitMin
    (Byte,  [Dec],    "predictableHitChange", Unsigned, predictable_hit_change,     u8, V0, panic!()), // byte 30 ; predictableHitChange
    (Byte,  [Dec],          "bigPlaneChange", Unsigned, big_plane_change,           u8, V1, 0),        // byte 0 ; bigPlaneChange
    (Byte,  [Dec],                   "gMiss", Unsigned, g_miss,                     u8, V0, panic!()), // byte 9 ; gMiss
    (Word,  [Dec],                "fuzeArmT", Unsigned, fuze_arm_t,                u16, V0, panic!()), // word 4 ; fuzeArmT
    (Word,  [Dec],              "fuzeRadius", Unsigned, fuze_radius,               u16, V0, panic!()), // word 100 ; fuzeRadius
    (Byte,  [Dec],      "sideHitFuzeFailure", Unsigned, side_hit_fuze_failure,      u8, V0, panic!()), // byte 0 ; sideHitFuzeFailure
    (Byte,  [Dec],          "expTypeForLand", Unsigned, exp_type_for_land,          u8, V0, panic!()), // byte 21 ; expTypeForLand
    (Byte,  [Dec],         "expTypeForWater", Unsigned, exp_type_for_water,         u8, V0, panic!()), // byte 34 ; expTypeForWater
    (Ptr,   [Sym],               "fireSound",   PtrStr, fire_sound,             String, V0, panic!()), // ptr fireSound
    (Word,  [Dec],              "maxSndDist", Unsigned, max_snd_dist,              u16, V0, panic!()), // word 6000 ; maxSndDist
    (Word,  [Dec],                 "freqAdj", Unsigned, freq_adj,                  u16, V0, panic!()), // word 0 ; freqAdj
    (Word,  [Dec],  "collateralDamageRadius", Unsigned, collateral_damage_radius,  u16, V1, 0),        // word 750 ; collateralDamageRadius
    (Word,  [Dec], "collateralDamagePercent", Unsigned, collateral_damage_percent, u16, V1, 0)         // word 35 ; collateralDamagePercent
}];

impl ProjectileType {
    pub fn from_text(data: &str) -> Result<Self> {
        let lines = data.lines().collect::<Vec<&str>>();
        ensure!(
            lines[0] == "[brent's_relocatable_format]",
            "not a type file"
        );
        let pointers = parse::find_pointers(&lines)?;
        let obj_lines = parse::find_section(&lines, "OBJ_TYPE")?;
        let obj = ObjectType::from_lines((), &obj_lines, &pointers)?;
        let proj_lines = parse::find_section(&lines, "PROJ_TYPE")?;
        Self::from_lines(obj, &proj_lines, &pointers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lib::{from_dos_string, Libs};

    #[test]
    fn it_can_parse_all_projectile_files() -> Result<()> {
        let libs = Libs::for_testing()?;
        for (game, _palette, catalog) in libs.all() {
            for fid in catalog.find_with_extension("JT")? {
                let meta = catalog.stat(fid)?;
                println!("At: {}:{:13} @ {}", game.test_dir, meta.name(), meta.path());
                let contents = from_dos_string(catalog.read(fid)?);
                let jt = ProjectileType::from_text(contents.as_ref())?;
                // Only one misspelling in 2500 files.
                assert!(jt.ot.file_name() == meta.name() || meta.name() == "SMALLARM.JT");
            }
        }

        Ok(())
    }
}
