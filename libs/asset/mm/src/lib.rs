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

mod obj;
mod special;
mod util;
mod waypoint;

use crate::{obj::ObjectInfo, special::SpecialInfo, waypoint::Waypoint};
use anyhow::{anyhow, bail, ensure, Result};
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

#[derive(Debug)]
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

#[derive(Debug)]
pub struct TMap {
    pub orientation: MapOrientation,
    pub loc: TLoc,
}

#[derive(Debug)]
pub struct TDic {
    n: usize,
    map: [[u8; 4]; 8],
}

#[allow(dead_code)]
pub struct MissionMap {
    map_name: String,
    t2_name: String,
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

        let mut map_name = None;
        let mut t2_name = None;
        let mut layer_name = None;
        let mut layer_index = None;
        let mut wind = Some((0, 0));
        let mut view = None;
        let mut time = None;
        let mut sides: Vec<u8> = Vec::with_capacity(64);
        let mut objects = Vec::with_capacity(obj_cnt);
        let mut specials: Vec<SpecialInfo> = Vec::with_capacity(special_cnt);
        let mut tmaps = HashMap::with_capacity(tmap_cnt);
        let mut tdics = Vec::with_capacity(tdic_cnt);

        ensure!(
            tokens.next() == Some("textFormat"),
            "missing textFormat node in MM"
        );
        while let Some(token) = tokens.next() {
            if token.starts_with(';') {
                continue;
            }
            match token {
                "map" => {
                    assert!(map_name.is_none());
                    map_name = Some(tokens.next().expect("map name").to_owned());
                    t2_name = Some(Self::find_t2_for_map(map_name.as_ref().unwrap(), catalog)?);
                }
                "layer" => {
                    layer_name = Some(Self::find_layer(
                        map_name.as_ref().expect("map before layer"),
                        tokens.next().expect("layer name"),
                        catalog,
                    )?);
                    layer_index = Some(tokens.next().expect("layer index").parse::<usize>()?);
                }
                "clouds" => {
                    ensure!(
                        tokens.next().expect("clouds") == "0",
                        "expected 0 clouds value"
                    );
                }
                "wind" => {
                    // The air is perfectly still in Ukraine.
                    let x = str::parse::<i16>(tokens.next().expect("wind x"))?;
                    let z = str::parse::<i16>(tokens.next().expect("wind z"))?;
                    wind = Some((x, z));
                }
                "view" => {
                    assert_eq!(view, None);
                    let x = str::parse::<u32>(tokens.next().expect("view x"))?;
                    let y = str::parse::<u32>(tokens.next().expect("view y"))?;
                    let z = str::parse::<u32>(tokens.next().expect("view z"))?;
                    view = Some((x, y, z));
                }
                "time" => {
                    assert_eq!(time, None);
                    let h = str::parse::<u8>(tokens.next().expect("time h"))?;
                    let m = str::parse::<u8>(tokens.next().expect("time m"))?;
                    time = Some((h, m));
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
                    assert_eq!(historical_era, 4);
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

        Ok(MissionMap {
            map_name: map_name.ok_or_else(|| anyhow!("mm must have a 'map' key"))?,
            t2_name: t2_name.ok_or_else(|| anyhow!("mm must have a 'map' key"))?,
            layer_name: layer_name.ok_or_else(|| anyhow!("mm must have a 'layer' key"))?,
            layer_index: layer_index.ok_or_else(|| anyhow!("mm must have a 'layer' key"))?,
            wind: wind.ok_or_else(|| anyhow!("mm must have a 'wind' key"))?,
            view: view.ok_or_else(|| anyhow!("mm must have a 'view' key"))?,
            time: time.ok_or_else(|| anyhow!("mm must have a 'time' key"))?,
            tmaps,
            tdics,
            objects,
        })
    }

    pub fn map_name(&self) -> &str {
        &self.map_name
    }

    pub fn t2_name(&self) -> &str {
        &self.t2_name
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
                let mm = MissionMap::from_str(&contents, &type_manager, &catalog)?;
                assert_eq!(mm.get_base_texture_name()?.len(), 3);
                assert!(mm.t2_name.ends_with(".T2"));
            }
        }

        Ok(())
    }
}
