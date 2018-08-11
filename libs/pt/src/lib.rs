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
extern crate nt;
extern crate ot;

use failure::Fallible;
use nt::NpcType;
use ot::parse;
use std::collections::HashMap;

#[allow(dead_code)]
pub struct EnvelopeCoord {
    speed: i16,
    altitude: u32,
}

impl EnvelopeCoord {
    pub fn from_lines(lines: &[&str], _pointers: &HashMap<&str, Vec<&str>>) -> Fallible<Self> {
        return Ok(EnvelopeCoord {
            speed: parse::word(lines[0])?,
            altitude: parse::dword(lines[1])?,
        });
    }
}

#[allow(dead_code)]
pub struct Envelope {
    g_load: i16,     // word -4 ; env [ii].gload
    count: i16,      // word 6 ; env [ii].count
    stall_lift: i16, // word 2 ; env [ii].stallLift
    max_speed: i16,  // word 4 ; env [ii].maxSpeed
    shape: [EnvelopeCoord; 20],
}

impl Envelope {
    pub fn from_lines(lines: &[&str], pointers: &HashMap<&str, Vec<&str>>) -> Fallible<Self> {
        return Ok(Envelope {
            g_load: parse::word(lines[0])?,
            count: parse::word(lines[1])?,
            stall_lift: parse::word(lines[2])?,
            max_speed: parse::word(lines[3])?,
            shape: [
                EnvelopeCoord::from_lines(&lines[4..6], pointers)?,
                EnvelopeCoord::from_lines(&lines[6..8], pointers)?,
                EnvelopeCoord::from_lines(&lines[8..10], pointers)?,
                EnvelopeCoord::from_lines(&lines[10..12], pointers)?,
                EnvelopeCoord::from_lines(&lines[12..14], pointers)?,
                EnvelopeCoord::from_lines(&lines[14..16], pointers)?,
                EnvelopeCoord::from_lines(&lines[16..18], pointers)?,
                EnvelopeCoord::from_lines(&lines[18..20], pointers)?,
                EnvelopeCoord::from_lines(&lines[20..22], pointers)?,
                EnvelopeCoord::from_lines(&lines[22..24], pointers)?,
                EnvelopeCoord::from_lines(&lines[24..26], pointers)?,
                EnvelopeCoord::from_lines(&lines[26..28], pointers)?,
                EnvelopeCoord::from_lines(&lines[28..30], pointers)?,
                EnvelopeCoord::from_lines(&lines[30..32], pointers)?,
                EnvelopeCoord::from_lines(&lines[32..34], pointers)?,
                EnvelopeCoord::from_lines(&lines[34..36], pointers)?,
                EnvelopeCoord::from_lines(&lines[36..38], pointers)?,
                EnvelopeCoord::from_lines(&lines[38..40], pointers)?,
                EnvelopeCoord::from_lines(&lines[40..42], pointers)?,
                EnvelopeCoord::from_lines(&lines[42..44], pointers)?,
            ],
        });
    }
}

#[allow(dead_code)]
pub struct PlaneType {
    npc: NpcType,

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

impl PlaneType {
    pub fn from_str(data: &str) -> Fallible<Self> {
        let lines = data.lines().collect::<Vec<&str>>();
        ensure!(
            lines[0] == "[brent's_relocatable_format]",
            "not a type file"
        );
        let pointers = parse::find_pointers(&lines)?;
        return Self::from_lines(&lines, &pointers);
    }

