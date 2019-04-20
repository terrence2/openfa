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
#![allow(clippy::cyclomatic_complexity)]

mod obj;
mod special;
mod util;
mod waypoint;

use crate::{obj::ObjectInfo, special::SpecialInfo, waypoint::Waypoint};
use failure::{bail, ensure, err_msg, Fallible};
use std::{collections::HashMap, str::FromStr};
use xt::TypeManager;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum TLoc {
    Index(usize),
    Name(String),
}

impl TLoc {
    pub fn pic_file(&self, base: &str) -> String {
        match self {
            TLoc::Index(ref i) => format!("{}{}.PIC", base, i),
            TLoc::Name(ref s) => s.to_owned(),
        }
    }
}

#[derive(Debug)]
pub enum MapOrientation {
    Unk0,
    Unk1,
    FlipS,
    RotateCCW,
}

impl MapOrientation {
    pub fn from_byte(n: u8) -> Fallible<Self> {
        Ok(match n {
            0 => MapOrientation::Unk0,
            1 => MapOrientation::Unk1,
            2 => MapOrientation::FlipS,
            3 => MapOrientation::RotateCCW,
            _ => bail!("invalid orientation"),
        })
    }
}

#[derive(Debug)]
pub struct TMap {
    pub orientation: MapOrientation,
    pub loc: TLoc,
}

#[allow(dead_code)]
pub struct TDic {
    n: usize,
    map: [[u8; 4]; 8],
}

#[allow(dead_code)]
pub struct MissionMap {
    pub map_name: String,
    pub layer_name: String,
    pub layer_index: usize,
    pub tmaps: HashMap<(u32, u32), TMap>,
    pub tdics: Vec<TDic>,
    pub wind: (i16, i16),
    pub view: (u32, u32, u32),
    pub time: (u8, u8),
}

