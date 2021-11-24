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
use crate::{
    formation::WingFormation,
    util::{maybe_hex, parse_header_delimited},
};
use anyhow::{anyhow, bail, ensure, Result};
use itertools::Itertools;
use nalgebra::Vector3;
use std::str::SplitAsciiWhitespace;

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct Waypoints {
    waypoints: Vec<Waypoint>,
    for_alias: i32,
}

impl Waypoints {
    pub(crate) fn from_tokens(count: usize, tokens: &mut SplitAsciiWhitespace) -> Result<Self> {
        let mut waypoints = Vec::new();
        let mut wps_tokens = tokens.take_while(|&tok| tok != "w_for").peekable();
        for i in 0..count {
            ensure!(wps_tokens.next().expect("w_index") == "w_index");
            ensure!(wps_tokens.next().expect("w_index value").parse::<usize>()? == i);
            let mut wp_tokens = wps_tokens.peeking_take_while(|&tok| tok != "w_index");
            waypoints.push(Waypoint::from_tokens(i, &mut wp_tokens)?);
        }
        ensure!(
            waypoints.len() == count,
            "waypoint count does not match waypoints"
        );
        let for_alias = tokens.next().expect("w_for").parse::<i32>()?;
        ensure!(tokens.next().expect("waypoints .") == ".");
        Ok(Self {
            waypoints,
            for_alias,
        })
    }

    pub fn for_alias(&self) -> i32 {
        self.for_alias
    }
}

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
    pub index: usize,
    flags: u8,
    goal: bool,
    next: bool,
    pos: Vector3<f32>,
    speed: usize,
    wng_formation: WingFormation,
    react: [u32; 3],
    search_dist: u8,
    name: Option<String>,
    preferred_target_id: Option<u32>,
}

impl Waypoint {
    pub(crate) fn from_tokens<'a, I: Iterator<Item = &'a str>>(
        index: usize,
        wp_tokens: &mut I,
    ) -> Result<Self> {
        let mut flags = None;
        let mut goal = None;
        let mut next = None;
        let mut pos = None;
        let mut speed = None;
        let mut wng_formation = None;
        let mut react = None;
        let mut name = None;
        let mut search_dist = None;
        let mut preferred_target_id = None;

        // Take the index, peek until we find the end or the next index, signaling next waypoint
        while let Some(token) = wp_tokens.next() {
            match token {
                "w_index" => panic!("w_index in main loop"),
                "w_flags" => flags = Some(maybe_hex::<u8>(wp_tokens.next().expect("w_flags"))?),
                "w_goal" => goal = Some(wp_tokens.next().expect("w_goal") == "1"),
                "w_next" => next = Some(wp_tokens.next().expect("w_next") == "1"),
                "w_pos2" => {
                    let a = wp_tokens.next().expect("pos2 a").parse::<u8>()?;
                    let b = wp_tokens.next().expect("pos2 b").parse::<u8>()?;
                    ensure!(a == 0 || a == 1);
                    ensure!(b == 0);
                    pos = Some(Vector3::new(
                        wp_tokens.next().expect("x").parse::<f32>()?,
                        wp_tokens.next().expect("y").parse::<f32>()?,
                        wp_tokens.next().expect("z").parse::<f32>()?,
                    ));
                }
                "w_speed" => speed = Some(wp_tokens.next().expect("w_speed").parse::<usize>()?),
                "w_wng" => {
                    wng_formation = Some(WingFormation::from_tokens(wp_tokens)?);
                }
                "w_react" => {
                    react = Some([
                        maybe_hex::<u32>(wp_tokens.next().expect("a"))?,
                        maybe_hex::<u32>(wp_tokens.next().expect("b"))?,
                        maybe_hex::<u32>(wp_tokens.next().expect("c"))?,
                    ]);
                }
                "w_searchDist" => {
                    search_dist = Some(wp_tokens.next().expect("w_searchDist").parse::<u8>()?)
                }
                "w_name" => {
                    name = parse_header_delimited(wp_tokens);
                }
                "w_preferredTargetId" => {
                    let v = str::parse::<u32>(wp_tokens.next().expect("w_preferredTargetId v"))?;
                    preferred_target_id = Some(v);
                }
                "w_preferredTargetId2" => {
                    let v = maybe_hex::<u32>(wp_tokens.next().expect("w_preferredTargetId2 $v"))?;
                    preferred_target_id = Some(v);
                }
                v => {
                    bail!("unknown waypoint key: {}", v);
                }
            }
        }

        Ok(Waypoint {
            index,
            flags: flags.ok_or_else(|| anyhow!("mm:waypoint: flags not set in waypoint",))?,
            goal: goal.ok_or_else(|| anyhow!("mm:waypoint: goal not set in waypoint",))?,
            next: next.ok_or_else(|| anyhow!("mm:waypoint: next not set in waypoint",))?,
            pos: pos.ok_or_else(|| anyhow!("mm:waypoint: pos not set in waypoint",))?,
            speed: speed.ok_or_else(|| anyhow!("mm:waypoint: speed not set in waypoint",))?,
            wng_formation: wng_formation
                .ok_or_else(|| anyhow!("mm:waypoint: wng not set in waypoint",))?,
            react: react.ok_or_else(|| anyhow!("mm:waypoint: react not set in waypoint",))?,
            search_dist: search_dist
                .ok_or_else(|| anyhow!("mm:waypoint: searchDist not set in waypoint",))?,
            name,
            preferred_target_id,
        })
    }
}
