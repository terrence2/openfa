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
use crate::util::maybe_hex;
use failure::{bail, err_msg, Fallible};
use nalgebra::Point3;

#[allow(dead_code)]
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
    pub(crate) fn from_lines(lines: &[&str], offset: &mut usize) -> Fallible<Self> {
        let mut pos = None;
        let mut name = None;
        let mut color = None;
        let mut icon = None;
        let mut flags = None;

        while lines[*offset].trim() != "." {
            let parts = lines[*offset].trim().splitn(2, ' ').collect::<Vec<&str>>();
            match parts[0].trim_start() {
                "pos" => {
                    let ns = parts[1].split(' ').collect::<Vec<&str>>();
                    pos = Some(Point3::new(
                        ns[0].parse::<i32>()? as f32,
                        ns[1].parse::<i32>()? as f32,
                        ns[2].parse::<i32>()? as f32,
                    ));
                }
                "name" => name = Some(parts[1].to_owned()),
                "color" => color = Some(parts[1].parse::<u8>()?),
                "icon" => icon = Some(parts[1].parse::<i32>()?),
                "flags" => flags = Some(maybe_hex::<u16>(parts[1])?),
                _ => {
                    bail!("unknown special key: {}", parts[0]);
                }
            }
            *offset += 1;
        }
        Ok(SpecialInfo {
            pos: pos.ok_or_else(|| {
                err_msg(format!(
                    "mm:special: pos not set in special ending {}",
                    *offset
                ))
            })?,
            name: name.ok_or_else(|| {
                err_msg(format!(
                    "mm:special: name not set in special ending {}",
                    *offset
                ))
            })?,
            color: color.ok_or_else(|| {
                err_msg(format!(
                    "mm:special: color not set in special ending {}",
                    *offset
                ))
            })?,
            icon: icon.ok_or_else(|| {
                err_msg(format!(
                    "mm:special: icon not set in special ending {}",
                    *offset
                ))
            })?,
            flags: flags.ok_or_else(|| {
                err_msg(format!(
                    "mm:special: flags not set in special ending {}",
                    *offset
                ))
            })?,
        })
    }
}