impl MissionMap {
    pub fn from_str(s: &str, type_manager: &TypeManager) -> Fallible<Self> {
        let lines = s.lines().collect::<Vec<&str>>();
        assert_eq!(lines[0], "textFormat");

        let mut map_name = None;
        let mut layer_name = None;
        let mut layer_index = None;
        let mut wind = Some((0, 0));
        let mut view = None;
        let mut time = None;
        let mut sides = Vec::new();
        let mut objects = Vec::new();
        let mut specials = Vec::new();
        let mut tmaps = HashMap::new();
        let mut tdics = Vec::new();

        let mut offset = 1;
        while offset < lines.len() {
            let line = if let Some(offset) = lines[offset].find(';') {
                &lines[offset][0..offset]
            } else {
                lines[offset]
            };
            let parts = line.split(' ').collect::<Vec<&str>>();
            if parts.is_empty() {
                continue;
            }

            match parts[0] {
                "map" => {
                    assert!(map_name.is_none());
                    map_name = Some(parts[1].to_owned());
                }
                "layer" => {
                    layer_name = Some(parts[1].to_owned());
                    layer_index = Some(parts[2].parse::<usize>()?);
                }
                "clouds" => {
                    ensure!(parts[1] == "0", "expected 0 clouds value");
                }
                "wind" => {
                    // The air is perfectly still in Ukraine.
                    let x = str::parse::<i16>(parts[1])?;
                    let z = str::parse::<i16>(parts[2])?;
                    wind = Some((x, z));
                }
                "view" => {
                    assert_eq!(view, None);
                    let x = str::parse::<u32>(parts[1])?;
                    let y = str::parse::<u32>(parts[2])?;
                    let z = str::parse::<u32>(parts[3])?;
                    view = Some((x, y, z));
                }
                "time" => {
                    assert_eq!(time, None);
                    let h = str::parse::<u8>(parts[1])?;
                    let m = str::parse::<u8>(parts[2])?;
                    time = Some((h, m));
                }
                "sides" => {
                    // Only used by Ukraine.
                    assert!(sides.is_empty());
                    loop {
                        let next_offset = offset + 1;
                        if let Ok(side) = str::parse::<u8>(lines[next_offset].trim()) {
                            assert!(side == 0 || side == 128);
                            sides.push(side);
                            offset = next_offset;
                        } else {
                            break;
                        }
                    }
                    //println!("S1: {}", sides.len());
                }
                "sides2" => {
                    // Same as `sides`, but with hex values. Same 0 or 128 assertion.
                    assert!(sides.is_empty());
                    loop {
                        let next_offset = offset + 1;
                        let trimmed = lines[next_offset].trim();
                        if !trimmed.starts_with('$') {
                            break;
                        }
                        let side = u8::from_str_radix(&trimmed[1..], 16)?;
                        assert!(side == 0 || side == 128);
                        sides.push(side);
                        offset = next_offset;
                    }
                    //println!("S2: {}", sides.len());
                }
                "sides3" => {
                    // Same as `sides2`.
                    assert!(sides.is_empty());
                    loop {
                        let next_offset = offset + 1;
                        let trimmed = lines[next_offset].trim();
                        if !trimmed.starts_with('$') {
                            break;
                        }
                        let side = u8::from_str_radix(&trimmed[1..], 16)?;
                        assert!(side == 0 || side == 128);
                        sides.push(side);
                        offset = next_offset;
                    }
                    //println!("S3: {}", sides.len());
                }
                "sides4" => {
                    // Same as `sides2`.
                    assert!(sides.is_empty());
                    loop {
                        let next_offset = offset + 1;
                        let trimmed = lines[next_offset].trim();
                        if !trimmed.starts_with('$') {
                            break;
                        }
                        let side = u8::from_str_radix(&trimmed[1..], 16)?;
                        assert!(side == 0 || side == 128);
                        sides.push(side);
                        offset = next_offset;
                    }
                    //println!("S4: {}", sides.len());
                }
                "historicalera" => {
                    let historical_era = u8::from_str(parts[1])?;
                    assert_eq!(historical_era, 4);
                }
                "obj" => {
                    offset += 1;
                    let obj = ObjectInfo::from_lines(&lines, &mut offset, type_manager)?;
                    objects.push(obj);
                }
                "special" => {
                    offset += 1;
                    let special = SpecialInfo::from_lines(&lines, &mut offset)?;
                    specials.push(special);
                }
                "tmap" => {
                    let x = parts[1].parse::<i16>()? as u32;
                    let y = parts[2].parse::<i16>()? as u32;
                    tmaps.insert(
                        (x, y),
                        TMap {
                            orientation: MapOrientation::from_byte(
                                parts[4].trim_end().parse::<u8>()?,
                            )?,
                            loc: TLoc::Index(parts[3].parse::<usize>()?),
                        },
                    );
                }
                "tmap_named" => {
                    let x = parts[2].parse::<i16>()? as u32;
                    let y = parts[3].parse::<i16>()? as u32;
                    tmaps.insert(
                        (x, y),
                        TMap {
                            orientation: MapOrientation::from_byte(0)?,
                            loc: TLoc::Name(format!("{}.PIC", parts[1].to_uppercase())),
                        },
                    );
                }
                "tdic" => {
                    offset += 1;
                    fn line_to_bits(line: &str) -> Fallible<Vec<u8>> {
                        let mut out = Vec::new();
                        for s in line.split(' ') {
                            out.push(s.trim().parse::<u8>()?);
                        }
                        Ok(out)
                    }
                    let r0 = line_to_bits(lines[offset])?;
                    let r1 = line_to_bits(lines[offset + 1])?;
                    let r2 = line_to_bits(lines[offset + 2])?;
                    let r3 = line_to_bits(lines[offset + 3])?;
                    let r4 = line_to_bits(lines[offset + 4])?;
                    let r5 = line_to_bits(lines[offset + 5])?;
                    let r6 = line_to_bits(lines[offset + 6])?;
                    let r7 = line_to_bits(lines[offset + 7])?;
                    offset += 7;
                    let tdic = TDic {
                        n: parts[1].parse::<usize>()?,
                        map: [
                            [r0[0], r0[1], r0[2], r0[3]],
                            [r1[0], r1[1], r1[2], r1[3]],
                            [r2[0], r2[1], r2[2], r2[3]],
                            [r3[0], r3[1], r3[2], r3[3]],
                            [r4[0], r4[1], r4[2], r4[3]],
                            [r5[0], r5[1], r5[2], r5[3]],
                            [r6[0], r6[1], r6[2], r6[3]],
                            [r7[0], r7[1], r7[2], r7[3]],
                        ],
                    };
                    tdics.push(tdic);
                }
                "waypoint2" => {
                    let cnt = parts[1].parse::<usize>()?;
                    offset += 1;
                    let mut waypoints = Vec::new();
                    for i in 0..cnt {
                        let wp = Waypoint::from_lines(&lines, &mut offset)?;
                        assert_eq!(wp.index as usize, i);
                        waypoints.push(wp);
                    }
                    let wfor = lines[offset].split(' ').collect::<Vec<&str>>();
                    ensure!(wfor[0] == "\tw_for", "expected w_for after waypoint list");
                    let alias = wfor[1].parse::<i32>()?;
                    let mut found = false;
                    for obj in objects.iter_mut() {
                        if obj.alias == alias {
                            found = true;
                            obj.waypoints = Some(waypoints);
                            break;
                        }
                    }
                    ensure!(found, "did not find the object with alias {}", alias);
                    offset += 1;
                    ensure!(lines[offset] == "\t.", "expected . after waypoint decl");
                }
                "" => {}
                "\0" | "\x1A" => {
                    // Why not EOF? Or nothing at all?
                }
                _ => {
                    println!("line{}: {:?}", offset, parts);
                    bail!("unknown mission map key: {}", parts[0]);
                }
            }

            offset += 1;
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
            map_name: map_name.ok_or_else(|| err_msg("mm must have a 'map' key"))?,
            layer_name: layer_name.ok_or_else(|| err_msg("mm must have a 'layer' key"))?,
            layer_index: layer_index.ok_or_else(|| err_msg("mm must have a 'layer' key"))?,
            wind: wind.ok_or_else(|| err_msg("mm must have a 'wind' key"))?,
            view: view.ok_or_else(|| err_msg("mm must have a 'view' key"))?,
            time: time.ok_or_else(|| err_msg("mm must have a 'time' key"))?,
            tmaps,
            tdics,
        })
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
    pub fn find_t2_for_map(&self, file_exists: &Fn(&str) -> bool) -> Fallible<String> {
        let raw = self.map_name.to_uppercase();

        if file_exists(&raw) {
            return Ok(raw.to_owned());
        }

        // ~KURILE.T2 && ~TVIET.T2
        if raw.starts_with('~') && file_exists(&raw[1..]) {
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
            return Ok(base[1..=3].to_owned() + ".T2");
        }

        bail!("no map file matching {} found", raw)
    }

