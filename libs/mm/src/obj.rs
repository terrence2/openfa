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
use crate::{util::maybe_hex, waypoint::Waypoint};
use catalog::Catalog;
use failure::{bail, err_msg, Fallible};
use nalgebra::{Point3, Unit, UnitQuaternion, Vector3};
use std::f32::consts::PI;
use xt::{TypeManager, TypeRef};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Nationality {
    Unk0 = 0,
    Unk1 = 1,
    Unk3 = 3,
    Unk4 = 4,
    Unk5 = 5,
    Unk7 = 7,
    Unk8 = 8,
    Unk10 = 10,
    Unk11 = 11,
    Unk12 = 12,
    Unk13 = 13,
    Unk15 = 15,
    Unk16 = 16,
    Unk17 = 17,
    Unk21 = 21,
    Unk22 = 22,
    Unk25 = 25,
    Unk26 = 26,
    Unk27 = 27,
    Unk28 = 28,
    Unk36 = 36,
    Unk39 = 39,
    Unk40 = 40,
    Unk130 = 130,
    Unk131 = 131,
    Unk137 = 137,
    Unk138 = 138,
    Unk142 = 142,
    Unk147 = 147,
    Unk148 = 148,
    Unk151 = 151,
    Unk152 = 152,
    Unk161 = 161,
    Unk162 = 162,
    Unk165 = 165,
    Unk169 = 169,
    Unk185 = 185,
}

