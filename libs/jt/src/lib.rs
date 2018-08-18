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
extern crate failure;
#[macro_use]
extern crate ot;

use failure::Fallible;
use ot::{
    parse, parse::{check_num_type, consume_ptr, parse_one, Repr, TryConvert}, ObjectType, Resource,
};
use std::collections::HashMap;

// placeholder
pub struct Sound {}
impl Resource for Sound {
    fn from_file(_: &str) -> Fallible<Self> {
        Ok(Sound {})
    }
}

struct ProjectileNames {
    short_name: String,
    long_name: String,
    file_name: Option<String>,
}

impl<'a> TryConvert<Vec<String>> for ProjectileNames {
    type Error = failure::Error;
    fn try_from(value: Vec<String>) -> Fallible<ProjectileNames> {
        ensure!(value.len() >= 2, "expected at least 2 names in si_names");
        let file_name = if value.len() == 3 {
            Some(parse::string(&value[2])?)
        } else {
            None
        };
        return Ok(ProjectileNames {
            short_name: parse::string(&value[0])?,
            long_name: parse::string(&value[1])?,
            file_name,
        });
    }
}

impl<'a> TryConvert<Vec<String>> for Sound {
    type Error = failure::Error;
    fn try_from(value: Vec<String>) -> Fallible<Sound> {
        ensure!(value.len() <= 1, "expected 0 or 1 names in sound");
        if value.len() > 0 {
            return Ok(Sound::from_file(&value[0])?);
        }
        bail!("not a sound")
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
    fn from_len(n: usize) -> Fallible<Self> {
        return Ok(match n {
            81 => ProjectileTypeVersion::V0,
            84 => ProjectileTypeVersion::V1,
            90 => ProjectileTypeVersion::V2,
            _ => bail!("unknown projectile type version for length: {}", n),
        });
    }
}

struct ProjectilesInPod(u16);

impl TryConvert<u16> for ProjectilesInPod {
    type Error = failure::Error;
    fn try_from(value: u16) -> Fallible<ProjectilesInPod> {
        Ok(ProjectilesInPod(value))
    }
}

