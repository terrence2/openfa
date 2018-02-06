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
#[macro_use]
extern crate error_chain;
extern crate peff;
extern crate ansi;

mod errors {
    error_chain!{}
}
use errors::{Error, ErrorKind, Result, ResultExt};

use std::path::{Path, PathBuf};
use std::io::prelude::*;
use std::{cmp, fs, mem, str};
use std::collections::{HashMap, HashSet};
use ansi::{Escape, Color};

pub struct Shape {
    pub vertices: Vec<[u16; 3]>
}

fn format_hex_bytes(offset: usize, buf: &[u8]) -> String {
    let mut out = Vec::new();
    for (i, &b) in buf.iter().enumerate() {
        out.push(format!("{:02X} ", b));
//        if (offset + i + 1) % 16 == 0 {
//            out.push(" ".to_owned());
//        }
    }
    return out.drain(..).collect::<String>();
}

fn n2h(n: u8) -> char {
    match n {
        0 => '0',
        1 => '1',
        2 => '2',
        3 => '3',
        4 => '4',
        5 => '5',
        6 => '6',
        7 => '7',
        8 => '8',
        9 => '9',
        10 => 'A',
        11 => 'B',
        12 => 'C',
        13 => 'D',
        14 => 'E',
        15 => 'F',
        _ => panic!("expected a nibble, got: {}", n)
    }
}

fn b2h(b: u8, v: &mut Vec<char>) {
    v.push(n2h(b >> 4));
    v.push(n2h(b & 0xF));
}

#[derive(Debug, PartialEq, Eq)]
enum SectionKind {
    Main(u16),
    Sub(u8),
    Unknown,
    RelocatedCall(String),
    RelocatedRef,
    RelocationTarget
}

#[derive(Debug, PartialEq, Eq)]
struct Section {
    kind: SectionKind,
    offset: usize,
    length: usize,
}

impl Section {
    fn new(kind: u16, offset: usize, length: usize) -> Self {
        Section { kind: SectionKind::Main(kind), offset, length }
    }

    fn sub(kind: u8, offset: usize, length: usize) -> Self {
        Section { kind: SectionKind::Sub(kind), offset, length }
    }

    fn unknown(offset: usize, length: usize) -> Self {
        Section { kind: SectionKind::Unknown, offset, length }
    }

    fn color(&self) -> Color {
        match self.kind {
            SectionKind::Main(k) => {
                match k {
                    0xFFFF => Color::Blue,
                    0x00F0 => Color::Magenta,
                    0x00F2 => Color::Blue,
                    0x0046 => Color::Magenta,
                    0x004E => Color::Blue,
                    0x00EE => Color::Magenta,
                    0x00B2 => Color::Blue,
                    0x00DA => Color::Magenta,
                    0x00CA => Color::Blue,
                    0x0048 => Color::Magenta,
                    0x00B8 => Color::Blue,
                    0x0042 => Color::Yellow,
                    0x00E2 => Color::Yellow,
                    0x007A => Color::Blue,
                    0x00CE => Color::Magenta,
                    0x0078 => Color::Blue,
                    0x00C8 => Color::Magenta,
                    0x00A6 => Color::Blue,
                    0x00AC => Color::Magenta,
                    0x0082 => Color::Green,
                    _ => Color::Red,
                }
            },
            SectionKind::Sub(k) => {
                match k {
                    0xF6 => Color::Cyan,
                    0xBC => Color::Cyan,
                    0xFC => Color::Cyan, // Variable Length
                    0x6C => Color::Cyan,
                    0x06 => Color::BrightCyan,
                    _ => Color::Red,
                }
            },
            SectionKind::Unknown => Color::BrightBlack,
            _ => Color::Red,
        }
    }

