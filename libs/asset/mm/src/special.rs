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
use crate::util::{maybe_hex, parse_header_delimited};
use anyhow::{anyhow, bail, Result};
use nalgebra::Point3;
use std::str::SplitAsciiWhitespace;

#[derive(Debug)]
pub struct SpecialInfo {
    pos: Point3<f32>,
    name: String,
    color: u8,
    icon: i32,
    flags: u16,
}

impl SpecialInfo {
    // special
    //         pos 1347582 0 315393
    //         name ^ASea of Japan^A
    //         color 48
    //         icon -1
    //         flags $0
    //         .
    pub(crate) fn from_tokens(tokens: &mut SplitAsciiWhitespace) -> Result<Self> {
        let mut pos = None;
        let mut name = None;
        let mut color = None;
        let mut icon = None;
        let mut flags = None;

        while let Some(token) = tokens.next() {
            match token {
                "pos" => {
                    let x = tokens.next().expect("pos x").parse::<i32>()? as f32;
                    let y = tokens.next().expect("pos y").parse::<i32>()? as f32;
                    let z = tokens.next().expect("pos z").parse::<i32>()? as f32;
                    pos = Some(Point3::new(x, y, z));
                }
                "name" => {
                    name = parse_header_delimited(tokens);
                }
                "color" => color = Some(tokens.next().expect("color").parse::<u8>()?),
                "icon" => icon = Some(tokens.next().expect("icon").parse::<i32>()?),
                "flags" => flags = Some(maybe_hex::<u16>(tokens.next().expect("flags"))?),
                "." => break,
                v => bail!("unknown special key: {}", v),
            }
        }
        Ok(SpecialInfo {
            pos: pos.ok_or_else(|| anyhow!("mm:special: pos not set in special",))?,
            name: name.ok_or_else(|| anyhow!("mm:special: name not set in special",))?,
            color: color.ok_or_else(|| anyhow!("mm:special: color not set in special",))?,
            icon: icon.ok_or_else(|| anyhow!("mm:special: icon not set in special",))?,
            flags: flags.ok_or_else(|| anyhow!("mm:special: flags not set in special",))?,
        })
    }
}