make_type_struct![ProjectileType(obj: ObjectType, version: ProjectileTypeVersion) {       // AA11.JT
    (flags0,                    u32, "flags",            ([Dec,Hex]: u32), V0, panic!()), // dword $1204f ; flags
    (projs_in_pod, ProjectilesInPod, "projsInPod",       (Dec: (u8, u16)), V0, panic!()), // word 1 ; projsInPod
    (struct_type,                u8, "structType",              (Dec: u8), V0, panic!()), // byte 10 ; structType
    (si_names,      ProjectileNames, "si_names",                      Ptr, V0, panic!()), // ptr si_names
    (weight,                    u16, "weight",                 (Dec: u16), V0, panic!()), // word 0 ; weight
    (flags1,                     u8, "flags",             ([Dec,Hex]: u8), V0, panic!()), // byte $0 ; flags
    (sig,                        u8, "sig",                     (Dec: u8), V0, panic!()), // byte 2 ; sig
    (flags2,                     u8, "flags",             ([Dec,Hex]: u8), V0, panic!()), // byte $0 ; flags
    (look_down,                  u8, "lookDown",                (Dec: u8), V0, panic!()), // byte 0 ; lookDown
    (doppler_speed_above,        u8, "dopplerSpeedAbove",       (Dec: u8), V0, panic!()), // byte 0 ; dopplerSpeedAbove
    (doppler_speed_below,        u8, "dopplerSpeedBelow",       (Dec: u8), V0, panic!()), // byte 0 ; dopplerSpeedBelow
    (doppler_min_range,          u8, "dopplerMinRange",         (Dec: u8), V0, panic!()), // byte 0 ; dopplerMinRange
    (all_aspect,                 u8, "allAspect",               (Dec: u8), V0, panic!()), // byte 30 ; allAspect
    (h0,                        u16, "h",                      (Dec: u16), V0, panic!()), // word 14560 ; h
    (p0,                        u16, "p",                      (Dec: u16), V0, panic!()), // word 14560 ; p
    (min_range0,                u32, "minRange",         ([Dec,Car]: u32), V0, panic!()), // dword ^0 ; minRange
    (max_range0,                u32, "maxRange",         ([Dec,Car]: u32), V0, panic!()), // dword ^60000 ; maxRange
    (min_alt0,                  i32, "minAlt",                   Altitude, V0, panic!()), // dword $80000000 ; minAlt
    (max_alt0,                  i32, "maxAlt",                   Altitude, V0, panic!()), // dword $7fffffff ; maxAlt
    (h1,                        u16, "h",                      (Dec: u16), V0, panic!()), // word 14560 ; h
    (p1,                        u16, "p",                      (Dec: u16), V0, panic!()), // word 14560 ; p
    (min_range1,                u32, "minRange",         ([Dec,Car]: u32), V0, panic!()), // dword ^2000 ; minRange
    (max_range1,                u32, "maxRange",         ([Dec,Car]: u32), V0, panic!()), // dword ^60000 ; maxRange
    (min_alt1,                  i32, "minAlt",                   Altitude, V0, panic!()), // dword $80000000 ; minAlt
    (max_alt1,                  i32, "maxAlt",                   Altitude, V0, panic!()), // dword $7fffffff ; maxAlt
    (chaff_flare_chance,         u8, "chaffFlareChance",        (Dec: u8), V0, panic!()), // byte 50 ; chaffFlareChance
    (deception_chance,           u8, "deceptionChance",         (Dec: u8), V0, panic!()), // byte 50 ; deceptionChance
    (track_t,                    u8, "trackT",                  (Dec: u8), V0, panic!()), // byte 12 ; trackT
    (track_max_g,                u8, "trackMaxG",               (Dec: u8), V0, panic!()), // byte 5 ; trackMaxG
    (target_sun_chance,          u8, "targetSunChance",         (Dec: u8), V0, panic!()), // byte 10 ; targetSunChance
    (random_fire_percent,       u16, "randomFirePercent",      (Dec: u16), V2, 0),        // word 0 ; randomFirePercent
    (offset_fire_percent,       u16, "offsetFirePercent",      (Dec: u16), V2, 0),        // word 0 ; offsetFirePercent
    (offset_fire_h,             u16, "offsetFireH",            (Dec: u16), V2, 0),        // word 0 ; offsetFireH
    (offset_fire_p,             u16, "offsetFireP",            (Dec: u16), V2, 0),        // word 0 ; offsetFireP
    (actual_rounds_per_game,     u8, "actualRoundsPerGame",     (Dec: u8), V2, 0),        // byte 1 ; actualRoundsPerGame
    (game_rounds_in_burst,       u8, "gameRoundsInBurst",       (Dec: u8), V0, panic!()), // byte 1 ; gameRoundsInBurst
    (game_rounds_in_carpet_burst,u8, "gameRoundsInCarpetBurst", (Dec: u8), V0, panic!()), // byte 1 ; gameRoundsInCarpetBurst
    (game_burst_t,               u8, "gameBurstT",              (Dec: u8), V0, panic!()), // byte 0 ; gameBurstT
    (reload_t,                   u8, "reloadT",                 (Dec: u8), V0, panic!()), // byte 24 ; reloadT
    (startup_shots,              u8, "startupShots",            (Dec: u8), V0, panic!()), // byte 0 ; startupShots
    (h_sines,                    u8, "hSines",                  (Dec: u8), V0, panic!()), // byte 0 ; hSines
    (h_sine_degrees,             u8, "hSineDegrees",            (Dec: u8), V0, panic!()), // byte 0 ; hSineDegrees
    (v_sines,                    u8, "vSines",                  (Dec: u8), V0, panic!()), // byte 0 ; vSines
    (v_sine_degrees,             u8, "vSineDegrees",            (Dec: u8), V2, 0),        // byte 0 ; vSineDegrees
    (max_aon,                    u8, "maxAON",                  (Dec: u8), V0, panic!()), // byte 31 ; maxAON
    (initial_speed,             u16, "initialSpeed",           (Dec: u16), V0, panic!()), // word 0 ; initialSpeed
    (final_speed,               u16, "finalSpeed",             (Dec: u16), V0, panic!()), // word 1026 ; finalSpeed
    (ignite_t,                  u16, "igniteT",                (Dec: u16), V0, panic!()), // word 0 ; igniteT
    (fuel_t,                    u16, "fuelT",                  (Dec: u16), V0, panic!()), // word 104 ; fuelT
    (remove_t,                  u16, "removeT",                (Dec: u16), V0, panic!()), // word 208 ; removeT
    (powered_turn_rate,         u16, "poweredTurnRate",        (Dec: u16), V0, panic!()), // word 21840 ; poweredTurnRate
    (unpowered_turn_rate,       u16, "unpoweredTurnRate",      (Dec: u16), V0, panic!()), // word 16380 ; unpoweredTurnRate
    (performance_at_0,           u8, "performanceAt0",          (Dec: u8), V0, panic!()), // byte 75 ; performanceAt0
    (performance_at_20,          u8, "performanceAt20",         (Dec: u8), V0, panic!()), // byte 100 ; performanceAt20
    (cruise1_dist,               u8, "cruise1Dist",             (Dec: u8), V0, panic!()), // byte 0 ; cruise1Dist
    (cruise1_alt,                u8, "cruise1Alt",              (Dec: u8), V0, panic!()), // byte 0 ; cruise1Alt
    (cruise2_dist,               u8, "cruise2Dist",             (Dec: u8), V0, panic!()), // byte 0 ; cruise2Dist
    (cruise2_alt,                u8, "cruise2Alt",              (Dec: u8), V0, panic!()), // byte 0 ; cruise2Alt
    (jink_size,                 u16, "jinkSize",               (Dec: u16), V0, panic!()), // word 546 ; jinkSize
    (jink_t,                    u16, "jinkT",                  (Dec: u16), V0, panic!()), // word 1 ; jinkT
    (total_jink_t,              u16, "totalJinkT",             (Dec: u16), V0, panic!()), // word 16 ; totalJinkT
    (launch_retard,              u8, "launchRetard",            (Dec: u8), V0, panic!()), // byte 100 ; launchRetard
    (smoke_type,                 u8, "smokeType",               (Dec: u8), V0, panic!()), // byte 8 ; smokeType
    (smoke_freq,                 u8, "smokeFreq",               (Dec: u8), V0, panic!()), // byte 12 ; smokeFreq
    (smoke_exist_time,           u8, "smokeExistTime",          (Dec: u8), V0, panic!()), // byte 2 ; smokeExistTime
    (smoke_start_size,           u8, "smokeStartSize",          (Dec: u8), V0, panic!()), // byte 15 ; smokeStartSize
    (smoke_end_size,             u8, "smokeEndSize",            (Dec: u8), V0, panic!()), // byte 50 ; smokeEndSize
    (chances_i_0,                u8, "chances [i]",             (Dec: u8), V0, panic!()), // byte 75 ; chances [i]
    (chances_i_1,                u8, "chances [i]",             (Dec: u8), V0, panic!()), // byte 75 ; chances [i]
    (chances_i_2,                u8, "chances [i]",             (Dec: u8), V0, panic!()), // byte 56 ; chances [i]
    (chances_i_3,                u8, "chances [i]",             (Dec: u8), V0, panic!()), // byte 0 ; chances [i]
    (taa_hit_change,             u8, "taaHitChange",            (Dec: u8), V0, panic!()), // byte 0 ; taaHitChange
    (climb_hit_change,           u8, "climbHitChange",          (Dec: u8), V0, panic!()), // byte 0 ; climbHitChange
    (g_hit_change,               u8, "gHitChange",              (Dec: u8), V0, panic!()), // byte 10 ; gHitChange
    (air_hit_change,             u8, "airHitChange",            (Dec: u8), V0, panic!()), // byte 0 ; airHitChange
    (speed_hit_change,           u8, "speedHitChange",          (Dec: u8), V0, panic!()), // byte 0 ; speedHitChange
    (speed_hit_min,              u8, "speedHitMin",             (Dec: u8), V0, panic!()), // byte 0 ; speedHitMin
    (predictable_hit_change,     u8, "predictableHitChange",    (Dec: u8), V0, panic!()), // byte 30 ; predictableHitChange
    (big_plane_change,           u8, "bigPlaneChange",          (Dec: u8), V1, 0),        // byte 0 ; bigPlaneChange
    (g_miss,                     u8, "gMiss",                   (Dec: u8), V0, panic!()), // byte 9 ; gMiss
    (fuze_arm_t,                u16, "fuzeArmT",               (Dec: u16), V0, panic!()), // word 4 ; fuzeArmT
    (fuze_radius,               u16, "fuzeRadius",             (Dec: u16), V0, panic!()), // word 100 ; fuzeRadius
    (side_hit_fuze_failure,      u8, "sideHitFuzeFailure",      (Dec: u8), V0, panic!()), // byte 0 ; sideHitFuzeFailure
    (exp_type_for_land,          u8, "expTypeForLand",          (Dec: u8), V0, panic!()), // byte 21 ; expTypeForLand
    (exp_type_for_water,         u8, "expTypeForWater",         (Dec: u8), V0, panic!()), // byte 34 ; expTypeForWater
    (fire_sound,              Sound, "fireSound",                     Ptr, V0, panic!()), // ptr fireSound
    (max_snd_dist,              u16, "maxSndDist",             (Dec: u16), V0, panic!()), // word 6000 ; maxSndDist
    (freq_adj,                  u16, "freqAdj",                (Dec: u16), V0, panic!()), // word 0 ; freqAdj
    (collateral_damage_radius,  u16, "collateralDamageRadius", (Dec: u16), V1, 0),        // word 750 ; collateralDamageRadius
    (collateral_damage_percent, u16, "collateralDamagePercent",(Dec: u16), V1, 0)         // word 35 ; collateralDamagePercent
}];

