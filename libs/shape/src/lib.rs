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
use ansi::{Span, Color};

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
    v.push(' ');
}

fn hex(buf: &[u8], v: &mut Vec<char>) {
    for &b in buf {
        b2h(b, v);
    }
}

enum SectionKind {
    Main(u16),
    Sub(u8),
    Unknown,
    RelocatedCall(String),
    RelocatedRef,
    RelocationTarget(usize)
}

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

    fn relocated_call(to: &str, offset: usize) -> Self {
        Section { kind: SectionKind::RelocatedCall(to.to_owned()), offset, length: 4 }
    }

    fn relocated_ref(offset: usize) -> Self {
        Section { kind: SectionKind::RelocatedRef, offset, length: 4 }
    }
}

impl Shape {
    fn read_name(n: &[u8]) -> Result<String> {
        let end_offset: usize = n.iter().position(|&c| c == 0).unwrap();
        return Ok(str::from_utf8(&n[..end_offset]).chain_err(|| "names should be utf8 encoded")?.to_owned());
    }

    pub fn code(buf: &[u8], offset: usize, len: usize, c: Color) -> String {
        let rng = &buf[offset..(offset + len)];
        let hd = Span::new(&format_hex_bytes(offset, &rng[0..2])).background(c);
        let rm = Span::new(&format_hex_bytes(offset + 2, &rng[2..])).foreground(c);
        return format!("{}{}", hd.format(), rm.format());
    }

    pub fn code_ellipsize(buf: &[u8], offset: usize, len: usize, cut_to: usize, c: Color) -> String {
        assert!(len > cut_to);
        assert!(cut_to > 4);
        assert_eq!(cut_to % 2, 0);
        let h = cut_to / 2;
        let rngl = &buf[offset..offset + h];
        let rngr = &buf[offset + len - h..offset + len];
        let hd = Span::new(&format_hex_bytes(offset, &rngl[0..2])).background(c);
        let l = Span::new(&format_hex_bytes(0, &rngl[2..])).foreground(c);
        let r = Span::new(&format_hex_bytes(0, rngr)).foreground(c);
        return format!("{}{}{}", hd.format(), l.format(), r.format());
    }

    pub fn data(buf: &[u8], offset: usize, len: usize, c: Color) -> String {
        let rng = &buf[offset..(offset + len)];
        let rm = Span::new(&format_hex_bytes(offset, &rng[0..])).foreground(c);
        return rm.format();
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
        let mut unk = 0;

        let mut sections = Vec::new();

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
                unk = 1;
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
                        sections.push(Section ::sub(0x6C, offset, 13));
                        offset += 13;
                    } else {
                        unk = 2;
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

        sections.push(Section::unknown(offset, pe.code.len() - offset));

        for &reloc in pe.relocs.iter() {
            assert!((reloc as usize) + 4 <= pe.code.len());
            let dwords: &[u32] = unsafe { mem::transmute(&pe.code[reloc as usize..]) };
            let thunk_id = dwords[0];
            if let Some(thunks) = pe.thunks.clone() {
                if thunks.contains_key(&thunk_id) {
                    sections.push(Section::relocated_call(&thunks[&thunk_id].name, reloc as usize));
                } else {
                    sections.push(Section::relocated_ref(reloc as usize));
                }
            }
        }
//        let remainder = cmp::min(1500, pe.code.len() - offset);
//        out += &format_hex_bytes(offset, &pe.code[offset..offset+remainder]);
        //                let buffer = &pe.code[offset..offset + remainder];
        //                let fmt = format_hex_bytes(offset, buffer);

        let mut num_thunk = 0;
        if let Some(thunks) = pe.thunks.clone() {
            num_thunk = thunks.len();
        }

        let out = format_sections(&pe.code, &mut sections);
        return Ok((verts, format!("{:04X}| {} => {:?}", unk, out, pe.thunks)));
    }
}

fn format_sections(code: &[u8], sections: &mut Vec<Section>) -> String {
    sections.sort_by(|a, b| { a.offset.cmp(&b.offset) });
    for (i, section_a) in sections.iter().enumerate() {
        for (j, section_b) in sections.iter().enumerate() {
            if j > i {
                assert!(section_a.offset < section_b.offset);
                assert!(section_a.offset + section_a.length <= section_b.offset ||
                        section_a.offset + section_a.length >= section_b.offset + section_b.length);
            }
        }
    }

    let mut acc = Vec::new();
    for i in 0..sections.len() {
        let start = sections[i].offset;
        let j = i + 1;
        if j == sections.len() || sections[i].offset + sections[i].length <= sections[j].offset {
            hex(&code[start..start + 2], &mut acc);
            hex(&code[start + 2..start + sections[i].length], &mut acc);
        } else {

        }
    }

    return acc.iter().collect::<String>();
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
