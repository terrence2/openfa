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
use asset::AssetManager;
use failure::{bail, ensure, err_msg, Fallible};
use lay::Layer;
use lib::Library;
use nalgebra::{Point3, Vector3};
use num_traits::Num;
use std::{str::FromStr, sync::Arc};
use t2::Terrain;
use xt::{TypeManager, TypeRef};

pub fn maybe_hex<T>(n: &str) -> Fallible<T>
where
    T: Num + ::std::str::FromStr,
    <T as Num>::FromStrRadixErr: 'static + ::std::error::Error + Send + Sync,
    <T as ::std::str::FromStr>::Err: 'static + ::std::error::Error + Send + Sync,
{
    ensure!(n.is_ascii(), "non-ascii in number");
    return Ok(if n.starts_with('$') {
        T::from_str_radix(&n[1..], 16)?
    } else {
        n.parse::<T>()?
    });
}

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
        return Ok(match n {
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
        });
    }
}

#[allow(dead_code)]
pub struct ObjectInst {
    ty: TypeRef,
    name: Option<String>,
    pos: Point3<f32>,
    angle: Vector3<f32>,
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

impl ObjectInst {
    fn from_lines(lines: &[&str], offset: &mut usize, types: &TypeManager) -> Fallible<Self> {
        let mut ty = None;
        let mut name = None;
        let mut pos = None;
        let mut angle = Vector3::new(0f32, 0f32, 0f32);
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
            match parts[0].trim_left() {
                "type" => {
                    ty = Some(types.load(&parts[1].trim().to_uppercase())?);
                }
                "name" => name = Some(parts[1].to_owned()),
                "pos" => {
                    let ns = parts[1].split(' ').collect::<Vec<&str>>();
                    pos = Some(Point3::new(
                        ns[0].parse::<i32>()? as f32,
                        ns[1].parse::<i32>()? as f32,
                        ns[2].parse::<i32>()? as f32,
                    ));
                }
                "angle" => {
                    let ns = parts[1].split(' ').collect::<Vec<&str>>();
                    angle = Vector3::new(
                        ns[0].parse::<i32>()? as f32,
                        ns[1].parse::<i32>()? as f32,
                        ns[2].parse::<i32>()? as f32,
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
                    assert!(ty.is_some());
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
        return Ok(ObjectInst {
            ty: ty.ok_or_else(|| {
                err_msg(format!("mm:obj: type not set in obj ending {}", *offset))
            })?,
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
        });
    }
}

// special
//         pos 1347582 0 315393
//         name ^ASea of Japan^A
//         color 48
//         icon -1
//         flags $0
//         .
#[allow(dead_code)]
struct SpecialInst {
    pos: Point3<f32>,
    name: String,
    color: u8,
    icon: i32,
    flags: u16,
}

impl SpecialInst {
    fn from_lines(lines: &[&str], offset: &mut usize) -> Fallible<Self> {
        let mut pos = None;
        let mut name = None;
        let mut color = None;
        let mut icon = None;
        let mut flags = None;

        while lines[*offset].trim() != "." {
            let parts = lines[*offset].trim().splitn(2, ' ').collect::<Vec<&str>>();
            match parts[0].trim_left() {
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
        return Ok(SpecialInst {
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
        });
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
#[allow(dead_code)]
pub struct Waypoint {
    index: u8,
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
    fn from_lines(lines: &[&str], offset: &mut usize) -> Fallible<Self> {
        let mut index = None;
        let mut flags = None;
        let mut goal = None;
        let mut next = None;
        let mut pos = None;
        let mut speed = None;
        let mut wng = None;
        let mut react = None;
        let mut search_dist = None;

        while lines[*offset].trim() != "." {
            let parts = lines[*offset]
                .trim()
                .split(' ')
                .filter(|&s| s != "")
                .collect::<Vec<&str>>();
            if parts.len() == 0 {
                break;
            }
            match parts[0].trim_left() {
                "w_index" => index = Some(parts[1].parse::<u8>()?),
                "w_flags" => flags = Some(parts[1].parse::<u8>()?),
                "w_goal" => goal = Some(parts[1] == "1"),
                "w_next" => next = Some(parts[1] == "1"),
                "w_pos2" => {
                    assert_eq!(parts[1].parse::<u8>()?, 0);
                    assert_eq!(parts[2].parse::<u8>()?, 0);
                    pos = Some(Vector3::new(
                        parts[3].parse::<f32>()?,
                        parts[4].parse::<f32>()?,
                        parts[5].parse::<f32>()?,
                    ));
                }
                "w_speed" => speed = Some(parts[1].parse::<usize>()?),
                "w_wng" => {
                    wng = Some([
                        parts[1].parse::<u16>()?,
                        parts[2].parse::<u16>()?,
                        parts[3].parse::<u16>()?,
                        parts[4].parse::<u16>()?,
                    ]);
                }
                "w_react" => {
                    react = Some([
                        parts[1].parse::<u32>()?,
                        parts[2].parse::<u32>()?,
                        parts[3].parse::<u32>()?,
                    ]);
                }
                "w_searchDist" => search_dist = Some(parts[1].parse::<u8>()?),
                "w_preferredTargetId" => assert_eq!(parts[1], "0"),
                "w_name" => assert_eq!(parts[1], "\x01\x01"),
                _ => {
                    bail!("unknown waypoint key: {}", parts[0]);
                }
            }
            *offset += 1;
        }
        *offset += 1;
        return Ok(Waypoint {
            index: index.ok_or_else(|| {
                err_msg(format!(
                    "mm:waypoint: index not set in waypoint ending at {}",
                    *offset
                ))
            })?,
            flags: flags.ok_or_else(|| {
                err_msg(format!(
                    "mm:waypoint: flags not set in waypoint ending at {}",
                    *offset
                ))
            })?,
            goal: goal.ok_or_else(|| {
                err_msg(format!(
                    "mm:waypoint: goal not set in waypoint ending at {}",
                    *offset
                ))
            })?,
            next: next.ok_or_else(|| {
                err_msg(format!(
                    "mm:waypoint: next not set in waypoint ending at {}",
                    *offset
                ))
            })?,
            pos: pos.ok_or_else(|| {
                err_msg(format!(
                    "mm:waypoint: pos not set in waypoint ending at {}",
                    *offset
                ))
            })?,
            speed: speed.ok_or_else(|| {
                err_msg(format!(
                    "mm:waypoint: speed not set in waypoint ending at {}",
                    *offset
                ))
            })?,
            wng: wng.ok_or_else(|| {
                err_msg(format!(
                    "mm:waypoint: wng not set in waypoint ending at {}",
                    *offset
                ))
            })?,
            react: react.ok_or_else(|| {
                err_msg(format!(
                    "mm:waypoint: react not set in waypoint ending at {}",
                    *offset
                ))
            })?,
            search_dist: search_dist.ok_or_else(|| {
                err_msg(format!(
                    "mm:waypoint: searchDist not set in waypoint ending at {}",
                    *offset
                ))
            })?,
        });
    }
}

pub enum TLoc {
    Index(usize),
    Name(String),
}

#[allow(dead_code)]
pub struct TMap {
    pos0: i16,
    pos1: i16,
    unk: u8,
    loc: TLoc,
}

#[allow(dead_code)]
pub struct TDic {
    n: usize,
    map: [[u8; 4]; 8],
}

#[allow(dead_code)]
pub struct MissionMap {
    pub map: Arc<Box<Terrain>>,
    pub layer: Arc<Box<Layer>>,
    wind: (i16, i16),
    view: (u32, u32, u32),
    time: (u8, u8),
}

impl MissionMap {
    pub fn from_str(
        s: &str,
        lib: Arc<Box<Library>>,
        types: &TypeManager,
        assets: Arc<Box<AssetManager>>,
    ) -> Fallible<Self> {
        let lines = s.lines().collect::<Vec<&str>>();
        assert_eq!(lines[0], "textFormat");

        let mut map = None;
        let mut layer = None;
        let mut wind = Some((0, 0));
        let mut view = None;
        let mut time = None;
        let mut sides = Vec::new();
        let mut objects = Vec::new();
        let mut specials = Vec::new();
        let mut tmaps = Vec::new();

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
                    assert!(map.is_none());
                    let raw = &parts[1].to_uppercase();
                    let filename = Self::get_t2_name_for_map(raw, &lib)?;
                    map = Some(assets.load_t2(&filename)?);
                }
                "layer" => {
                    let filename = &parts[1].to_uppercase();
                    layer = Some(assets.load_lay(&filename)?);
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
                        if !trimmed.starts_with("$") {
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
                        if !trimmed.starts_with("$") {
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
                        if !trimmed.starts_with("$") {
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
                    let obj = ObjectInst::from_lines(&lines, &mut offset, types)?;
                    objects.push(obj);
                }
                "special" => {
                    offset += 1;
                    let special = SpecialInst::from_lines(&lines, &mut offset)?;
                    specials.push(special);
                }
                "tmap" => tmaps.push(TMap {
                    pos0: parts[1].parse::<i16>()?,
                    pos1: parts[2].parse::<i16>()?,
                    unk: parts[4].trim_right().parse::<u8>()?,
                    loc: TLoc::Index(parts[3].parse::<usize>()?),
                }),
                "tmap_named" => tmaps.push(TMap {
                    pos0: parts[2].parse::<i16>()?,
                    pos1: parts[3].parse::<i16>()?,
                    unk: 0,
                    loc: TLoc::Name(format!("{}.PIC", parts[1].to_uppercase())),
                }),
                "tdic" => {
                    offset += 1;
                    fn line_to_bits(line: &str) -> Fallible<Vec<u8>> {
                        let mut out = Vec::new();
                        for s in line.split(' ') {
                            out.push(s.trim().parse::<u8>()?);
                        }
                        return Ok(out);
                    }
                    let r0 = line_to_bits(lines[offset + 0])?;
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
            match tmap.loc {
                TLoc::Index(i) => assert!((i as usize) < tdics.len()),
                TLoc::Name(ref n) => assert!(lib.file_exists(n)),
            }
        }

        ensure!(map.is_some(), "mission map must have a 'map' key");
        ensure!(layer.is_some(), "mission map must have a 'layer' key");
        return Ok(MissionMap {
            map: map.unwrap(),
            layer: layer.unwrap(),
            wind: wind.unwrap(),
            view: view.unwrap(),
            time: time.unwrap(),
        });
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
    pub fn get_t2_name_for_map(raw: &str, lib: &Arc<Box<Library>>) -> Fallible<String> {
        if lib.file_exists(raw) {
            return Ok(raw.to_owned());
        }

        // ~KURILE.T2 && ~TVIET.T2
        if raw.starts_with('~') && lib.file_exists(&raw[1..]) {
            return Ok(raw[1..].to_owned());
        }

        let parts = raw.split('.').collect::<Vec<&str>>();
        let sym = parts[0];
        if sym.len() == 5 {
            let ss = sym.chars().next().unwrap();
            let se = sym.chars().rev().take(1).collect::<String>();
            println!("SYM: {}, ss: {}, se: {}", sym, ss, se);
            ensure!(
                ss == '~' || ss == '$',
                "expected non-literal map name to start with $ or ~"
            );
            ensure!(
                se == "F" || se.parse::<u8>().is_ok(),
                "expected non-literal map name to end with f or a number"
            );
            return Ok(sym[1..=3].to_owned() + ".T2");
        }

        bail!("no map file matching {} found", raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omnilib::OmniLib;

    #[test]
    fn it_can_parse_all_mm_files() -> Fallible<()> {
        let omni = OmniLib::new_for_test_in_games(vec![
            "FA",
            "USNF97",
            "ATFGOLD",
            "ATFNATO",
            "ATF",
            "MF",
            "USNF"
        ])?;
        for (game, name) in omni.find_matching("*.MM")?.iter() {
            if game == "ATFGOLD" {
                if name.contains("UKR") || name == "KURILE.MM" || name == "VIET.MM" {
                    continue;
                }
            }
            if name == "$VARF.MM" {
                // This looks a fragment of an MM used for... something?
                continue;
            }

            println!("At: {}:{} @ {}", game, name, omni.path(game, name)?);
            let lib = omni.library(game);
            let assets = Arc::new(Box::new(AssetManager::new(lib.clone())?));
            let types = TypeManager::new(lib.clone(), assets.clone())?;
            let contents = lib.load_text(name)?;
            let _mm = MissionMap::from_str(&contents, lib.clone(), &types, assets.clone())?;
        }

        return Ok(());
    }
}
