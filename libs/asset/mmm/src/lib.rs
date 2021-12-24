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

pub use crate::{
    formation::{FormationControl, FormationKind, WingFormation},
    special::SpecialInfo,
};

use crate::util::maybe_hex;
use crate::{obj::ObjectInfo, waypoint::Waypoints};
use anyhow::{anyhow, bail, ensure, Result};
use bitflags::bitflags;
use catalog::Catalog;
use geodesy::CartesianOrigin;
use lib::from_dos_string;
use log::debug;
use std::{borrow::Cow, collections::HashMap, str::FromStr};
use xt::TypeManager;

/// Map coordinates are relative to the bottom left corner.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct MmmOrigin;
impl CartesianOrigin for MmmOrigin {
    fn origin_name() -> &'static str {
        "missionmap"
    }
}

/// This mission is used to show the "vehicle info" screen in the reference.
///
/// INPUTS:
///     <selected> - an XT file for alias -2
pub const VEHICLE_INFO_MISSION: &str = "~INFO.M";

/// ~/$MC\[_NATO\].M claim to be the base for created missions.
/// INPUTS: none
pub const NEW_MISSION_PREFIX: &str = "~MC";

/// Freeflight mission prefix. There are a bunch of missions with a freeflight tag
/// that generally take a <selected> input, but also a bunch that have a fixed input
/// XT that generally matches the name. The intent is probably the same for all of
/// these. There are no ~A missions that do not have the freeflight tag. These are
/// generally able to be parsed independently, unlike quick mission fragments.
///
/// INPUTS:
///     <selected> - but some are hardcoded
pub const FREEFLIGHT_PREFIX: &str = "~A";

/// Quick mission prefix. Quick missions are shipped as pile of fragments that gets
/// catted together based on what's selected in the GUI with a bunch of variables
/// provided. These are generally tokenable, but not individually recognizable as
/// anything mission-like.
///
/// INPUTS:
///     <aaa>
///     <afv>
///     <cargo>
///     <carrier>
///     <cruiser>
///     <destroyer>
///     <hovercraft>
///     <sam>
///     <small>
///     <tank>
pub const QUICK_MISSION_PREFIX: &str = "~Q";

/// Multiplayer missions? These have Red and Blue bases, but nothing else.
/// Clearly just a framework for something more.
///
/// INPUTS:
///     <aaa>
///     <jstars>
///     <sam>
///     <wateraaa>
///     <watersam>
pub const MULTIPLAYER_MISSION_PREFIX: &str = "~F";