    fn from_lines(lines: &Vec<&str>, pointers: &HashMap<&str, Vec<&str>>) -> Fallible<Self> {
        let npc = NpcType::from_lines(lines, pointers)?;
        let lines = parse::find_section(&lines, "PLANE_TYPE")?;

        let envelope_lines = parse::follow_pointer(lines[1], pointers)?;
        let mut envelope = Vec::new();
        for chunk in envelope_lines.chunks(44) {
            envelope.push(Envelope::from_lines(chunk, pointers)?);
        }

        return Ok(PlaneType {
            npc,

            unk_flags: parse::dword(lines[0])?,
            envelope,
            negative_envelopes: parse::word(lines[2])?,
            positive_envelopes: parse::word(lines[3])?,
            max_speed_sea_level: parse::word(lines[4])?,
            max_speed_36a: parse::word(lines[5])?,
            unk6: parse::word(lines[6])?,
            unk7: parse::word(lines[7])?,
            unk8: parse::word(lines[8])?,
            unk9: parse::word(lines[9])?,
            unk10: parse::word(lines[10])?,
            unk11: parse::word(lines[11])?,
            unk12: parse::word(lines[12])?,
            unk13: parse::word(lines[13])?,
            unk14: parse::word(lines[14])?,
            unk15: parse::word(lines[15])?,
            unk16: parse::word(lines[16])?,
            unk17: parse::word(lines[17])?,
            unk18: parse::word(lines[18])?,
            unk19: parse::word(lines[19])?,
            unk20: parse::word(lines[20])?,
            unk21: parse::word(lines[21])?,
            unk22: parse::word(lines[22])?,
            unk23: parse::word(lines[23])?,
            unk24: parse::word(lines[24])?,
            unk25: parse::word(lines[25])?,
            unk26: parse::word(lines[26])?,
            unk27: parse::word(lines[27])?,
            unk28: parse::word(lines[28])?,
            unk29: parse::word(lines[29])?,
            unk30: parse::word(lines[30])?,
            unk31: parse::word(lines[31])?,
            unk32: parse::word(lines[32])?,
            unk33: parse::word(lines[33])?,
            unk34: parse::word(lines[34])?,
            unk35: parse::word(lines[35])?,
            unk36: parse::word(lines[36])?,
            unk37: parse::word(lines[37])?,
            unk38: parse::word(lines[38])?,
            unk39: parse::word(lines[39])?,
            unk40: parse::word(lines[40])?,
            unk41: parse::word(lines[41])?,
            unk42: parse::word(lines[42])?,
            unk43: parse::word(lines[43])?,
            unk44: parse::word(lines[44])?,
            unk45: parse::word(lines[45])?,
            unk46: parse::word(lines[46])?,
            unk47: parse::word(lines[47])?,
            unk48: parse::word(lines[48])?,
            unk49: parse::word(lines[49])?,
            unk50: parse::word(lines[50])?,
            unk51: parse::word(lines[51])?,
            unk52: parse::word(lines[52])?,
            unk53: parse::word(lines[53])?,
            unk54: parse::word(lines[54])?,
            unk55: parse::word(lines[55])?,
            unk56: parse::word(lines[56])?,
            unk57: parse::word(lines[57])?,
            unk58: parse::word(lines[58])?,
            unk59: parse::word(lines[59])?,
            unk60: parse::word(lines[60])?,
            unk61: parse::word(lines[61])?,
            unk62: parse::word(lines[62])?,
            unk63: parse::word(lines[63])?,
            unk64: parse::word(lines[64])?,
            unk65: parse::word(lines[65])?,
            unk66: parse::word(lines[66])?,
            unk67: parse::word(lines[67])?,
            unk68: parse::word(lines[68])?,
            unk69: parse::word(lines[69])?,
            unk70: parse::word(lines[70])?,
            unk_engine_count: parse::byte(lines[71])?,
            unk72: parse::word(lines[72])?,
            unk_thrust_100: parse::dword(lines[73])?,
            unk_thrust_after: parse::dword(lines[74])?,
            unk75: parse::word(lines[75])?,
            unk76: parse::word(lines[76])?,
            unk77: parse::word(lines[77])?,
            unk78: parse::word(lines[78])?,
            unk79: parse::word(lines[79])?,
            unk80: parse::word(lines[80])?,
            unk81: parse::word(lines[81])?,
            unk_fuel_capacity: parse::dword(lines[82])?,
            unk83: parse::word(lines[83])?,
            unk84: parse::word(lines[84])?,
            unk_air_brake_drag: parse::word(lines[85])?,
            unk_wheel_brake_drag: parse::word(lines[86])?,
            unk_flap_drag: parse::word(lines[87])?,
            unk_gear_drag: parse::word(lines[88])?,
            unk_bay_drag: parse::word(lines[89])?,
            unk_flaps_lift: parse::word(lines[90])?,
            unk_loadout_drag: parse::word(lines[91])?,
            unk_loadout_g_drag: parse::word(lines[92])?,
            unk93: parse::word(lines[93])?,
            unk94: parse::word(lines[94])?,
            unk95: parse::word(lines[95])?,
            structural_speed_warning: parse::word(lines[96])?,
            structural_speed_limit: parse::word(lines[97])?,
            unk_system_maintainence: [
                parse::byte(lines[98])?,
                parse::byte(lines[99])?,
                parse::byte(lines[100])?,
                parse::byte(lines[101])?,
                parse::byte(lines[102])?,
                parse::byte(lines[103])?,
                parse::byte(lines[104])?,
                parse::byte(lines[105])?,
                parse::byte(lines[106])?,
                parse::byte(lines[107])?,
                parse::byte(lines[108])?,
                parse::byte(lines[109])?,
                parse::byte(lines[110])?,
                parse::byte(lines[111])?,
                parse::byte(lines[112])?,
                parse::byte(lines[113])?,
                parse::byte(lines[114])?,
                parse::byte(lines[115])?,
                parse::byte(lines[116])?,
                parse::byte(lines[117])?,
                parse::byte(lines[118])?,
                parse::byte(lines[119])?,
                parse::byte(lines[120])?,
                parse::byte(lines[121])?,
                parse::byte(lines[122])?,
                parse::byte(lines[123])?,
                parse::byte(lines[124])?,
                parse::byte(lines[125])?,
                parse::byte(lines[126])?,
                parse::byte(lines[127])?,
                parse::byte(lines[128])?,
                parse::byte(lines[129])?,
                parse::byte(lines[130])?,
                parse::byte(lines[131])?,
                parse::byte(lines[132])?,
                parse::byte(lines[133])?,
                parse::byte(lines[134])?,
                parse::byte(lines[135])?,
                parse::byte(lines[136])?,
                parse::byte(lines[137])?,
                parse::byte(lines[138])?,
                parse::byte(lines[139])?,
                parse::byte(lines[140])?,
                parse::byte(lines[141])?,
                parse::byte(lines[142])?,
            ],
            unk_maintainence_per_mission: parse::word(lines[143])?,
            unk_maintainence_multiplier: parse::word(lines[144])?,
            unk_max_takeoff_weight: parse::dword(lines[145])?,
        });
    }
}

#[cfg(test)]
extern crate omnilib;

#[cfg(test)]
mod tests {
    use super::*;
    use omnilib::OmniLib;

    #[test]
    fn it_can_parse_all_plane_files() -> Fallible<()> {
        let omni = OmniLib::new_for_test_in_games(vec!["FA"])?;
        for (game, name) in omni.find_matching("*.PT")?.iter() {
            let contents = omni.library(game).load_text(name)?;
            let pt = PlaneType::from_str(&contents)?;
            assert_eq!(pt.npc.obj.file_name, *name);
            println!(
                "{}:{:13}> {:08X} <> {}",
                game, name, pt.unk_max_takeoff_weight, pt.npc.obj.long_name
            );
        }
        return Ok(());
    }
}