    fn show(&self) -> bool {
        if let SectionKind::Unknown = self.kind {
            return true;
        }
        return false;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TagKind {
    RelocatedCall(String),
    RelocatedRef,
    RelocationTarget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Tag {
    kind: TagKind,
    offset: usize,
    length: usize,
}

impl Shape {
    fn read_name(n: &[u8]) -> Result<String> {
        let end_offset: usize = n.iter().position(|&c| c == 0).unwrap();
        return Ok(str::from_utf8(&n[..end_offset]).chain_err(|| "names should be utf8 encoded")?.to_owned());
    }

    pub fn new(path: &str, data: &[u8]) -> Result<(Vec<[f32; 3]>, String)> {
        let mut verts = Vec::new();

        let pe = peff::PE::parse(data).chain_err(|| "parse pe")?;
        let mut offset = 0;
        let mut cnt = 0;
        let show = false;
        let show_sub = false;
        let mut pics = Vec::new();
        let mut n_coords = 0;

        let mut sections = Vec::new();
        let mut tags = Vec::new();

        loop {
            let code: &[u16] = unsafe { mem::transmute(&pe.code[offset..]) };
            if code[0] == 0xFFFF {
                sections.push(Section::new(0xFFFF, offset, 14));
                offset += 14;
            } else if code[0] == 0x00F0 {
                break;
            } else if code[0] == 0x00F2 {
                sections.push(Section::new(0x00F2, offset, 4));
                offset += 4;
            } else if code[0] == 0x0046 {
                sections.push(Section::new(0x0046, offset, 2));
                offset += 2;
            } else if code[0] == 0x004E {
                sections.push(Section::new(0x004E, offset, 2));
                offset += 2;
            } else if code[0] == 0x00EE {
                sections.push(Section::new(0x00EE, offset, 2));
                offset += 2;
            } else if code[0] == 0x00B2 {
                sections.push(Section::new(0x00B2, offset, 2));
                offset += 2;
            } else if code[0] == 0x00DA {
                sections.push(Section::new(0x00DA, offset, 4));
                offset += 4;
            } else if code[0] == 0x00CA {
                sections.push(Section::new(0x00CA, offset, 4));
                offset += 4;
            } else if code[0] == 0x0048 {
                sections.push(Section::new(0x0048, offset, 4));
                offset += 4;
            } else if code[0] == 0x00B8 {
                sections.push(Section::new(0x00B8, offset, 4));
                offset += 4;
            } else if code[0] == 0x0042 {
                let s = Self::read_name(&pe.code[offset + 2..]).unwrap();
                sections.push(Section::new(0x0042, offset, s.len() + 3));
                offset += 2 + s.len() + 1;
            } else if code[0] == 0x00E2 {
                pics.push(Self::read_name(&pe.code[offset + 2..]).unwrap());
                sections.push(Section::new(0x00E2, offset, 16));
                offset += 16;
            } else if code[0] == 0x007A {
                sections.push(Section::new(0x007A, offset, 10));
                offset += 10;
            } else if code[0] == 0x00CE {
                //CE 00  00 5E  00 00 00 0D  00 00 00 11  00 00 00 00  00 AC FF FF  00 AC FF FF  00 22  00 00 00 54  00 00  00 AC FF FF  00 22  00 00
                sections.push(Section::new(0x00CE, offset, 40));
                offset += 40;
            } else if code[0] == 0x0078 {
                // 78 00 00 00 BC 01 82 00 90 01 00 00
                sections.push(Section::new(0x0078, offset, 12));
                offset += 12;
            } else if code[0] == 0x00C8 {
                // C8 00 E6 00 10 00 33 11
                // C8 00 E6 00 21 00 61 0F
                sections.push(Section::new(0x00C8, offset, 8));
                offset += 8;
            } else if code[0] == 0x00A6 {
                // A6 00 5B 0F 01 00
                sections.push(Section::new(0x00A6, offset, 6));
                offset += 6;
            } else if code[0] == 0x00AC {
                // AC 00 04 07
                sections.push(Section::new(0x00AC, offset, 4));
                offset += 4;
            } else if code[0] == 0x0082 {
                n_coords = code[1] as usize;
                let hdr_cnt= 2;
                let coord_sz= 6;
                let length = 2 + hdr_cnt * 2 + n_coords * coord_sz;
                if offset + length >= code.len() {
                    return Ok((verts, format!("FAILURE on {}", path)));
                }
                sections.push(Section ::new(0x0082, offset, length));
                offset += 2 + hdr_cnt * 2;
                fn s2f(s: u16) -> f32 { (s as i16) as f32 }
                for i in 0..n_coords {
                    let x = s2f(code[offset + 0]);
                    let y = s2f(code[offset + 1]);
                    let z = s2f(code[offset + 2]);
                    verts.push([x, y, z]);
                    offset += coord_sz;
                }

                // switch to a second vm after verts that works per-byte.
                loop {
                    let code2 = &pe.code[offset..];
                    if code2[0] == 0xF6 {
                        sections.push(Section ::sub(0xF6, offset, 7));
                        offset += 7;
                    } else if code2[0] == 0xBC {
                        // BC 9E 08 00 08 00
                        sections.push(Section ::sub(0xBC, offset, 6));
                        offset += 6;
                    } else if code2[0] == 0xFC {
                        let unk0 = code2[1];
                        let flags = code2[2];
                        let i = if (flags & 2) == 0 { 0x11 } else { 0x0e } as usize;
                        let index_count = code2[i] as usize;
                        let have_shorts = (flags & 1) != 0;
                        let mut length = i + 1 + index_count;
                        if have_shorts {
                            length += index_count * 2;
                        }
                        sections.push(Section ::sub(0xFC, offset, length));
                        offset += length;
                    } else if code2[0] == 0x6C {
                        // 6C 00 06 00 00 00 05 00 36 06 38 9B 06
                        sections.push(Section::sub(0x6C, offset, 13));
                        offset += 13;
                    } else if code2[0] == 0x06 {
                        // 06 00 5A 45 FD FF 7E 6B FF FF E6 FB F9 FF 05 00 07 03 38 53 03
                        sections.push(Section::sub(0x06, offset, 21));
                        offset += 21;
                    } else {
                        break;
                    }
                }
                // We need to get back into word mode somehow.
                // 06 00 00 93 03 00 46 EC 07 00 1E 40 CA FF 05 00 6E 29 38 99 29
                //break;
            } else {
                break;
            }
            cnt += 1;
        }

        sections.push(Section::unknown(offset, cmp::min(1024, pe.code.len() - offset)));

        for &reloc in pe.relocs.iter() {
            assert!((reloc as usize) + 4 <= pe.code.len());
            let dwords: &[u32] = unsafe { mem::transmute(&pe.code[reloc as usize..]) };
            let thunk_ptr = dwords[0];
            if let Some(thunks) = pe.thunks.clone() {
                if thunks.contains_key(&thunk_ptr) || thunks.contains_key(&(thunk_ptr - 2)){
                    // This relocation is for a pointer into the thunk table; store the name so
                    // that we can print the name instead of the address.
                    // println!("Relocating {:X} in code to {}", thunk_ptr, &thunks[&thunk_ptr].name);
                    tags.push(Tag {kind: TagKind::RelocatedCall(thunks[&thunk_ptr].name.clone()), offset: reloc as usize, length: 4});
                } else {
                    // This relocation is to somewhere in code; mark both it and the target word
                    // of the pointer that is stored at the reloc position.
                    tags.push(Tag {kind: TagKind::RelocatedRef, offset: reloc as usize, length: 4});

                    assert!(thunk_ptr > pe.code_vaddr, "thunked ptr before code");
                    assert!(thunk_ptr <= pe.code_vaddr + pe.code.len() as u32 - 4, "thunked ptr after code");
                    let code_offset = thunk_ptr - pe.code_vaddr;
                    let value_to_relocate_arr: &[u16] = unsafe { mem::transmute(&pe.code[code_offset as usize..]) };
                    let value_to_relocate = value_to_relocate_arr[0];
                    //println!("Relocating {:X} at offset {:X}", value_to_relocate, code_offset);
                    tags.push(Tag {kind: TagKind::RelocationTarget, offset: code_offset as usize, length: 2});
                }
            }
        }

        let mut out = format_sections(&pe.code, &sections, &mut tags);
//        for (key, value) in pe.thunks.unwrap().iter() {
//            out += &format!("\n  {:X} <- {:?}", key, value);
//        }
        return Ok((verts, out + " - " + path));
    }
}

fn format_sections(code: &[u8], sections: &Vec<Section>, tags: &mut Vec<Tag>) -> String {
    // Assert that sections tightly abut.
    let mut next_offset = 0;
    for section in sections {
        assert_eq!(section.offset, next_offset);
        next_offset = section.offset + section.length;
    }

    // Assert that there are no tags overlapping.
    tags.sort_by(|a, b| { a.offset.cmp(&b.offset) });
    tags.dedup();
    for (i, tag_a) in tags.iter().enumerate() {
        for (j, tag_b) in tags.iter().enumerate() {
            if j > i {
                // println!("{:?}@{}+{}; {:?}@{}+{}", tag_a.kind, tag_a.offset, tag_a.length, tag_b.kind, tag_b.offset, tag_b.length);
                assert!(tag_a.offset <= tag_b.offset);
                assert!(tag_a.offset + tag_a.length <= tag_b.offset ||
                        tag_a.offset + tag_a.length >= tag_b.offset + tag_b.length);
            }
        }
    }

    let mut acc: Vec<char> = Vec::new();
    for section in sections {
        accumulate_section(code, section, tags, &mut acc);
    }
    return acc.iter().collect::<String>();
}

fn tgt<'a>(x: &'a mut Vec<char>, y: &'a mut Vec<char>) -> &'a mut Vec<char> {
    if true {
        return x;
    }
    return y;
}

fn accumulate_section(code: &[u8], section: &Section, tags: &Vec<Tag>, v: &mut Vec<char>) {
    if !section.show() {
        return;
    }

    let mut nul = Vec::new();
    let n = &mut nul;

    let section_tags = find_tags_in_section(section, tags);
    if let Some(t) = section_tags.first() {
        if t.offset == section.offset {
            Escape::new().underline().put(tgt(v, n));
        }
    }

    Escape::new().bg(section.color()).put(tgt(v, n));
    b2h(code[section.offset + 0], v);
    v.push(' ');
    b2h(code[section.offset + 1], v);
    Escape::new().put(tgt(v, n));
    Escape::new().fg(section.color()).put(tgt(v, n));
    let mut off = section.offset + 2;
    for &b in &code[section.offset + 2..section.offset + section.length] {
        // Push an tag closers.
        for tag in section_tags.iter() {
            if tag.offset + tag.length == off {
                if let &TagKind::RelocatedCall(ref target) = &tag.kind {
                    Escape::new().put(tgt(v, n));
                    v.push('(');
                    Escape::new().fg(Color::Red).put(tgt(v, n));
                    for c in target.chars() {
                        v.push(c)
                    }
                    Escape::new().put(tgt(v, n));
                    v.push(')');
                    v.push(' ');
                }
                Escape::new().put(tgt(v, n));
                Escape::new().fg(section.color()).put(tgt(v, n));
            }
        }
        v.push(' ');
        // Push any tag openers.
        for tag in section_tags.iter() {
            if tag.offset == off {
                match &tag.kind {
                    &TagKind::RelocatedCall(_) => Escape::new().dimmed().put(tgt(v, n)),
                    &TagKind::RelocatedRef => Escape::new().bg(Color::BrightRed).bold().put(tgt(v, n)),
                    &TagKind::RelocationTarget => Escape::new().fg(Color::BrightMagenta).strike_through().put(tgt(v, n)),
                };
            }
        }
        b2h(b, v);
        off += 1;
    }
    Escape::new().put(tgt(v, n));
    v.push(' ');
}

fn find_tags_in_section(section: &Section, tags: &Vec<Tag>) -> Vec<Tag> {
    return tags.iter()
        .filter(|t| { t.offset >= section.offset && t.offset < section.offset + section.length })
        .map(|t| { t.to_owned() })
        .collect::<Vec<Tag>>();
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::prelude::*;
    use super::*;

    #[test]
    fn it_works() {
        let mut rv: Vec<String> = Vec::new();
        let paths = fs::read_dir("./test_data").unwrap();
        for i in paths {
            let entry = i.unwrap();
            let path = format!("{}", entry.path().display());
            //println!("AT: {}", path);

            //if path == "./test_data/MIG21.SH" {
            if true {

                let mut fp = fs::File::open(entry.path()).unwrap();
                let mut data = Vec::new();
                fp.read_to_end(&mut data).unwrap();

                let (_verts, desc) = Shape::new(&path, &data).unwrap();
                rv.push(desc);

            }

            //assert_eq!(format!("./test_data/{}", t.object.file_name), path);
            //rv.push(format!("{:?} <> {} <> {}",
            //                t.object.unk_explosion_type,
            //                t.object.long_name, path));
        }
        rv.sort();

        let mut set = HashSet::new();
        for v in rv {
            set.insert((&v[0..2]).to_owned());
            println!("{}", v);
        }
        println!("{:?}", set);
    }
}
