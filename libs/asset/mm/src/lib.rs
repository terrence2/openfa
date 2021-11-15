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
#![allow(clippy::cognitive_complexity)]

mod formation;
mod obj;
mod special;
mod util;
mod waypoint;

use crate::{obj::ObjectInfo, special::SpecialInfo, waypoint::Waypoint};
use anyhow::{anyhow, bail, ensure, Result};
use bitflags::bitflags;
use catalog::Catalog;
use log::debug;
use std::{borrow::Cow, collections::HashMap, str::FromStr};
use xt::TypeManager;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum TLoc {
    Index(usize),
    Name(String),
}

impl TLoc {
    pub fn pic_file(&self, base: &str) -> Cow<str> {
        match self {
            TLoc::Index(ref i) => Cow::from(format!("{}{}.PIC", base, i)),
            TLoc::Name(ref s) => Cow::from(s),
        }
    }

    pub fn is_named(&self) -> bool {
        match self {
            Self::Index(_) => false,
            Self::Name(_) => true,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MapOrientation {
    Unk0,
    Unk1,
    FlipS,
    RotateCcw,
}

impl MapOrientation {
    pub fn from_byte(n: u8) -> Result<Self> {
        Ok(match n {
            0 => MapOrientation::Unk0,
            1 => MapOrientation::Unk1,
            2 => MapOrientation::FlipS,
            3 => MapOrientation::RotateCcw,
            _ => bail!("invalid orientation"),
        })
    }

    pub fn as_byte(&self) -> u8 {
        match self {
            MapOrientation::Unk0 => 0,
            MapOrientation::Unk1 => 1,
            MapOrientation::FlipS => 2,
            MapOrientation::RotateCcw => 3,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TMap {
    pub orientation: MapOrientation,
    pub loc: TLoc,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TDic {
    n: usize,
    map: [[u8; 4]; 8],
}

#[derive(Debug, Eq, PartialEq)]
pub struct MapName {
    raw: String,
    prefix: Option<char>,
    name: String,
    number: Option<u32>,
    ext: String,
}

impl MapName {
    // These are all of the terrains and map references in the base games.
    // FA:
    //     FA_2.LIB:
    //         EGY.T2, FRA.T2, VLA.T2, BAL.T2, UKR.T2, KURILE.T2, TVIET.T2
    //         APA.T2, CUB.T2, GRE.T2, IRA.T2, LFA.T2, NSK.T2, PGU.T2, SPA.T2, WTA.T2
    //     MM refs:
    //         // Campaign missions?
    //         $bal[0-7].T2
    //         $egy[1-9].T2
    //         $fra[0-9].T2
    //         $vla[1-8].T2
    //         ~ukr[1-8].T2
    //         // Freeform missions and ???; map editor layouts maybe?
    //         ~apaf.T2, apa.T2
    //         ~balf.T2, bal.T2
    //         ~cubf.T2, cub.T2
    //         ~egyf.T2, egy.T2
    //         ~fraf.T2, fra.T2
    //         ~gref.T2, gre.T2
    //         ~iraf.T2, ira.T2
    //         ~kurile.T2, kurile.T2
    //         ~lfaf.T2, lfa.T2
    //         ~nskf.T2, nsk.T2
    //         ~pguf.T2, pgu.T2
    //         ~spaf.T2, spa.T2
    //         ~tviet.T2, tviet.T2
    //         ~ukrf.T2, ukr.T2
    //         ~vlaf.T2, vla.T2
    //         ~wtaf.T2, wta.T2
    //    M refs:
    //         $bal[0-7].T2
    //         $egy[1-8].T2
    //         $fra[0-3,6-9].T2
    //         $vla[1-8].T2
    //         ~bal[0,2,3,6,7].T2
    //         ~egy[1,2,4,7].T2
    //         ~fra[3,9].T2
    //         ~ukr[1-8].T2
    //         ~vla[1,2,5].T2
    //         bal.T2, cub.T2, egy.T2, fra.T2, kurile.T2, tviet.T2, ukr.T2, vla.T2
    // USNF97:
    //     USNF_2.LIB: UKR.T2, ~UKR[1-8].T2, KURILE.T2, VIET.T2
    //     MM refs: ukr.T2, ~ukr[1-8].T2, kurile.T2, viet.T2
    //     M  refs: ukr.T2, ~ukr[1-8].T2, kurile.T2, viet.T2
    // ATFGOLD:
    //     ATF_2.LIB: EGY.T2, FRA.T2, VLA.T2, BAL.T2
    //     MM refs: egy.T2, fra.T2, vla.T2, bal.T2
    //              $egy[1-9].T2, $fra[0-9].T2, $vla[1-8].T2, $bal[0-7].T2
    //     INVALID: kurile.T2, ~ukr[1-8].T2, ukr.T2, viet.T2
    //     M  refs: $egy[1-8].T2, $fra[0-3,6-9].T2, $vla[1-8].T2, $bal[0-7].T2,
    //              ~bal[2,6].T2, bal.T2, ~egy4.T2, egy.T2, fra.T2, vla.T2
    //     INVALID: ukr.T2
    // ATFNATO:
    //     installdir: EGY.T2, FRA.T2, VLA.T2, BAL.T2
    //     MM refs: egy.T2, fra.T2, vla.T2, bal.T2,
    //              $egy[1-9].T2, $fra[0-9].T2, $vla[1-8].T2, $bal[0-7].T2
    //     M  refs: egy.T2, fra.T2, vla.T2, bal.T2,
    //              $egy[1-8].T2, $fra[0-3,6-9].T2, $vla[1-8].T2, $bal[0-7].T2
    // ATF:
    //     installdir: EGY.T2, FRA.T2, VLA.T2
    //     MM refs: egy.T2, fra.T2, vla.T2,
    //              $egy[1-8].T2, $fra[0-9].T2, $vla[1-8].T2
    //     M  refs: $egy[1-8].T2, $fra[0-3,6-9].T2, $vla[1-8].T2, egy.T2
    // MF:
    //     installdir: UKR.T2, $UKR[1-8].T2, KURILE.T2
    //     MM+M refs: ukr.T2, $ukr[1-8].T2, kurile.T2
    // USNF:
    //     installdir: UKR.T2, $UKR[1-8].T2
    //     MM+M refs: ukr.T2, $ukr[1-8].T2
    fn parse(map_name: &str) -> Result<Self> {
        let raw = map_name.to_uppercase();

        let (name, ext) = raw
            .rsplit_once('.')
            .ok_or_else(|| anyhow!("invalid map_name; must be a t2 metafile"))?;

        let mut name = name.to_owned();
        ensure!(!name.is_empty());
        let maybe_prefix = name.chars().next().unwrap();
        let maybe_number = name.chars().last().unwrap();

        let mut prefix = None;
        if ['~', '$'].contains(&maybe_prefix) {
            prefix = Some(maybe_prefix);
            name = name[1..].to_owned();
        }

        let mut number = None;
        if let Some(digit) = maybe_number.to_digit(10) {
            number = Some(digit);
            name = name[..name.len() - 1].to_owned();
        }

        Ok(Self {
            ext: ext.to_owned(),
            raw,
            prefix,
            name,
            number,
        })
    }

    pub fn meta_name(&self) -> &str {
        &self.raw
    }

    pub fn base_name(&self) -> &str {
        &self.name
    }

    pub fn base_texture_name(&self) -> &str {
        &self.name[..3]
    }

    pub fn t2_name(&self) -> String {
        format!("{}.T2", self.name)
    }

    /// The first character of the name section of the metaname. The LAY data may have a
    /// map-specific version of a layer that should be preferred over the base LAY. This
    /// verison has this token appended to the non-extention part of the name.
    pub fn layer_token(&self) -> char {
        self.name.chars().next().expect("non-empty map name")
    }

    pub fn parent(&self) -> String {
        format!("{}.MM", self.name)
    }
}

bitflags! {
    struct ScreenSet : u8 {
        const Briefing = 0x01;
        const BriefingMap = 0x02;
        const SelectPlane = 0x04;
        const ArmPlane = 0x08;
    }
}

#[derive(Debug)]
enum MValue {
    TextFormat,
    Brief,
    BriefMap,
    SelectPlane,
    ArmPlane,
    AllowRearmRefuel(bool),
    MapName(MapName),
    Layer((String, usize)),
    Clouds(u32),
    Wind((i16, i16)),
    View((u32, u32, u32)),
    Time((u8, u8)),
    UsAirSkill(u8),
    UsGroundSkill(u8),
    ThemAirSkill(u8),
    ThemGroundSkill(u8),
    Sides(Vec<u8>),
    HistoricalEra(u8),
    TMaps(HashMap<(u32, u32), TMap>),
    TDics(Vec<TDic>),
    Objects(Vec<ObjectInfo>),
}

impl MValue {
    fn from_str(s: &str, type_manager: &TypeManager, catalog: &Catalog) -> Result<Vec<MValue>> {
        let mut mm = Vec::new();

        // Do a fast pre-pass to get array pre-sizing for allocations and check if we need a
        // lexical pass to remove comments.
        let mut obj_cnt = 0;
        let mut special_cnt = 0;
        let mut tmap_cnt = 0;
        let mut tdic_cnt = 0;
        let mut need_lexical_pass = false;
        let init_tokens = s.split_ascii_whitespace();
        let prepass_tokens = init_tokens.clone();
        for token in prepass_tokens {
            match token {
                "obj" => obj_cnt += 1,
                "special" => special_cnt += 1,
                "tmap" => tmap_cnt += 1,
                "tmap_named" => tmap_cnt += 1,
                "tdic" => tdic_cnt += 1,
                v => {
                    if v.starts_with(';') {
                        need_lexical_pass = true;
                    }
                }
            }
        }
        let owned;
        let mut tokens = if need_lexical_pass {
            owned = s
                .lines()
                .filter(|l| !l.starts_with(';'))
                .collect::<Vec<_>>()
                .join("\n");
            owned.split_ascii_whitespace()
        } else {
            init_tokens
        };

        let mut layer_token = None;
        let mut sides: Vec<u8> = Vec::with_capacity(64);
        let mut objects = Vec::with_capacity(obj_cnt);
        let mut specials: Vec<SpecialInfo> = Vec::with_capacity(special_cnt);
        let mut tmaps = HashMap::with_capacity(tmap_cnt);
        let mut tdics = Vec::with_capacity(tdic_cnt);

        while let Some(token) = tokens.next() {
            assert!(!token.starts_with(';'));
            println!("TOKEN: {}", token);
            match token {
                "allowrearmrefuel" => {
                    let v = str::parse::<u8>(tokens.next().expect("allow rearm value"))?;
                    ensure!(v == 0);
                    mm.push(MValue::AllowRearmRefuel(false));
                }
                "textFormat" => mm.push(MValue::TextFormat),
                "brief" => mm.push(MValue::Brief),
                "briefmap" => mm.push(MValue::BriefMap),
                "selectplane" => mm.push(MValue::SelectPlane),
                "armplane" => mm.push(MValue::ArmPlane),
                "map" => {
                    let raw_map_name = tokens.next().ok_or_else(|| anyhow!("map name expected"))?;
                    let map_name = MapName::parse(raw_map_name)?;
                    layer_token = Some(map_name.layer_token().to_owned());
                    mm.push(MValue::MapName(map_name));
                }
                "layer" => {
                    let raw_layer_name = tokens.next().expect("layer name");
                    let layer_index = tokens.next().expect("layer index").parse::<usize>()?;
                    let layer_name = Self::find_layer(
                        layer_token.expect("map name must come before layer"),
                        &raw_layer_name,
                        catalog,
                    )?;
                    mm.push(MValue::Layer((layer_name, layer_index)));
                }
                "clouds" => {
                    mm.push(MValue::Clouds(
                        tokens.next().expect("clouds").parse::<u32>()?,
                    ));
                }
                "wind" => {
                    let x = str::parse::<i16>(tokens.next().expect("wind x"))?;
                    let z = str::parse::<i16>(tokens.next().expect("wind z"))?;
                    mm.push(MValue::Wind((x, z)));
                }
                "view" => {
                    let x = str::parse::<u32>(tokens.next().expect("view x"))?;
                    let y = str::parse::<u32>(tokens.next().expect("view y"))?;
                    let z = str::parse::<u32>(tokens.next().expect("view z"))?;
                    mm.push(MValue::View((x, y, z)));
                }
                "time" => {
                    let h = str::parse::<u8>(tokens.next().expect("time h"))?;
                    let m = str::parse::<u8>(tokens.next().expect("time m"))?;
                    mm.push(MValue::Time((h, m)));
                }
                "usGroundSkill" => {
                    let skill = str::parse::<u8>(tokens.next().expect("skill"))?;
                    mm.push(MValue::UsGroundSkill(skill));
                }
                "usAirSkill" => {
                    let skill = str::parse::<u8>(tokens.next().expect("skill"))?;
                    mm.push(MValue::UsAirSkill(skill));
                }
                "themGroundSkill" => {
                    let skill = str::parse::<u8>(tokens.next().expect("skill"))?;
                    mm.push(MValue::ThemGroundSkill(skill));
                }
                "themAirSkill" => {
                    let skill = str::parse::<u8>(tokens.next().expect("skill"))?;
                    mm.push(MValue::ThemAirSkill(skill));
                }
                "sides" => {
                    // Only used by Ukraine.
                    assert!(sides.is_empty());
                    for _ in 0..18 {
                        let side = str::parse::<u8>(tokens.next().expect("side"))?;
                        ensure!(side == 0 || side == 128, "mm: unknown side flag");
                        sides.push(side);
                    }
                }
                "sides2" => {
                    // Post USNF: one more nationality, now in hex format, 0 or $80
                    assert!(sides.is_empty());
                    for _ in 0..19 {
                        let side = u8::from_str_radix(&tokens.next().expect("side")[1..], 16)?;
                        ensure!(side == 0 || side == 128, "mm: unknown side flag");
                        sides.push(side);
                    }
                }
                "sides3" => {
                    // Protocol bump for 24 nationalities.
                    assert!(sides.is_empty());
                    for _ in 0..24 {
                        let side = u8::from_str_radix(&tokens.next().expect("side")[1..], 16)?;
                        ensure!(side == 0 || side == 128, "mm: unknown side flag");
                        sides.push(side);
                    }
                }
                "sides4" => {
                    // Protocol bump for 64 nationalities.
                    assert!(sides.is_empty());
                    for _ in 0..64 {
                        let side = u8::from_str_radix(&tokens.next().expect("side")[1..], 16)?;
                        ensure!(side == 0 || side == 128, "mm: unknown side flag");
                        sides.push(side);
                    }
                }
                "historicalera" => {
                    let historical_era = u8::from_str(tokens.next().expect("historical era"))?;
                    mm.push(MValue::HistoricalEra(historical_era));
                }
                "obj" => {
                    let obj = ObjectInfo::from_tokens(&mut tokens, type_manager, catalog)?;
                    objects.push(obj);
                }
                "special" => {
                    let special = SpecialInfo::from_tokens(&mut tokens)?;
                    specials.push(special);
                }
                "tmap" => {
                    let x = tokens.next().expect("tmap x").parse::<i16>()? as u32;
                    let y = tokens.next().expect("tmap y").parse::<i16>()? as u32;
                    ensure!(x % 4 == 0, "unaligned tmap x index");
                    ensure!(y % 4 == 0, "unaligned tmap y index");
                    let index = tokens.next().expect("index").parse::<usize>()?;
                    let orientation = tokens.next().expect("orientation").parse::<u8>()?;
                    tmaps.insert(
                        (x, y),
                        TMap {
                            orientation: MapOrientation::from_byte(orientation)?,
                            loc: TLoc::Index(index),
                        },
                    );
                }
                "tmap_named" => {
                    // TODO: maybe push to_uppercase lower?
                    let tmp = tokens.next().expect("name");
                    let name = (String::with_capacity(tmp.len() + 4) + tmp).to_uppercase() + ".PIC";
                    let x = tokens.next().expect("tmap_named x").parse::<i16>()? as u32;
                    let y = tokens.next().expect("tmap_named y").parse::<i16>()? as u32;
                    ensure!(x % 4 == 0, "unaligned tmap_named x index");
                    ensure!(y % 4 == 0, "unaligned tmap_named y index");
                    tmaps.insert(
                        (x, y),
                        TMap {
                            orientation: MapOrientation::from_byte(0)?,
                            loc: TLoc::Name(name),
                        },
                    );
                }
                "tdic" => {
                    let n = tokens.next().expect("tdic n").parse::<usize>()?;
                    let mut map = [[0u8; 4]; 8];
                    for row in &mut map {
                        for item in row {
                            let t = tokens.next().expect("map");
                            *item = (t == "1") as u8;
                        }
                    }
                    let tdic = TDic { n, map };
                    tdics.push(tdic);
                }
                "waypoint2" => {
                    let cnt = tokens.next().expect("waypoint cnt").parse::<usize>()?;
                    let mut waypoints = Vec::with_capacity(cnt);
                    for i in 0..cnt {
                        let wp = Waypoint::from_tokens(&mut tokens)?;
                        assert_eq!(wp.index as usize, i);
                        waypoints.push(wp);
                    }
                    let w_for_tok = tokens.next().expect("w_for");
                    ensure!(w_for_tok == "w_for");
                    // FIXME: this is probably an index into objects? Except it's negative?
                    let _w_for = tokens.next().expect("w_for").parse::<i16>()?;
                    let dot_tok = tokens.next().expect("dot");
                    ensure!(dot_tok == ".");
                }
                "\0" | "\x1A" => {
                    // DOS EOF char?
                }
                v => {
                    println!("mm parse error near token: {:?} {:?}", v, tokens.next());
                    bail!("unknown mission map key: {}", v);
                }
            }
        }

        for tmap in tmaps.iter() {
            if let TLoc::Index(i) = tmap.1.loc {
                ensure!(
                    (i as usize) < tdics.len(),
                    "expected at tdict for each tmap index"
                );
            }
        }

        if !specials.is_empty() {
            //mm.push(MValue::Specials(specials));
        }
        if !objects.is_empty() {
            mm.push(MValue::Objects(objects));
        }
        if !tmaps.is_empty() {
            mm.push(MValue::TMaps(tmaps));
        }
        if !tdics.is_empty() {
            mm.push(MValue::TDics(tdics));
        }

        Ok(mm)
    }

    // This is yet a different lookup routine than for T2 or PICs. It is usually the `layer` value,
    // except when it is a modified version with the first (non-tilde) character of the MM name
    // appended to the end of the LAY name, before the dot.
    fn find_layer(layer_token: char, raw_layer_name: &str, catalog: &Catalog) -> Result<String> {
        debug!("find_layer token:{}, layer:{}", layer_token, raw_layer_name);
        let layer_name = raw_layer_name.to_uppercase();
        let (layer_prefix, layer_ext) = layer_name
            .rsplit_once('.')
            .ok_or_else(|| anyhow!("layer must have extension"))?;
        let alt_layer_name = format!("{}{}.{}", layer_prefix, layer_token, layer_ext);
        if catalog.exists(&alt_layer_name) {
            debug!("B: using lay: {}", alt_layer_name);
            return Ok(alt_layer_name);
        }
        debug!("A: using lay: {}", layer_name);
        Ok(layer_name)
    }
}

#[allow(dead_code)]
pub struct MissionMap {
    map_name: MapName,
    layer_name: String,
    layer_index: usize,
    tmaps: HashMap<(u32, u32), TMap>,
    tdics: Vec<TDic>,
    wind: (i16, i16),
    view: (u32, u32, u32),
    time: (u8, u8),
    objects: Vec<ObjectInfo>,
}

impl MissionMap {
    pub fn from_str(s: &str, type_manager: &TypeManager, catalog: &Catalog) -> Result<Self> {
        let mut mm = MValue::from_str(s, type_manager, catalog)?;

        let mut map_name = None;
        let mut layer_name = None;
        let mut layer_index = None;
        let mut wind = Some((0, 0));
        let mut view = None;
        let mut time = None;
        // let mut sides: Vec<u8> = Vec::with_capacity(64);
        // let mut specials: Vec<SpecialInfo> = Vec::with_capacity(special_cnt);
        let mut tmaps = None;
        let mut tdics = None;
        let mut objects = None;

        ensure!(
            matches!(mm[0], MValue::TextFormat),
            "missing textFormat node in MM"
        );
        for key in mm.drain(..) {
            match key {
                MValue::TextFormat => {}
                MValue::MapName(map) => {
                    //ensure!(map_name.parent(name.chars().next().unwrap()) == name);
                    map_name = Some(map);
                    /*
                    assert!(raw_map_name.is_none());
                    raw_map_name = Some(tokens.next().expect("map name").to_owned());
                    let parent_name = raw_map_name
                        .as_ref()
                        .unwrap()
                        .replace('$', &name.chars().next().unwrap().to_string())
                        .replace(".T2", ".MM")
                        .to_uppercase();
                    //if name.ends_with("MM") {
                    ensure!(parent_name == name);
                    //}
                    t2_name = Some(Self::find_t2_for_map(
                        raw_map_name.as_ref().unwrap(),
                        catalog,
                    )?);
                     */
                }
                MValue::Layer((name, index)) => {
                    layer_name = Some(name);
                    layer_index = Some(index);
                }
                MValue::View(v) => view = Some(v),
                MValue::Time(t) => time = Some(t),
                MValue::Clouds(clouds) => ensure!(clouds == 0),
                MValue::HistoricalEra(historical_era) => ensure!(historical_era == 4),
                MValue::TMaps(tm) => tmaps = Some(tm),
                MValue::TDics(td) => tdics = Some(td),
                MValue::Objects(objs) => objects = Some(objs),
                _ => {}
            }
        }

        Ok(MissionMap {
            map_name: map_name.ok_or_else(|| anyhow!("mm must have a 'map' key"))?,
            layer_name: layer_name.ok_or_else(|| anyhow!("mm must have a 'layer' key"))?,
            layer_index: layer_index.ok_or_else(|| anyhow!("mm must have a 'layer' key"))?,
            wind: wind.ok_or_else(|| anyhow!("mm must have a 'wind' key"))?,
            view: view.ok_or_else(|| anyhow!("mm must have a 'view' key"))?,
            time: time.ok_or_else(|| anyhow!("mm must have a 'time' key"))?,
            tmaps: tmaps.ok_or_else(|| anyhow!("mm must have 'tmaps' keys"))?,
            tdics: tdics.ok_or_else(|| anyhow!("mm must have 'tdics' keys"))?,
            objects: objects.ok_or_else(|| anyhow!("mm must have 'object' keys"))?,
        })
    }

    pub fn map_name(&self) -> &MapName {
        &self.map_name
    }

    pub fn layer_name(&self) -> &str {
        &self.layer_name
    }

    pub fn layer_index(&self) -> usize {
        self.layer_index
    }

    pub fn texture_dictionary(&self) -> &[TDic] {
        &self.tdics
    }

    pub fn texture_maps(&self) -> std::collections::hash_map::Values<'_, (u32, u32), TMap> {
        self.tmaps.values()
    }

    pub fn texture_map(&self, xi: u32, zi: u32) -> Option<&TMap> {
        self.tmaps.get(&(xi, zi))
    }

    pub fn objects(&self) -> &Vec<ObjectInfo> {
        &self.objects
    }

    /*
    fn find_t2_for_map(map_name: &str, catalog: &Catalog) -> Result<String> {
        let raw = map_name.to_uppercase();

        if catalog.exists(&raw) {
            debug!("A: using t2: {}", raw);
            return Ok(raw);
        }

        // ~KURILE.T2 && ~TVIET.T2
        if raw.starts_with('~') && catalog.exists(&raw[1..]) {
            debug!("B: using t2: {}", &raw[1..]);
            return Ok(raw[1..].to_owned());
        }

        let parts = raw.split('.').collect::<Vec<&str>>();
        let base = parts[0];
        if base.len() == 5 {
            let sigil = base.chars().next().unwrap();
            ensure!(
                sigil == '~' || sigil == '$',
                "expected non-literal map name to start with $ or ~"
            );
            let suffix = base.chars().rev().take(1).collect::<String>();
            ensure!(
                suffix == "F" || suffix.parse::<u8>().is_ok(),
                "expected non-literal map name to end with f or a number"
            );
            debug!("C: using t2: {}.T2", &base[1..=3]);
            return Ok(base[1..=3].to_owned() + ".T2");
        }

        bail!("no map file matching {} found", raw)
    }
     */

    // This is yet a different lookup routine than for T2 or PICs. It is usually the `layer` value,
    // except when it is a modified version with the first (non-tilde) character of the MM name
    // appended to the end of the LAY name, before the dot.
    fn find_layer(map_name: &str, layer_name: &str, catalog: &Catalog) -> Result<String> {
        debug!("find_layer map:{}, layer:{}", map_name, layer_name);
        let first_char = map_name.chars().next().expect("the first character");
        let layer_parts = layer_name.split('.').collect::<Vec<&str>>();
        ensure!(layer_parts.len() == 2, "expected one dot in layer name");
        ensure!(
            layer_parts[1].to_uppercase() == "LAY",
            "expected LAY extension"
        );
        let alt_layer_name = format!("{}{}.LAY", layer_parts[0], first_char).to_uppercase();
        if catalog.exists(&alt_layer_name) {
            debug!("B: using lay: {}", alt_layer_name);
            return Ok(alt_layer_name);
        }
        debug!("A: using lay: {}", layer_name.to_uppercase());
        Ok(layer_name.to_uppercase())
    }

    // This is a slightly different problem then getting the T2, because even though ~ABCn.T2
    // might exist for ~ABCn.MM, we need to look up ABCi.PIC without the tilda.
    /*
    pub fn get_base_texture_name(&self) -> Result<String> {
        let raw = self.map_name.to_uppercase();
        let mut name = raw
            .split('.')
            .next()
            .ok_or_else(|| anyhow!("expected a dotted name"))?;
        if name.starts_with('~') || name.starts_with('$') {
            name = &name[1..];
        }
        name = &name[0..3];
        let se = name.chars().rev().take(1).collect::<String>();
        if se.parse::<u8>().is_ok() {
            name = &name[..name.len() - 1];
        }

        Ok(name.to_owned())
    }
     */
}

/// Represents an M file.
pub struct Mission {}

impl Mission {
    pub fn from_str(s: &str, type_manager: &TypeManager, catalog: &Catalog) -> Result<Self> {
        let mkeys = MValue::from_str(s, type_manager, catalog)?;
        Ok(Mission {})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lib::{from_dos_string, CatalogManager};

    #[test]
    fn it_can_parse_all_mm_files() -> Result<()> {
        let catalogs = CatalogManager::for_testing()?;
        for (game, catalog) in catalogs.all() {
            for fid in catalog.find_with_extension("MM")? {
                let meta = catalog.stat_sync(fid)?;

                // For some reason, the ATF Gold disks contain USNF missions, but
                // do not contain the USNF assets. Not sure how that works.
                if game.test_dir == "ATFGOLD"
                    && (meta.name().contains("UKR")
                        || meta.name() == "KURILE.MM"
                        || meta.name() == "VIET.MM")
                {
                    continue;
                }

                // This looks a fragment of an MM used for... something?
                if meta.name() == "$VARF.MM" {
                    continue;
                }

                println!(
                    "At: {}:{:13} @ {}",
                    game.test_dir,
                    meta.name(),
                    meta.path()
                        .map(|v| v.to_string_lossy())
                        .unwrap_or_else(|| "<none>".into())
                );

                let type_manager = TypeManager::empty();
                let contents = from_dos_string(catalog.read_sync(fid)?);
                let mm = MissionMap::from_str(&contents, &type_manager, catalog)?;
                assert_eq!(mm.map_name().base_texture_name().len(), 3);
                assert!(mm.map_name().t2_name().ends_with(".T2"));
            }
        }

        Ok(())
    }

    #[test]
    fn it_can_parse_all_m_files() -> Result<()> {
        let catalogs = CatalogManager::for_testing()?;
        for (game, catalog) in catalogs.all() {
            for fid in catalog.find_with_extension("M")? {
                let meta = catalog.stat_sync(fid)?;

                // Quick mission fragments...
                if meta.name().starts_with("~Q") {
                    continue;
                }

                // For some reason, the ATF Gold disks contain USNF missions, but
                // do not contain the USNF assets. Not sure how that works.
                // if game.test_dir == "ATFGOLD"
                //     && (meta.name().contains("UKR")
                //     || meta.name() == "KURILE.MM"
                //     || meta.name() == "VIET.MM")
                // {
                //     continue;
                // }

                // This looks a fragment of an MM used for... something?
                // if meta.name() == "$VARF.MM" {
                //     continue;
                // }

                println!(
                    "At: {}:{:13} @ {}",
                    game.test_dir,
                    meta.name(),
                    meta.path()
                        .map(|v| v.to_string_lossy())
                        .unwrap_or_else(|| "<none>".into())
                );

                let type_manager = TypeManager::empty();
                let contents = from_dos_string(catalog.read_sync(fid)?);
                let mm = Mission::from_str(&contents, &type_manager, catalog)?;
                // assert_eq!(mm.get_base_texture_name()?.len(), 3);
                // assert!(mm.t2_name.ends_with(".T2"));
            }
        }

        Ok(())
    }
}