// It appears that '$' got changed to '~' in filenames (when moving to
// windows?), but only M/MM references were caught, not PICs. Thus, this
// local routine to normalize.
pub(crate) fn canonicalize(name: &str) -> Cow<str> {
    if name.starts_with('$') {
        Cow::from(name.replace('$', "~"))
    } else {
        Cow::from(name)
    }
}

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
    //
    // There are only unadorned T2, but many of the T2 references in M/MM are adorned with a sigil
    // and a number. It turns out when these appear in M files, those map directly to MM instead of
    // T2 (with some squinting at the sigil). So I think how this goes is: loading an M, findes the
    // MM (instead of T2) referenced in the map_name, then loading the MM strips any sigil and
    // number to find the T2, since the MM are the ones with tmap and tdict entries needed to
    // actually understand what's in the T2 file.
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
        } else if maybe_number == 'F' {
            // Note that ~KURILE and ~TVIET also exist; the E in kurile would be parsed as hex,
            // so we can't just interpret as hex here. I don't know the significance of F.
            number = Some(15);
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

#[derive(Copy, Clone, Debug)]
pub enum CodeCookie {
    CatFail,
    Extra01,
    Extra02,
    K16,
    K17,
    Train01,
    U01,
    U07,
    U08,
    U11,
    U12,
    U15,
    U22,
    U23,
    U24,
    U25,
    U29,
    U34,
    Ukr02,
    Viet03,
}

impl CodeCookie {
    fn from_str(s: &str) -> Result<Self> {
        Ok(match s {
            "catfail" => Self::CatFail,
            "extra01" => Self::Extra01,
            "extra02" => Self::Extra02,
            "k16" => Self::K16,
            "k17" => Self::K17,
            "train01" => Self::Train01,
            "u01" => Self::U01,
            "u07" => Self::U07,
            "u08" => Self::U08,
            "u11" => Self::U11,
            "u12" => Self::U12,
            "u15" => Self::U15,
            "u22" => Self::U22,
            "u23" => Self::U23,
            "u24" => Self::U24,
            "u25" => Self::U25,
            "u29" => Self::U29,
            "u34" => Self::U34,
            "ukr02" => Self::Ukr02,
            "viet03" => Self::Viet03,
            _ => bail!("unknown code type {}", s),
        })
    }
}

bitflags! {
    pub struct ScreenSet : u8 {
        const BRIEFING = 0x01;
        const BRIEFING_MAP = 0x02;
        const SELECT_PLANE = 0x04;
        const ARM_PLANE = 0x08;
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
    PrintMissionOutcome(bool),
    Code(CodeCookie),
    MapName(MapName),
    Layer((String, usize)),
    Clouds(u32),
    Wind((i16, i16)),
    View((u32, u32, u32)),
    Time((u8, u8)),
    Revive((u8, u8, u8)),
    EndScenario((u32, u32, u32)),
    UsAirSkill(u8),
    UsGroundSkill(u8),
    ThemAirSkill(u8),
    ThemGroundSkill(u8),
    FreeFlight,
    MapObjSuccessFlags((i32, u8)),
    Sides(Vec<u8>),
    HistoricalEra(u8),
    TMaps(HashMap<(u32, u32), TMap>),
    TDics(Vec<TDic>),
    Objects(Vec<ObjectInfo>),
    Specials(Vec<SpecialInfo>),
    GunsOnly,
}

impl MValue {
    fn tokenize(s: &str, type_manager: &TypeManager, catalog: &Catalog) -> Result<Vec<MValue>> {
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
        let mut objects_by_alias = HashMap::with_capacity(obj_cnt);
        let mut objects = Vec::with_capacity(obj_cnt);
        let mut specials: Vec<SpecialInfo> = Vec::with_capacity(special_cnt);
        let mut tmaps = HashMap::with_capacity(tmap_cnt);
        let mut tdics = Vec::with_capacity(tdic_cnt);

        while let Some(token) = tokens.next() {
            assert!(!token.starts_with(';'));
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
                "printmissionoutcome" | "printMissionOutcome" => {
                    let v = str::parse::<u8>(tokens.next().expect("allow rearm value"))?;
                    ensure!(v == 0);
                    mm.push(MValue::PrintMissionOutcome(false));
                }
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
                        raw_layer_name,
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
                "code" => {
                    let cookie = CodeCookie::from_str(tokens.next().expect("code cookie"))?;
                    mm.push(MValue::Code(cookie));
                }
                "time" => {
                    let h = str::parse::<u8>(tokens.next().expect("time h"))?;
                    let m = str::parse::<u8>(tokens.next().expect("time m"))?;
                    mm.push(MValue::Time((h, m)));
                }
                "revive" => {
                    let a = str::parse::<u8>(tokens.next().expect("revive lives"))?;
                    let b = str::parse::<u8>(tokens.next().expect("revive wait"))?;
                    let c = str::parse::<u8>(tokens.next().expect("revive unk"))?;
                    ensure!(a <= 4);
                    ensure!(b == 0 || b == 15);
                    ensure!(c == 10);
                    mm.push(MValue::Revive((a, b, c)));
                }
                "endscenario" => {
                    let a = str::parse::<u32>(tokens.next().expect("endscenario timeout"))?;
                    let b = str::parse::<u32>(tokens.next().expect("endscenario unk 1"))?;
                    let c = str::parse::<u32>(tokens.next().expect("endscenario unk 2"))?;
                    ensure!(a == 600 || a == 900 || a == 1200 || a == 1500);
                    ensure!(b == 0x7FFF_FFFF);
                    ensure!(c == 0);
                    mm.push(MValue::EndScenario((a, b, c)));
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
                "freeflight" | "freeFlight" => mm.push(MValue::FreeFlight),
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
                "map_obj_success_flags" => {
                    // Only used in a handful of vietnam missions: T02, T08, T10.
                    // Seems like it might be positional, since the first arg doesn't map to an
                    // alias or anything else obvious, even in the MM.
                    let a = str::parse(tokens.next().expect("map_obj_success_flags a"))?;
                    let b = maybe_hex(tokens.next().expect("map_obj_success_flags b"))?;
                    ensure!(a < 0);
                    ensure!(b == 0x80);
                    mm.push(MValue::MapObjSuccessFlags((a, b)));
                }
                "obj" => {
                    let obj = ObjectInfo::from_tokens(&mut tokens, type_manager, catalog)?;
                    let obj_offset = objects.len();
                    objects.push(obj);
                    if let Some(alias) = objects[obj_offset].alias() {
                        ensure!(
                            !objects_by_alias.contains_key(&alias),
                            "duplicate alias detected"
                        );
                        objects_by_alias.insert(alias, obj_offset);
                    }
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
                    let wp = Waypoints::from_tokens(cnt, &mut tokens)?;
                    let obj_offset = *objects_by_alias
                        .get(&wp.for_alias())
                        .ok_or_else(|| anyhow!("waypoints for unknown object"))?;
                    objects[obj_offset].set_waypoints(wp);
                }
                "\0" | "\x1A" => {
                    // DOS EOF char, but not always at eof.
                }
                "gunsOnly" => {
                    // Used only in training mission 9: EXTRA09.M in USNF+MF and UKR09 in USNF97.
                    // TODO: does FA even support this mission properly?
                    mm.push(MValue::GunsOnly);
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

        if !sides.is_empty() {
            mm.push(MValue::Sides(sides));
        }

        if !specials.is_empty() {
            mm.push(MValue::Specials(specials));
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
    wind: Option<(i16, i16)>,
    view: (u32, u32, u32),
    time: (u8, u8),
    sides: Vec<u8>,
    objects: Vec<ObjectInfo>,
    specials: Vec<SpecialInfo>,
}

impl MissionMap {
    pub fn from_str(s: &str, type_manager: &TypeManager, catalog: &Catalog) -> Result<Self> {
        let mut tokens = MValue::tokenize(s, type_manager, catalog)?;

        let mut map_name = None;
        let mut layer_name = None;
        let mut layer_index = None;
        let mut wind = None;
        let mut view = None;
        let mut time = None;
        let mut sides = None;
        let mut specials = None;
        let mut tmaps = None;
        let mut tdics = None;
        let mut objects = None;

        ensure!(
            matches!(tokens[0], MValue::TextFormat),
            "missing textFormat node in MM"
        );
        for value in tokens.drain(..) {
            match value {
                MValue::TextFormat => {}
                MValue::Brief => bail!("Brief in MM"),
                MValue::BriefMap => bail!("BriefMap in MM"),
                MValue::SelectPlane => bail!("SelectPlane in MM"),
                MValue::ArmPlane => bail!("ArmPlane in MM"),
                MValue::AllowRearmRefuel(_) => bail!("AllowRearmRefuel in MM"),
                MValue::PrintMissionOutcome(_) => bail!("PrintMissionOutcome in MM"),
                MValue::Code(_) => bail!("Code in MM"),
                MValue::Wind(v) => wind = Some(v),
                MValue::Revive(_) => bail!("Revive in MM"),
                MValue::EndScenario(_) => bail!("EndScenario in MM"),
                MValue::UsAirSkill(_) => bail!("UsAirSkill in MM"),
                MValue::UsGroundSkill(_) => bail!("UsGroundSkill in MM"),
                MValue::ThemAirSkill(_) => bail!("ThemAirSkill in MM"),
                MValue::ThemGroundSkill(_) => bail!("ThemGroundSkill in MM"),
                MValue::FreeFlight => bail!("FreeFlight in MM"),
                MValue::MapObjSuccessFlags(_) => bail!("MapObjSuccessFlags in MM"),
                MValue::GunsOnly => bail!("GunsOnly in MM"),
                MValue::Sides(v) => sides = Some(v),
                MValue::MapName(map) => {
                    // ensure!(map_name.parent(name.chars().next().unwrap()) == name);
                    map_name = Some(map);
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
                MValue::Specials(sps) => specials = Some(sps),
            }
        }

        Ok(MissionMap {
            map_name: map_name.ok_or_else(|| anyhow!("mm must have a 'map' key"))?,
            layer_name: layer_name.ok_or_else(|| anyhow!("mm must have a 'layer' key"))?,
            layer_index: layer_index.ok_or_else(|| anyhow!("mm must have a 'layer' key"))?,
            wind,
            view: view.ok_or_else(|| anyhow!("mm must have a 'view' key"))?,
            time: time.ok_or_else(|| anyhow!("mm must have a 'time' key"))?,
            sides: sides.ok_or_else(|| anyhow!("mm must have 'sides' key"))?,
            tmaps: tmaps.ok_or_else(|| anyhow!("mm must have 'tmaps' keys"))?,
            tdics: tdics.ok_or_else(|| anyhow!("mm must have 'tdics' keys"))?,
            objects: objects.ok_or_else(|| anyhow!("mm must have 'object' keys"))?,
            specials: specials.ok_or_else(|| anyhow!("mm must have 'special' keys"))?,
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

    pub fn objects(&self) -> impl Iterator<Item = &ObjectInfo> {
        self.objects.iter()
    }
}

/// Represents an M file.
pub struct Mission {
    mm: MissionMap,
    map_name: MapName,
    layer_name: String,
    layer_index: usize,
    screens: ScreenSet,
    view: (u32, u32, u32),
    wind: (i16, i16),
    time: (u8, u8),
    clouds: u32,
    us_air_skill: u8,
    us_ground_skill: u8,
    them_air_skill: u8,
    them_ground_skill: u8,
    free_flight: bool,
    guns_only: bool,
    allow_rearm_refuel: Option<bool>,
    print_mission_outcome: Option<bool>,
    code_cookie: Option<CodeCookie>,
    revive: Option<(u8, u8, u8)>,
    end_scenario: Option<(u32, u32, u32)>,
    sides: Vec<u8>,
    objects: Vec<ObjectInfo>,
}

impl Mission {
    pub fn from_str(s: &str, type_manager: &TypeManager, catalog: &Catalog) -> Result<Self> {
        let mut tokens = MValue::tokenize(s, type_manager, catalog)?;

        let mut mm = None;
        let mut map_name = None;
        let mut screens = ScreenSet::empty();
        let mut allow_rearm_refuel = None;
        let mut print_mission_outcome = None;
        let mut code_cookie = None;
        let mut wind = None;
        let mut revive = None;
        let mut end_scenario = None;
        let mut us_air_skill = None;
        let mut us_ground_skill = None;
        let mut them_air_skill = None;
        let mut them_ground_skill = None;
        let mut free_flight = false;
        let mut sides = None;
        let mut layer_name = None;
        let mut layer_index = None;
        let mut view = None;
        let mut time = None;
        let mut clouds = None;
        let mut objects = None;
        let mut guns_only = false;

        ensure!(
            matches!(tokens[0], MValue::TextFormat | MValue::AllowRearmRefuel(_)),
            "missing textFormat node in M"
        );
        for value in tokens.drain(..) {
            match value {
                MValue::TextFormat => {}
                MValue::Brief => screens |= ScreenSet::BRIEFING,
                MValue::BriefMap => screens |= ScreenSet::BRIEFING_MAP,
                MValue::SelectPlane => screens |= ScreenSet::SELECT_PLANE,
                MValue::ArmPlane => screens |= ScreenSet::ARM_PLANE,
                MValue::AllowRearmRefuel(v) => allow_rearm_refuel = Some(v),
                MValue::PrintMissionOutcome(v) => print_mission_outcome = Some(v),
                MValue::Code(v) => code_cookie = Some(v),
                MValue::Wind(v) => wind = Some(v),
                MValue::Revive(v) => revive = Some(v),
                MValue::EndScenario(v) => end_scenario = Some(v),
                MValue::UsAirSkill(v) => us_air_skill = Some(v),
                MValue::UsGroundSkill(v) => us_ground_skill = Some(v),
                MValue::ThemAirSkill(v) => them_air_skill = Some(v),
                MValue::ThemGroundSkill(v) => them_ground_skill = Some(v),
                MValue::FreeFlight => free_flight = true,
                MValue::MapObjSuccessFlags(_) => {
                    // TODO: we probably need to handle this as part of the next(?) object?
                }
                MValue::Sides(v) => sides = Some(v),
                MValue::MapName(map) => {
                    let mm_raw = catalog.read_name_sync(&map.parent())?;
                    let mm_content = from_dos_string(mm_raw);
                    mm = Some(MissionMap::from_str(&mm_content, type_manager, catalog)?);
                    map_name = Some(map);
                }
                MValue::Layer((name, index)) => {
                    layer_name = Some(name);
                    layer_index = Some(index);
                }
                MValue::View(v) => view = Some(v),
                MValue::Time(t) => time = Some(t),
                MValue::Clouds(v) => clouds = Some(v),
                MValue::HistoricalEra(historical_era) => ensure!(historical_era == 4),
                MValue::Objects(objs) => objects = Some(objs),
                MValue::TMaps(_) => bail!("TMaps not allowed in M files"),
                MValue::TDics(_) => bail!("TDics not allowed in M files"),
                MValue::Specials(_) => bail!("Special markers not allowed in M files"),
                MValue::GunsOnly => guns_only = true,
            }
        }

        Ok(Mission {
            map_name: map_name.ok_or_else(|| anyhow!("Missions must have a map_name"))?,
            mm: mm.ok_or_else(|| anyhow!("Missions must have a parent MissionMap"))?,
            layer_name: layer_name.ok_or_else(|| anyhow!("Missions must have a layer name"))?,
            layer_index: layer_index.ok_or_else(|| anyhow!("Missions must have a layer index"))?,
            screens,
            view: view.ok_or_else(|| anyhow!("mission must have view"))?,
            wind: wind.ok_or_else(|| anyhow!("mission must have wind"))?,
            time: time.ok_or_else(|| anyhow!("mission must have time"))?,
            clouds: clouds.ok_or_else(|| anyhow!("mission must have clouds"))?,
            us_air_skill: us_air_skill.ok_or_else(|| anyhow!("mission must have usAirSkill"))?,
            us_ground_skill: us_ground_skill
                .ok_or_else(|| anyhow!("mission must have usGroundSkill"))?,
            them_air_skill: them_air_skill
                .ok_or_else(|| anyhow!("mission must have themAirSkill"))?,
            them_ground_skill: them_ground_skill
                .ok_or_else(|| anyhow!("mission must have themGroundSkill"))?,
            free_flight,
            guns_only,
            allow_rearm_refuel,
            print_mission_outcome,
            code_cookie,
            revive,
            end_scenario,
            sides: sides.ok_or_else(|| anyhow!("missions must have sides defined"))?,
            objects: objects.ok_or_else(|| anyhow!("missions must have objects"))?,
        })
    }

    pub fn mission_map(&self) -> &MissionMap {
        &self.mm
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

    pub fn screens(&self) -> &ScreenSet {
        &self.screens
    }

    pub fn view(&self) -> (u32, u32, u32) {
        self.view
    }

    pub fn time(&self) -> (u8, u8) {
        self.time
    }

    pub fn wind(&self) -> (i16, i16) {
        self.wind
    }

    pub fn clouds(&self) -> u32 {
        self.clouds
    }

    pub fn us_air_skill(&self) -> u8 {
        self.us_air_skill
    }

    pub fn us_ground_skill(&self) -> u8 {
        self.us_ground_skill
    }

    pub fn them_air_skill(&self) -> u8 {
        self.them_air_skill
    }

    pub fn them_ground_skill(&self) -> u8 {
        self.them_ground_skill
    }

    pub fn free_flight(&self) -> bool {
        self.free_flight
    }

    pub fn guns_only(&self) -> bool {
        self.guns_only
    }

    pub fn allow_rearm_refuel(&self) -> bool {
        self.allow_rearm_refuel.unwrap_or(false)
    }

    pub fn print_mission_outcome(&self) -> bool {
        self.print_mission_outcome.unwrap_or(false)
    }

    pub fn code_cookie(&self) -> Option<CodeCookie> {
        self.code_cookie
    }

    pub fn revive(&self) -> Option<(u8, u8, u8)> {
        self.revive
    }

    pub fn end_scenario(&self) -> Option<(u32, u32, u32)> {
        self.end_scenario
    }

    pub fn sides(&self) -> &[u8] {
        &self.sides
    }

    pub fn mission_objects(&self) -> &[ObjectInfo] {
        &self.objects
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

                println!("At: {}:{:13} @ {}", game.test_dir, meta.name(), meta.path());

                let type_manager = TypeManager::empty();
                let contents = from_dos_string(catalog.read_sync(fid)?);
                let mm = MissionMap::from_str(&contents, &type_manager, &catalog)?;
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
            let type_manager = TypeManager::empty();
            for fid in catalog.find_with_extension("M")? {
                let meta = catalog.stat_sync(fid)?;

                if meta.name() == VEHICLE_INFO_MISSION
                    || meta.name().starts_with(NEW_MISSION_PREFIX)
                    || meta.name().starts_with(FREEFLIGHT_PREFIX)
                    || meta.name().starts_with(MULTIPLAYER_MISSION_PREFIX)
                    || meta.name().starts_with(QUICK_MISSION_PREFIX)
                    || canonicalize(meta.name()) == VEHICLE_INFO_MISSION
                    || canonicalize(meta.name()).starts_with(NEW_MISSION_PREFIX)
                    || canonicalize(meta.name()).starts_with(FREEFLIGHT_PREFIX)
                    || canonicalize(meta.name()).starts_with(MULTIPLAYER_MISSION_PREFIX)
                    || canonicalize(meta.name()).starts_with(QUICK_MISSION_PREFIX)
                {
                    continue;
                }

                println!("At: {}:{:13} @ {}", game.test_dir, meta.name(), meta.path());

                let contents = from_dos_string(catalog.read_sync(fid)?);
                let mission = Mission::from_str(&contents, &type_manager, &catalog)?;
                assert!(!mission.sides.is_empty());
                assert!(mission.map_name.raw.ends_with(".T2"));
            }
        }

        Ok(())
    }
}