impl ProjectileType {
    pub fn from_str(data: &str) -> Fallible<Self> {
        let lines = data.lines().collect::<Vec<&str>>();
        ensure!(
            lines[0] == "[brent's_relocatable_format]",
            "not a type file"
        );
        let pointers = parse::find_pointers(&lines)?;
        let obj_lines = parse::find_section(&lines, "OBJ_TYPE")?;
        let obj = ObjectType::from_lines((), &obj_lines, &pointers)?;
        let proj_lines = parse::find_section(&lines, "PROJ_TYPE")?;
        return Self::from_lines(obj, &proj_lines, &pointers);
    }
}

#[cfg(test)]
extern crate omnilib;

#[cfg(test)]
mod tests {
    use super::*;
    use omnilib::OmniLib;

    #[test]
    fn it_can_parse_all_projectile_files() -> Fallible<()> {
        let omni = OmniLib::new_for_test_in_games(vec![
            "FA", "ATF", "ATFGOLD", "ATFNATO", "USNF", "MF", "USNF97",
        ])?;
        for (game, name) in omni.find_matching("*.JT")?.iter() {
            println!("{}:{} @ {}", game, name, omni.path(game, name)?);
            let contents = omni.library(game).load_text(name)?;
            let jt = ProjectileType::from_str(&contents)?;
            assert!(jt.obj.file_name() == *name || *name == "SMALLARM.JT");
            // println!(
            //     "{}:{:13}> {:08X} <> {} <> {}",
            //     game, name, jt.unk0, jt.obj.long_name, name
            // );
        }
        return Ok(());
    }
}
