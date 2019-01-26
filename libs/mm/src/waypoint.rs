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
use failure::{bail, err_msg, Fallible};
use nalgebra::Vector3;

// w_index 0
// w_flags 1
// w_goal 0
// w_next 0
// w_pos2 0   0   -36199 15000 1734859
// w_speed 0
// w_wng 0 0 0 0
// w_react 0 0 0
// w_searchDist 0
// w_preferredTargetId 0
// w_name ^A^A
#[allow(dead_code)]
pub struct Waypoint {
    pub index: u8,
    flags: u8,
    goal: bool,
    next: bool,
    pos: Vector3<f32>,
    speed: usize,
    wng: [u16; 4],
    react: [u32; 3],
    search_dist: u8,
    // preferred_target_id: 0
    // name: ""
}

impl Waypoint {
    pub(crate) fn from_lines(lines: &[&str], offset: &mut usize) -> Fallible<Self> {
        let mut index = None;
        let mut flags = None;
        let mut goal = None;
        let mut next = None;
        let mut pos = None;
        let mut speed = None;
        let mut wng = None;
        let mut react = None;
        let mut search_dist = None;

        while lines[*offset].trim() != "." {
            let parts = lines[*offset]
                .trim()
                .split(' ')
                .filter(|&s| s != "")
                .collect::<Vec<&str>>();
            if parts.is_empty() {
                break;
            }
            match parts[0].trim_left() {
                "w_index" => index = Some(parts[1].parse::<u8>()?),
                "w_flags" => flags = Some(parts[1].parse::<u8>()?),
                "w_goal" => goal = Some(parts[1] == "1"),
                "w_next" => next = Some(parts[1] == "1"),
                "w_pos2" => {
                    assert_eq!(parts[1].parse::<u8>()?, 0);
                    assert_eq!(parts[2].parse::<u8>()?, 0);
                    pos = Some(Vector3::new(
                        parts[3].parse::<f32>()?,
                        parts[4].parse::<f32>()?,
                        parts[5].parse::<f32>()?,
                    ));
                }
                "w_speed" => speed = Some(parts[1].parse::<usize>()?),
                "w_wng" => {
                    wng = Some([
                        parts[1].parse::<u16>()?,
                        parts[2].parse::<u16>()?,
                        parts[3].parse::<u16>()?,
                        parts[4].parse::<u16>()?,
                    ]);
                }
                "w_react" => {
                    react = Some([
                        parts[1].parse::<u32>()?,
                        parts[2].parse::<u32>()?,
                        parts[3].parse::<u32>()?,
                    ]);
                }
                "w_searchDist" => search_dist = Some(parts[1].parse::<u8>()?),
                "w_preferredTargetId" => assert_eq!(parts[1], "0"),
                "w_name" => assert_eq!(parts[1], "\x01\x01"),
                _ => {
                    bail!("unknown waypoint key: {}", parts[0]);
                }
            }
            *offset += 1;
        }
        *offset += 1;
        Ok(Waypoint {
            index: index.ok_or_else(|| {
                err_msg(format!(
                    "mm:waypoint: index not set in waypoint ending at {}",
                    *offset
                ))
            })?,
            flags: flags.ok_or_else(|| {
                err_msg(format!(
                    "mm:waypoint: flags not set in waypoint ending at {}",
                    *offset
                ))
            })?,
            goal: goal.ok_or_else(|| {
                err_msg(format!(
                    "mm:waypoint: goal not set in waypoint ending at {}",
                    *offset
                ))
            })?,
            next: next.ok_or_else(|| {
                err_msg(format!(
                    "mm:waypoint: next not set in waypoint ending at {}",
                    *offset
                ))
            })?,
            pos: pos.ok_or_else(|| {
                err_msg(format!(
                    "mm:waypoint: pos not set in waypoint ending at {}",
                    *offset
                ))
            })?,
            speed: speed.ok_or_else(|| {
                err_msg(format!(
                    "mm:waypoint: speed not set in waypoint ending at {}",
                    *offset
                ))
            })?,
            wng: wng.ok_or_else(|| {
                err_msg(format!(
                    "mm:waypoint: wng not set in waypoint ending at {}",
                    *offset
                ))
            })?,
            react: react.ok_or_else(|| {
                err_msg(format!(
                    "mm:waypoint: react not set in waypoint ending at {}",
                    *offset
                ))
            })?,
            search_dist: search_dist.ok_or_else(|| {
                err_msg(format!(
                    "mm:waypoint: searchDist not set in waypoint ending at {}",
                    *offset
                ))
            })?,
        })
    }
}