    // This is a slightly different problem then getting the T2, because even though ~ABCn.T2
    // might exist for ~ABCn.MM, we need to look up ABCi.PIC without the tilda.
    pub fn get_base_texture_name(&self) -> Fallible<String> {
        let raw = self.map_name.to_uppercase();
        let mut name = raw
            .split('.')
            .next()
            .ok_or_else(|| err_msg("expected a dotted name"))?;
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
    use omnilib::OmniLib;

    #[test]
    fn it_can_parse_all_mm_files() -> Fallible<()> {
        let omni = OmniLib::new_for_test_in_games(&[
            "FA", "USNF97", "ATFGOLD", "ATFNATO", "ATF", "MF", "USNF",
        ])?;
        for (game, name) in omni.find_matching("*.MM")?.iter() {
            if game == "ATFGOLD"
                && (name.contains("UKR") || name == "KURILE.MM" || name == "VIET.MM")
            {
                continue;
            }

            if name == "$VARF.MM" {
                // This looks a fragment of an MM used for... something?
                continue;
            }

            println!(
                "At: {}:{} @ {}",
                game,
                name,
                omni.path(game, name)
                    .unwrap_or_else(|_| "<unknown>".to_owned())
            );
            let lib = omni.library(game);
            let type_manager = TypeManager::new(lib.clone());
            let contents = lib.load_text(name)?;
            let mm = MissionMap::from_str(&contents, &type_manager)?;
            assert_eq!(mm.get_base_texture_name()?.len(), 3);
            assert!(mm
                .find_t2_for_map(&|s| lib.file_exists(s))?
                .ends_with(".T2"));
        }

        Ok(())
    }
}