impl Nationality {
    fn from_ordinal(n: usize) -> Fallible<Self> {
        Ok(match n {
            0 => Nationality::Unk0,
            1 => Nationality::Unk1,
            3 => Nationality::Unk3,
            4 => Nationality::Unk4,
            5 => Nationality::Unk5,
            7 => Nationality::Unk7,
            8 => Nationality::Unk8,
            10 => Nationality::Unk10,
            11 => Nationality::Unk11,
            12 => Nationality::Unk12,
            13 => Nationality::Unk13,
            15 => Nationality::Unk15,
            16 => Nationality::Unk16,
            17 => Nationality::Unk17,
            21 => Nationality::Unk21,
            22 => Nationality::Unk22,
            25 => Nationality::Unk25,
            26 => Nationality::Unk26,
            27 => Nationality::Unk27,
            28 => Nationality::Unk28,
            36 => Nationality::Unk36,
            39 => Nationality::Unk39,
            40 => Nationality::Unk40,
            130 => Nationality::Unk130,
            131 => Nationality::Unk131,
            137 => Nationality::Unk137,
            138 => Nationality::Unk138,
            142 => Nationality::Unk142,
            147 => Nationality::Unk147,
            148 => Nationality::Unk148,
            151 => Nationality::Unk151,
            152 => Nationality::Unk152,
            161 => Nationality::Unk161,
            162 => Nationality::Unk162,
            165 => Nationality::Unk165,
            169 => Nationality::Unk169,
            185 => Nationality::Unk185,
            _ => bail!("nationality: do not know {}", n),
        })
    }
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct ObjectInfo {
    xt: TypeRef,
    name: Option<String>,
    pos: Point3<f32>,
    angle: UnitQuaternion<f32>,
    nationality: Nationality,
    flags: u16,
    speed: f32,
    alias: i32,
    // NT only.
    skill: Option<u8>,
    react: Option<(u16, u16, u16)>,
    search_dist: Option<u32>,
    // PT only.
    waypoints: Option<Vec<Waypoint>>,
}

impl ObjectInfo {
    pub(crate) fn from_lines(
        lines: &[&str],
        offset: &mut usize,
        type_manager: &TypeManager,
        catalog: &Catalog,
    ) -> Fallible<Self> {
        let mut type_name = None;
        let mut name = None;
        let mut pos = None;
        let mut angle = UnitQuaternion::identity();
        let mut nationality = None;
        let mut flags = 0u16;
        let mut speed = 0f32;
        let mut alias = 0i32;
        // NT only.
        let mut skill = None;
        let mut react = None;
        let mut search_dist = None;

        while lines[*offset].trim() != "." {
            let parts = lines[*offset].trim().splitn(2, ' ').collect::<Vec<&str>>();
            match parts[0].trim_start() {
                "type" => {
                    type_name = Some(parts[1].trim().to_owned());
                }
                "name" => {
                    // Start of Header in ascii? :shrug:
                    assert!(parts[1].starts_with(1 as char));
                    assert!(parts[1].ends_with(1 as char));
                    let end = parts[1].len() - 1;
                    name = Some(parts[1][1..end].to_owned());
                }
                "pos" => {
                    let ns = parts[1].split(' ').collect::<Vec<&str>>();
                    pos = Some(Point3::new(
                        ns[0].parse::<i32>()? as f32,
                        ns[1].parse::<i32>()? as f32,
                        ns[2].parse::<i32>()? as f32,
                    ));
                    // All non-plane entities are at height 0 and need to be moved
                    // to the right elevation at startup.
                    if !type_name.as_ref().expect("type name").ends_with(".PT") {
                        assert_eq!(ns[1].parse::<i32>()?, 0);
                    }
                }
                "angle" => {
                    let ns = parts[1].split(' ').collect::<Vec<&str>>();
                    let is = [
                        ns[0].parse::<i32>()?,
                        ns[1].parse::<i32>()?,
                        ns[2].parse::<i32>()?,
                    ];
                    // No entities are tilted or pitched, only rotated.
                    assert_eq!(is[1], 0);
                    assert_eq!(is[2], 0);
                    angle = UnitQuaternion::from_axis_angle(
                        &Unit::new_unchecked(Vector3::new(0f32, 1f32, 0f32)),
                        -(is[0] as f32 * PI / 180f32),
                    );
                }
                "nationality" => {
                    nationality = Some(Nationality::from_ordinal(parts[1].parse::<usize>()?)?)
                }
                "nationality2" => {
                    nationality = Some(Nationality::from_ordinal(parts[1].parse::<usize>()?)?)
                }
                "nationality3" => {
                    nationality = Some(Nationality::from_ordinal(parts[1].parse::<usize>()?)?)
                }
                "flags" => flags = maybe_hex::<u16>(parts[1])?,
                "speed" => speed = parts[1].parse::<i32>()? as f32,
                "alias" => alias = parts[1].parse::<i32>()?,
                "skill" => skill = Some(parts[1].parse::<u8>()?),
                "react" => {
                    let subparts = parts[1].split(' ').collect::<Vec<&str>>();
                    assert!(type_name.is_some());
                    react = Some((
                        maybe_hex::<u16>(subparts[0])?,
                        maybe_hex::<u16>(subparts[1])?,
                        maybe_hex::<u16>(subparts[2])?,
                    ));
                }
                "searchDist" => search_dist = Some(parts[1].parse::<u32>()?),
                _ => {
                    bail!("unknown obj key: {}", parts[0]);
                }
            }
            *offset += 1;
        }
        Ok(ObjectInfo {
            xt: type_manager.load(
                &type_name
                    .ok_or_else(|| {
                        err_msg(format!("mm:obj: type not set in obj ending {}", *offset))
                    })?
                    .to_uppercase(),
                catalog,
            )?,
            name,
            pos: pos
                .ok_or_else(|| err_msg(format!("mm:obj: pos not set in obj ending {}", *offset)))?,
            angle,
            nationality: nationality.ok_or_else(|| {
                err_msg(format!(
                    "mm:obj: nationality not set in obj ending {}",
                    *offset
                ))
            })?,
            flags,
            speed,
            alias,
            skill,
            react,
            search_dist,
            waypoints: None,
        })
    }

    pub fn set_waypoints(&mut self, waypoints: Vec<Waypoint>) {
        self.waypoints = Some(waypoints);
    }

    pub fn alias(&self) -> i32 {
        self.alias
    }

    pub fn name(&self) -> Option<String> {
        self.name.clone()
    }

    pub fn xt(&self) -> TypeRef {
        self.xt.clone()
    }

    pub fn position(&self) -> Point3<f32> {
        self.pos
    }

    pub fn angle(&self) -> &UnitQuaternion<f32> {
        &self.angle
    }
}
