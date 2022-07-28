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
    canonicalize,
    formation::WingFormation,
    util::{maybe_hex, parse_header_delimited},
    waypoint::Waypoints,
};
use absolute_unit::{degrees, meters, pounds_mass, Angle, Degrees, Mass, PoundsMass};
use anyhow::{anyhow, bail, ensure, Result};
use catalog::Catalog;
use geodesy::{Graticule, Target};
use nalgebra::Point3;
use std::{collections::HashMap, str::SplitAsciiWhitespace};
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
    Unk18 = 18,
    Unk19 = 19,
    Unk21 = 21,
    Unk22 = 22,
    Unk25 = 25,
    Unk26 = 26,
    Unk27 = 27,
    Unk28 = 28,
    Unk36 = 36,
    Unk39 = 39,
    Unk40 = 40,
    Unk128 = 128,
    Unk130 = 130,
    Unk131 = 131,
    Unk132 = 132,
    Unk133 = 133,
    Unk136 = 136,
    Unk137 = 137,
    Unk138 = 138,
    Unk140 = 140,
    Unk142 = 142,
    Unk143 = 143,
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
    fn from_ordinal(n: usize) -> Result<Self> {
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
            18 => Nationality::Unk18,
            19 => Nationality::Unk19,
            21 => Nationality::Unk21,
            22 => Nationality::Unk22,
            25 => Nationality::Unk25,
            26 => Nationality::Unk26,
            27 => Nationality::Unk27,
            28 => Nationality::Unk28,
            36 => Nationality::Unk36,
            39 => Nationality::Unk39,
            40 => Nationality::Unk40,
            128 => Nationality::Unk128,
            130 => Nationality::Unk130,
            131 => Nationality::Unk131,
            132 => Nationality::Unk132,
            133 => Nationality::Unk133,
            136 => Nationality::Unk136,
            137 => Nationality::Unk137,
            138 => Nationality::Unk138,
            142 => Nationality::Unk142,
            140 => Nationality::Unk140,
            143 => Nationality::Unk143,
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

#[derive(Debug, Default)]
pub struct EulerAngles {
    yaw: Angle<Degrees>,
    pitch: Angle<Degrees>,
    #[allow(unused)]
    roll: Angle<Degrees>,
}

impl EulerAngles {
    pub fn facing(&self) -> Graticule<Target> {
        Graticule::new(degrees!(self.pitch), degrees!(self.yaw), meters!(1))
    }

    pub fn yaw(&self) -> Angle<Degrees> {
        self.yaw
    }

    pub fn pitch(&self) -> Angle<Degrees> {
        self.pitch
    }

    pub fn roll(&self) -> Angle<Degrees> {
        self.roll
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct ObjectInfo {
    xt: TypeRef,
    name: Option<String>,
    pos: Point3<i32>,
    angle: EulerAngles,
    nationality: Nationality,
    flags: u16,
    speed: f32,
    alias: Option<i32>,
    // NT only.
    skill: Option<u8>,
    react: Option<(u16, u16, u16)>,
    search_dist: Option<u32>,
    group: Option<(u8, u8)>, // like wing but for ground convoys; baltics only
    // PT only.
    waypoints: Option<Waypoints>,
    wing: Option<(u8, u8)>,
    wng_formation: Option<WingFormation>,
    start_time: u32,
    controller: u8,
    preferred_target_id: Option<u32>,
    npc_flags: Option<u8>,
    hardpoint_overrides: Option<HashMap<usize, (u8, Option<TypeRef>)>>,
    fuel_override: Option<Mass<PoundsMass>>, // Only in VIET03.M
}

impl ObjectInfo {
    pub fn from_xt(xt: TypeRef, name: &str) -> Self {
        Self {
            xt,
            name: Some(name.to_owned()),
            pos: Point3::new(1, 1, 1), // avoid origin for special discover of e.g. gear
            angle: EulerAngles::default(),
            nationality: Nationality::Unk0,
            flags: 0,
            speed: 200.,
            alias: None,
            // NT only.
            skill: None,
            react: None,
            search_dist: None,
            group: None,
            // PT only.
            waypoints: None,
            wing: None,
            wng_formation: None,
            start_time: 0,
            controller: 0,
            preferred_target_id: None,
            npc_flags: None,
            hardpoint_overrides: None,
            fuel_override: None,
        }
    }

    pub(crate) fn from_tokens(
        tokens: &mut SplitAsciiWhitespace,
        type_manager: &TypeManager,
        catalog: &Catalog,
    ) -> Result<Self> {
        let mut xt = None;
        let mut name = None;
        let mut pos = None;
        let mut angle = EulerAngles::default();
        let mut nationality = None;
        let mut flags = 0u16;
        let mut speed = 0f32;
        let mut alias = None;
        // NT only.
        let mut skill = None;
        let mut react = None;
        let mut search_dist = None;
        let mut group = None;
        // PT only.
        let mut wing = None;
        let mut wng_formation = None;
        let mut start_time = 0;
        let mut controller = 0;
        let mut preferred_target_id = None;
        let mut npc_flags = None;
        let mut hardpoint_overrides = None;
        let mut fuel_override = None;

        while let Some(token) = tokens.next() {
            match token {
                "type" => {
                    // TODO: pass raw in so we can dedup on the &str.
                    let name = tokens.next().expect("type").to_uppercase();
                    if let Ok(ty) = type_manager.load(&name, catalog) {
                        xt = Some(ty);
                    } else {
                        let name = canonicalize(&name);
                        xt = Some(type_manager.load(&name, catalog)?);
                    }
                }
                "name" => {
                    name = parse_header_delimited(tokens);
                }
                "pos" => {
                    let x = tokens.next().expect("pos x").parse::<i32>()?;
                    let y = tokens.next().expect("pos y").parse::<i32>()?;
                    let z = tokens.next().expect("pos z").parse::<i32>()?;
                    pos = Some(Point3::new(x, y, z));
                    // All non-plane entities are at height 0 and need to be moved
                    // to the right elevation at startup.
                    if !xt.as_ref().expect("xt").is_pt()
                        && xt.as_ref().expect("xt").ot().file_name() != "MICROM.OT"
                    {
                        // In UKR17, there is a fight over a radio antenna on a roof.
                        // Otherwise all objects are on the ground.
                        assert_eq!(y, 0);
                    }
                }
                "angle" => {
                    let yaw = tokens.next().expect("ang yaw").parse::<i32>()?;
                    let pitch = tokens.next().expect("ang pitch").parse::<i32>()?;
                    let roll = tokens.next().expect("ang roll").parse::<i32>()?;
                    angle = EulerAngles {
                        yaw: degrees!(yaw),
                        pitch: degrees!(pitch),
                        roll: degrees!(roll),
                    };
                }
                "nationality" => {
                    nationality = Some(Nationality::from_ordinal(
                        tokens.next().expect("nationality").parse::<usize>()?,
                    )?)
                }
                "nationality2" => {
                    nationality = Some(Nationality::from_ordinal(maybe_hex(
                        tokens.next().expect("nationality2"),
                    )?)?)
                }
                "nationality3" => {
                    nationality = Some(Nationality::from_ordinal(
                        tokens.next().expect("nationality3").parse::<usize>()?,
                    )?)
                }
                "flags" => flags = maybe_hex::<u16>(tokens.next().expect("flags"))?,
                "speed" => speed = tokens.next().expect("speed").parse::<i32>()? as f32,
                "alias" => alias = Some(tokens.next().expect("alias").parse::<i32>()?),
                "skill" => skill = Some(tokens.next().expect("skill").parse::<u8>()?),
                "react" => {
                    react = Some((
                        maybe_hex::<u16>(tokens.next().expect("react[0]"))?,
                        maybe_hex::<u16>(tokens.next().expect("react[1]"))?,
                        maybe_hex::<u16>(tokens.next().expect("react[2]"))?,
                    ));
                }
                "searchDist" => {
                    search_dist = Some(tokens.next().expect("search dist").parse::<u32>()?)
                }
                "group" => {
                    let squad = str::parse::<u8>(tokens.next().expect("group squad"))?;
                    let offset = str::parse::<u8>(tokens.next().expect("group offset"))?;
                    group = Some((squad, offset));
                }
                "wing" => {
                    let squad = str::parse::<u8>(tokens.next().expect("wing squad"))?;
                    let offset = str::parse::<u8>(tokens.next().expect("wing offset"))?;
                    wing = Some((squad, offset));
                }
                "wng" => {
                    wng_formation = Some(WingFormation::from_tokens(tokens)?);
                }
                "startTime" => {
                    let t = str::parse::<u32>(tokens.next().expect("startTime t"))?;
                    start_time = t; // unknown units
                }
                "controller" => {
                    let v = maybe_hex::<u8>(tokens.next().expect("controller v"))?;
                    ensure!(v == 128);
                    controller = v;
                }
                "preferredTargetId" => {
                    let v = str::parse::<u32>(tokens.next().expect("preferredTargetId v"))?;
                    preferred_target_id = Some(v);
                }
                "preferredTargetId2" => {
                    let v = maybe_hex::<u32>(tokens.next().expect("preferredTargetId2 $v"))?;
                    preferred_target_id = Some(v);
                }
                "npcFlags" => {
                    let flags = str::parse::<u8>(tokens.next().expect("npcFlags v"))?;
                    ensure!(flags == 2);
                    npc_flags = Some(flags);
                }
                "hardpoint" => {
                    if hardpoint_overrides.is_none() {
                        hardpoint_overrides = Some(HashMap::new());
                    }
                    let idx = str::parse::<usize>(tokens.next().expect("hardpoint a"))?;
                    let cnt = str::parse::<u8>(tokens.next().expect("hardpoint b"))?;
                    let hp_xt = if cnt > 0 {
                        let ty_name = tokens.next().expect("hardpoint c").to_uppercase();
                        Some(type_manager.load(&ty_name, catalog)?)
                    } else {
                        None
                    };
                    hardpoint_overrides
                        .as_mut()
                        .map(|hps| hps.insert(idx, (cnt, hp_xt)));
                }
                "fuel" => {
                    let unk = str::parse::<u8>(tokens.next().expect("fuel unk"))?;
                    fuel_override = Some(pounds_mass!(unk));
                }
                "." => break,
                _ => {
                    bail!("unknown obj key: {}", token);
                }
            }
        }

        Ok(ObjectInfo {
            xt: xt.ok_or_else(|| anyhow!("mm:obj: type not set in obj"))?,
            name,
            pos: pos.ok_or_else(|| anyhow!("mm:obj: pos not set in obj"))?,
            angle,
            nationality: nationality
                .ok_or_else(|| anyhow!("mm:obj: nationality not set in obj",))?,
            flags,
            speed,
            alias,
            skill,
            react,
            search_dist,
            group,
            wing,
            wng_formation,
            start_time,
            controller,
            preferred_target_id,
            npc_flags,
            hardpoint_overrides,
            fuel_override,
            waypoints: None,
        })
    }

    pub fn set_waypoints(&mut self, waypoints: Waypoints) {
        self.waypoints = Some(waypoints);
    }

    pub fn alias(&self) -> Option<i32> {
        self.alias
    }

    pub fn name(&self) -> Option<String> {
        self.name.clone()
    }

    pub fn xt(&self) -> TypeRef {
        self.xt.clone()
    }

    pub fn position(&self) -> &Point3<i32> {
        &self.pos
    }

    pub fn angle(&self) -> &EulerAngles {
        &self.angle
    }

    pub fn speed(&self) -> f32 {
        self.speed
    }

    pub fn fuel_override(&self) -> Option<Mass<PoundsMass>> {
        self.fuel_override
    }
}
