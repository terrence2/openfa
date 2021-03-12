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
use anyhow::{anyhow, bail, Result};
use nalgebra::Vector3;
use std::str::SplitAsciiWhitespace;

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
#[derive(Clone, Debug)]
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
    pub(crate) fn from_tokens(tokens: &mut SplitAsciiWhitespace) -> Result<Self> {
        let mut index = None;
        let mut flags = None;
        let mut goal = None;
        let mut next = None;
        let mut pos = None;
        let mut speed = None;
        let mut wng = None;
        let mut react = None;
        let mut search_dist = None;

        while let Some(token) = tokens.next() {
            match token {
                "w_index" => index = Some(tokens.next().expect("w_index").parse::<u8>()?),
                "w_flags" => flags = Some(tokens.next().expect("w_flags").parse::<u8>()?),
                "w_goal" => goal = Some(tokens.next().expect("w_goal") == "1"),
                "w_next" => next = Some(tokens.next().expect("w_next") == "1"),
                "w_pos2" => {
                    assert_eq!(tokens.next().expect("pos2 a").parse::<u8>()?, 0);
                    assert_eq!(tokens.next().expect("pos2 b").parse::<u8>()?, 0);
                    pos = Some(Vector3::new(
                        tokens.next().expect("x").parse::<f32>()?,
                        tokens.next().expect("y").parse::<f32>()?,
                        tokens.next().expect("z").parse::<f32>()?,
                    ));
                }
                "w_speed" => speed = Some(tokens.next().expect("w_speed").parse::<usize>()?),
                "w_wng" => {
                    wng = Some([
                        tokens.next().expect("wng a").parse::<u16>()?,
                        tokens.next().expect("wng b").parse::<u16>()?,
                        tokens.next().expect("wng c").parse::<u16>()?,
                        tokens.next().expect("wng d").parse::<u16>()?,
                    ]);
                }
                "w_react" => {
                    react = Some([
                        tokens.next().expect("a").parse::<u32>()?,
                        tokens.next().expect("b").parse::<u32>()?,
                        tokens.next().expect("c").parse::<u32>()?,
                    ]);
                }
                "w_searchDist" => {
                    search_dist = Some(tokens.next().expect("w_searchDist").parse::<u8>()?)
                }
                "w_preferredTargetId" => {
                    assert_eq!(tokens.next().expect("w_preferredTargetId"), "0")
                }
                "w_name" => {
                    assert_eq!(tokens.next().expect("w_name"), "\x01\x01");
                    break;
                }
                v => {
                    bail!("unknown waypoint key: {}", v);
                }
            }
        }
        Ok(Waypoint {
            index: index.ok_or_else(|| anyhow!("mm:waypoint: index not set in waypoint",))?,
            flags: flags.ok_or_else(|| anyhow!("mm:waypoint: flags not set in waypoint",))?,
            goal: goal.ok_or_else(|| anyhow!("mm:waypoint: goal not set in waypoint",))?,
            next: next.ok_or_else(|| anyhow!("mm:waypoint: next not set in waypoint",))?,
            pos: pos.ok_or_else(|| anyhow!("mm:waypoint: pos not set in waypoint",))?,
            speed: speed.ok_or_else(|| anyhow!("mm:waypoint: speed not set in waypoint",))?,
            wng: wng.ok_or_else(|| anyhow!("mm:waypoint: wng not set in waypoint",))?,
            react: react.ok_or_else(|| anyhow!("mm:waypoint: react not set in waypoint",))?,
            search_dist: search_dist
                .ok_or_else(|| anyhow!("mm:waypoint: searchDist not set in waypoint",))?,
        })
    }
}
