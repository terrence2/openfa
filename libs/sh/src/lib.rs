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
extern crate bitflags;
#[macro_use]
extern crate error_chain;
extern crate peff;
extern crate reverse;

mod errors {
    error_chain!{}
}
use errors::{Error, ErrorKind, Result, ResultExt};

use std::path::{Path, PathBuf};
use std::io::prelude::*;
use std::{cmp, fs, mem, str};
use std::collections::{HashMap, HashSet};
use reverse::{b2h, Escape, Color};

pub struct Shape {
    pub meshes: Vec<Mesh>,
    pub source: String
}

impl Shape {
    fn empty() -> Shape {
        Shape {
            meshes: Vec::new(),
            source: "".to_owned(),
        }
    }
}

pub struct Mesh {
    pub vertices: Vec<[f32; 3]>,
    pub facets: Vec<Facet>,
}

impl Mesh {
    fn empty() -> Mesh {
        Mesh {
            vertices: Vec::new(),
            facets: Vec::new()
        }
    }
}

#[derive(Debug)]
pub struct Facet {
    pub flags: FacetFlags,
    pub indices: Vec<u16>,
    //pub texcoords: Vec<[f32; 2]>,
}

fn b2b(b: u8, v: &mut Vec<char>) {
    for i in 0..8 {
        if i == 4 {
            v.push('_');
        }
        let off = -(i - 7);
        if (b >> off) & 0b1 == 1 {
            v.push('1');
        } else {
            v.push('0');
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum SectionKind {
    Main(u16),
    Unknown,
    Invalid,
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

    fn unknown(offset: usize, length: usize) -> Self {
        Section { kind: SectionKind::Unknown, offset, length }
    }

    fn color(&self) -> Color {
        match self.kind {
            SectionKind::Main(k) => {
                match k {
                    0xFFFF => Color::Blue,
                    0x00F0 => Color::BrightGreen,
                    0x00F2 => Color::Blue,
                    0x00DA => Color::Magenta,
                    0x00CA => Color::Blue,
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
                    0x1E1E => Color::Red,
                    0x00FC => Color::Cyan,
                    _ => Color::Red,
                }
            },
            SectionKind::Unknown => Color::BrightBlack,
            _ => Color::Red,
        }
    }

    fn show(&self) -> bool {
        return true;
        if let SectionKind::Unknown = self.kind {
            return true;
        }
        return false;
    }
}

bitflags! {
    pub struct FacetFlags : u16 {
        const HAVE_MATERIAL      = 0b0100_0000_0000_0000;
        const HAVE_TEXCOORDS     = 0b0000_0100_0000_0000;
        const USE_SHORT_INDICES  = 0b0000_0000_0000_0100;
        const USE_SHORT_MATERIAL = 0b0000_0000_0000_0010;
        const USE_BYTE_TEXCOORDS = 0b0000_0000_0000_0001;
    }
}

impl FacetFlags {
    fn from_u16(flags: u16) -> FacetFlags {
        unsafe { mem::transmute(flags) }
    }

    pub fn to_u16(&self) -> u16 {
        unsafe { mem::transmute(*self) }
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

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ShowMode {
    AllOneLine,
    AllPerLine,
    UnknownFacet,
    UnknownMinus,
    Unknown,
    Custom,
}

impl Shape {
    fn read_name(n: &[u8]) -> Result<String> {
        let end_offset: usize = n.iter().position(|&c| c == 0).chain_err(|| "no terminator")?;
        return Ok(str::from_utf8(&n[..end_offset]).chain_err(|| "names should be utf8 encoded")?.to_owned());
    }

    pub fn new(data: &[u8], path: &str, mode: ShowMode) -> Result<(Shape, Vec<String>)> {
        let pe = peff::PE::parse(data).chain_err(|| "parse pe")?;

        let (shape, sections) = Self::_read_sections(&pe, path).chain_err(|| "read sections")?;
        if mode == ShowMode::UnknownMinus && sections.last().unwrap().kind != SectionKind::Unknown && sections.last().unwrap().kind != SectionKind::Invalid {
            return Ok((shape, vec!["".to_owned()]));
        }

        let mut tags = Self::_apply_tags(&pe, &sections).chain_err(|| "apply tags")?;

        let mut out = format_sections(&pe.code, &sections, &mut tags, mode);
        let mut out2 = out.drain(..).map(|v| v + &format!(" - {}", path)).collect::<Vec<String>>();
        //out.push(path.to_owned());
//        for (key, value) in pe.thunks.unwrap().iter() {
//            out += &format!("\n  {:X} <- {:?}", key, value);
//        }
        return Ok((shape, out2));
    }

    fn _read_sections(pe: &peff::PE, path: &str) -> Result<(Shape, Vec<Section>)> {
        let mut shape = Shape::empty();

        let mut offset = 0;
        let mut cnt = 0;
        let mut current_pic = None;
        let mut n_coords = 0;

        let mut sections = Vec::new();

        loop {
            if offset >= pe.code.len() { break; }
            let code: &[u16] = unsafe { mem::transmute(&pe.code[offset..]) };
            //println!("offset: {} => {:4X}", offset, code[0]);
            if code[0] == 0xFFFF {
                sections.push(Section::new(0xFFFF, offset, 14));
                offset += 14;
            } else if pe.code.len() - offset == 22 {
                // FF 26 00 F5 FF 10 00 D0 FF 23 00 00 00 01 02 03 02 01 02 03 02 01
                sections.push(Section::new(0x00FF, offset, 22));
                offset += 22;
            } else if code[0] == 0x00F2 {
                sections.push(Section::new(0x00F2, offset, 4));
                offset += 4;
            } else if code[0] == 0x0046 {
                sections.push(Section::new(0x0046, offset, 2));
                offset += 2;
            } else if code[0] == 0x00B2 {
                sections.push(Section::new(0x00B2, offset, 2));
                offset += 2;
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
                shape.source = Self::read_name(&pe.code[offset + 2..]).chain_err(|| "read name")?;
                sections.push(Section::new(0x0042, offset, shape.source.len() + 3));
                offset += 2 + shape.source.len() + 1;
            } else if code[0] == 0x00E2 {
                current_pic = Some(Self::read_name(&pe.code[offset + 2..]).chain_err(|| "read name")?);
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
                // Even though 82 adds more verts to the shape's pile of verts, it also
                // indicates a new independent(?) collections of facets.
                shape.meshes.push(Mesh::empty());
                n_coords = code[1] as usize;
                let hdr_cnt = 2;
                let coord_sz = 6;
                let length = 2 + hdr_cnt * 2 + n_coords * 6;
                sections.push(Section::new(0x0082, offset, length));
                fn s2f(s: u16) -> f32 { (s as i16) as f32 }
                for i in 0..n_coords {
                    let x = s2f(code[3 + i * 3 + 0]);
                    let y = s2f(code[3 + i * 3 + 1]);
                    let z = s2f(code[3 + i * 3 + 2]);
                    shape.meshes.last_mut().unwrap().vertices.push([x, y, z]);
                }
                offset += length;
//            } else if code[0] == 0x001E {
//                sections.push(Section::new(0x001E, offset, 6));
//                offset += 6;
            } else if code[0] & 0xFF == 0x1E {
                sections.push(Section::new(0x1E1E, offset, 1));
                offset += 1;
            } else if code[0] & 0xFF == 0xFC {
                /*
                // FC 0b0000_0000_0010_0100  00 FC                                       00
                // FC 0b0000_0001_0000_0000  9F 00                                       04   16 16 22 22
                // FC 0b0000_0001_0000_0010  9F 00                                       04   19 19 2A 2A
                // FC 0b0000_0100_0000_0011  00 00                                       03   26 29 26                B8   B4    C3   CA    B9   B3
                // FC 0b0100_0001_0000_0000  6D 00 11 08 91 7F 93 06 1D 00 FB FF 30 01   03   02 01 0B
                // FC 0b0100_0001_0000_0010  5B 00 00 00 00 00 FD 7F 02 FE F3            04   13 24 23 05
                // FC 0b0100_0001_0000_0110  9F 00 00 00 BE 0E 23 7F 00 0A 1E            04   4D00 4000 0701 0801
                // FC 0b0100_0001_0000_1010  A0 00 D1 7F A8 06 00 00 E3 FB 00            03   03 0A 09
                // FC 0b0100_0100_0000_0000  00 00 9F B9 E7 6A 00 00 5E FF F4 FF C5 FF   04   0E 0B 0A 0F           0000 2101  0000 4A01  8E00 4A01  8E00 2101
                // FC 0b0100_0100_0000_0000  00 00 B4 93 F1 C4 21 22 CB FF 16 00 E9 00   03   00 01 02              4C00 6C02  4C00 8202  0000 6C02
                // FC 0b0100_0100_0000_0001  00 00 00 00 00 00 03 80 00 00 DA FF 66 FF   04   00 01 02 03             04   07    2E   2D    54   2D    7E   07
                // FC 0b0100_0100_0000_0010  00 00 00 00 00 00 03 80 01 FA D9            04   06 07 08 09           0C00 E500  0C00 2A01  5B00 2A01  5A00 E500
                // FC 0b0100_0100_0000_0011  00 00 00 00 00 00 03 80 00 02 FA            04   04 06 07 05             C0   39    F6   39    F6   22    C0   22
                // FC 0b0100_0100_0000_0100  00 00 03 80 00 00 00 00 AF 00 C7 FF 2D 01   04   FE00 FF00 0001 0101   0400 2C01  4C00 2C01  4C00 BB00  0400 BB00
                // FC 0b0100_0100_0000_0101  00 00 00 00 FD 7F 00 00 DB 00 DC FF 2D 01   04   0701 0601 0301 0201     61   80    61   38    04   38    04   80
                // FC 0b0100_0100_0000_0111  00 00 00 00 3F 77 7E 2E FD 07 30            04   0101 0201 FA00 FF00    46 82 46 67 00 66 00 84
                // FC 0b0100_0100_0000_1001  00 00 00 00 FD 7F 00 00 CB FE 00 00 6A 02   04   6E 20 6D 6F             00   D2    76   D2    76   5C    00   5C
                // FC 0b0100_0100_0000_1011  00 00 00 00 62 73 9E C8 00 07 F7            04   03 06 07 00             35   1C    35   02    00   02    00   1C
                // FC 0b0100_1001_0000_0000  5D 00 00 00 00 00 FD 7F 91 00 D4 FF A4 00   04   00 04 07 05
                // FC 0b0100_1001_0000_0010  52 00 0A 60 F8 AF 70 1B 01 FF 1A            04   04 06 02 01
                // FC 0b0100_1100_0000_0000  00 00 00 00 00 00 FD 7F D9 FE BF FF 67 00   04   1B 1A 19 1C           0000 0C04  8C00 0C04  8C00 9B03  0000 9B03
                // FC 0b0100_1100_0000_0001  00 00 00 00 00 00 03 80 7F 00 7C FF 97 FF   04   7A 72 79 82             A6   7B    F0   7C    F0   BC    A6   BC
                // FC 0b0100_1100_0000_0010  00 00 00 00 00 00 03 80 01 E2 AA            04   04 02 01 05           0400 A601  8000 A601  8000 4901  0400 4901
                // FC 0b0100_1100_0000_0011  00 00 00 00 00 00 03 80 00 01 01            04   3B 3A 3D 3C             5B   05    34   05    34   23    5B   23
                // FC 0b0100_1100_0000_0110  6B 00 3E 70 00 00 81 3D 09 F7 32            04   0301 EB00 EA00 0201   2900 7F01 2900 8801 2100 8801 2100 7F01
                // FC 0b0100_1100_0000_0111  00 00 00 00 FD 7F 00 00 00 13 26            04   7200 0001 0101 7300   C2 E5 C2 FF FC FF FC E6
                // FC 0b0100_1100_0000_1011  00 00 00 00 7A 72 3D 39 00 09 09            04   03 02 01 00           00 24 00 3C 35 3C 35 24
                // FC 0b0101_0001_0000_0000  90 00 00 00 FD 7F 00 00 85 02 FF FF 7D FD   04   DF E0 E1 E2
                // FC 0b0101_0001_0000_0010  90 00 00 00 FD 7F 00 00 77 00 6A            04   99 9A 9B 9C
                // FC 0b0101_0001_0000_0100  AF 00 00 00 FD 7F 00 00 55 00 00 00 D7 01   04   FD00 FE00 FF00 0001
                // FC 0b0101_0001_0000_1000  44 00 D0 9D B6 3B 5A 38 4D 00 1A 00 61 FF   04   03 05 06 00
                // FC 0b0101_0001_0000_1010  91 00 22 E2 74 7C 00 00 FF 18 00            04   04 05 01 00
                // FC 0b0101_0001_0000_1100  92 00 F9 7F 0D 02 00 00 CD FF 3E 00 71 FF   04   1601 1701 1801 1101
                // FC 0b0101_0100_0000_0000  00 00 00 00 FD 7F 00 00 19 02 00 00 5C 00   04   43 47 48 44           9500 0202  9500 6F01  0300 7001  0300 0202
                // FC 0b0101_0100_0000_0001  00 00 00 00 FD 7F 00 00 30 02 00 00 CD FE   04   6C 6D 6B 6A             FA   75    97   75    97   D6    FA   D7
                // FC 0b0101_0100_0000_0010  00 00 00 00 FD 7F 00 00 C3 FF 09            04   0F 10 11 12           FC00 5A01 9900 5B01 9900 BC01 FC00 BC01
                // FC 0b0101_0100_0000_0011  00 00 00 00 FD 7F 00 00 A1 00 4E            04   32 33 31 30           3A 30 00 32 00 AA 3A AA
                // FC 0b0101_0100_0000_0111  00 00 FD 7F 00 00 00 00 30 67 DF            04   1D01 1E01 1F01 2001   2F 29 1D 2A 1D 1E 2F 1E
                // FC 0b0101_0100_0000_1000  00 00 00 00 FD 7F 00 00 39 00 18 00 77 FF   05   0A 5C 59 68 3F        3E00 5B01  0C00 7601  0000 9701  0D00 9701  5E00 9701
                // FC 0b0101_0100_0000_1001  00 00 00 00 00 00 FD 7F C9 FF B7 FF 4E 01   04   00 01 02 03             DE   03    DE   1B    F4   1B    F4   03
                // FC 0b0101_0100_0000_1010  00 00 31 80 3D F9 00 00 B4 84 81            04   3D 3C 38 37           0000 5101  0000 8501  7800 8501  8A00 5101
                // FC 0b0101_0100_0000_1011  00 00 00 00 FD 7F 00 00 00 EE 8F            04   0C 0D 02 01             CB   4F    99   50    99   97    CB   97
                // FC 0b0101_0100_0000_1100  00 00 59 80 FF F6 37 02 B6 FF 84 FF 0C 01   04   3A00 2A01 2B01 3B00   7E00 1C01 0000 1C01 0000 5001 7E00 5001
                // FC 0b0101_0100_0000_1101  00 00 1C EE 45 81 00 00 AB FF 38 00 71 FF   04   0301 0201 0401 0501   7D 78 88 78 88 54 7D 54
                // FC 0b0101_1001_0000_0000  A0 00 00 00 66 82 9B 18 00 00 F0 FF 21 FF   04   05 09 0D 0E
                // FC 0b0101_1001_0000_0010  A0 00 00 00 33 80 F2 06 00 F8 87            04   26 2D 38 33
                // FC 0b0101_1001_0000_0100  90 00 00 00 FD 7F 00 00 21 FF FF FF 64 03   04   0501 0601 0701 0801
                // FC 0b0101_1001_0000_0110  9E 00 00 00 03 80 00 00 1C 7F D6            04   0901 0A01 0B01 0C01
                // FC 0b0101_1001_0000_1000  5D 00 00 00 00 00 FD 7F 00 00 E4 FF AC 00   04   03 06 09 04
                // FC 0b0101_1001_0000_1010  59 00 69 02 F7 7F B7 00 21 E4 56            05   09 0A 0B 0C 0D        1E FC 59 0A 59 00 88 FD F7 7F
                // FC 0b0101_1100_0000_0000  00 00 00 00 FD 7F 00 00 41 03 00 00 80 FD   04   5A 5B 57 56           6300 0A01  0000 0A01  0000 7701  6300 7701
                // FC 0b0101_1100_0000_0001  00 00 00 00 9E 7D 7B E7 00 00 F5 FF 30 FF   04   00 01 02 03             01   17    2B   19    2B   02    01   03
                // FC 0b0101_1100_0000_0010  00 00 00 00 50 6B C1 45 00 F4 EC            04   39 38 3B 3A           3D00 1101  0000 1101  0000 2C01  3D00 2C01
                // FC 0b0101_1100_0000_0011  00 00 FD 7F 00 00 00 00 0B F7 CE            04   17 00 0D 1F             93   64    93   93    FB   93    FB   64
                // FC 0b0101_1100_0000_0100  00 00 00 00 00 00 FD 7F 14 FD 07 00 91 03   04   0201 0301 0401 0501   FE00 EC00 EE00 EC00 EE00 0C01 FE00 0C01
                // FC 0b0101_1100_0000_0101  00 00 00 00 00 00 FD 7F 72 FF 0B 00 1D 00   04   1601 1501 1801 1701   F0 12 E0 12 E0 32 F0 32
                // FC 0b0101_1100_0000_0110  00 00 00 00 00 00 03 80 1F 78 DC            04   1601 1501 1401 1301   F600 2402 F600 6202 DB00 6202 DB00 2402
                // FC 0b0101_1100_0000_0111  00 00 00 00 FD 7F 00 00 1F 6B CE            04   1901 1C01 1B01 1A01   01 2B 01 39 26 39 26 2B
                // FC 0b0101_1100_0000_1000  00 00 00 00 00 00 FD 7F 00 00 B8 FF BD 00   04   00 01 02 03           9600 2A01 F500 2A01 F500 B000 9600 B000
                // FC 0b0101_1100_0000_1001  00 00 00 00 00 00 03 80 00 00 B8 FF 46 FF   04   04 05 06 07             96   AF    F5   AF    F5   35    96   35
                // FC 0b0101_1100_0000_1010  00 00 03 80 00 00 00 00 AB E4 AA            04   07 06 0A 0B           0000 FA02 8900 FA02 8900 8002 0000 8002
                // FC 0b0101_1100_0000_1011  00 00 00 00 FD 7F 00 00 00 2E A6            04   0A 08 01 00           00 02 00 69 6E 69 6E 02
                // FC 0b0101_1100_0000_1100  00 00 A5 55 00 00 1C 5F A0 FF 6F FF 6D 00   04   0201 0301 0401 0501   C100 6801 F600 A401 ED00 A801 BE00 7301
                // FC 0b0101_1100_0000_1101  00 00 5B AA 00 00 1C 5F 60 00 6F FF 6D 00   04   FE00 FF00 0001 0101   AD 96 AA A1 D9 D6 E2 D2
                // FC 0b0101_1100_0000_1111  00 00 00 00 00 00 03 80 02 F3 2D            04   0001 FF00 FE00 FD00   95 31 95 38 60 38 60 31
                // FC 0b0110_0001_0000_0000  96 00 DC 01 DD FD F5 7F F8 FF 0E 00 77 FF   04   18 19 1A 1B
                // FC 0b0110_0001_0000_0010  44 00 75 54 8A 5F 09 F5 06 FD F5            03   09 0A 08
                // FC 0b0110_0001_0000_0110  6E 00 00 00 FD 7F 00 00 F7 FD 01            04   0501 0401 0301 0201
                // FC 0b0110_0011_0000_0000  6D 00 00 00 0D 80 C9 FC 50 FF F0 FF 18 00   04   0A 09 06 07
                // FC 0b0110_0011_0000_0010  78 00 00 00 2D 71 3C C4 FC 12 08            04   19 10 0F 14
                // FC 0b0110_0011_0000_0110  96 00 FD 7F 00 00 00 00 00 FF FD            04   2601 0800 0700 0600
                // FC 0b0110_0100_0000_0010  00 00 25 80 F0 04 D8 FC FE 01 E9            04   00 01 02 03           B100 0801 C100 1101 D500 0901 B700 F800
                // FC 0b0110_0100_0000_0011  00 00 00 00 03 80 00 00 0D 0A D9            04   0B 29 28 27           CB 34 9A 33 97 25 C9 1C
                // FC 0b0110_1100_0000_0001  00 00 00 00 00 00 FD 7F 02 00 04 00 5D FF   04   2D 2C 2B 2A           33 17 33 2C 00 2C 00 17
                // FC 0b0110_1100_0000_0010  00 00 03 80 00 00 00 00 02 09 C7            04   1D 1C 20 1E           D500 FA00 D500 0A01 5600 0B01 5600 FA00    # 12 00 03 00 38 32 00
                // FC 0b0110_1100_0000_0011  00 00 00 00 03 80 00 00 09 01 02            04   0b 0a 09 08           52 55 52 35 9a 36 9a 55
                // FC 0b1100_1101_0000_0010  6B 00 FD 7F CD FF D9 FF 00 0A A5            04   A8 A9 8E AA           DC00 7101 C200 7601 9B00 3401 BB00 2801
                // FC 0b1100_1101_0000_0011  6E 00 00 00 FD 7F 00 00 00 07 F8            04   00 01 02 03           EB A7 D9 A7 D9 E8 EB EA
                // FC 0b1100_1101_0000_0111  6C 00 00 00 03 80 00 00 00 F5 32            04   0201 FC00 F900 0301   AF D8 AF 89 A9 7F A8 E4
                // FC 0b1101_1101_0000_1000  9E 00 00 00 00 00 03 80 A1 00 5A FF 36 FF   04   12 13 14 15           0000 0101 3B00 0101 3B00 5C01 0000 5C01
                // FC 0b1101_1101_0000_1001  98 00 D6 75 BC 0E BD 2F 0E 00 59 00 0A FF   04   A1 A0 A2 A3           10 4D 10 69 0C 69 08 4D
                // FC 0b1101_1101_0000_1010  9B 00 03 80 00 00 00 00 F8 10 FB            05   29 1B 17 16 2A        8600 1501 8600 1E01 8600 2701 AA00 2701 AD00 1501
                // FC 0b1101_1101_0000_1011  96 00 74 00 FD 7F FE FF 11 EE D1            05   0D 0E 0F 07 06        00 62  07 62  07 35  00 1C  00 49
                // FC 0b1101_1101_0000_1100  9F 00 03 80 00 00 00 00 B9 FF 5A FF 56 00   04   F900 F800 0001 0101   E200 0C01 E200 6701 BE00 6701 BE00 0C01
                // FC 0b1101_1101_0000_1101  9F 00 03 80 00 00 00 00 28 00 5A FF 56 00   04   FB00 FA00 0201 0301   24 4E 24 A9 00 A9 00 4E
                // FC 0b1110_1101_0000_0010  5A 00 7F 00 F1 7F 93 FC 03 01 F8            04   03 04 05 00           EC00 DB00 DC00 D900 DC00 FD00 ED00 0001
                // FC 0b1110_1101_0000_0011  44 00 00 00 0F 80 97 FC 10 FB 18            03   03 02 04              CB 08 CB 70 B9 08
                // FC 0b1110_1110_0000_0010  94 00 32 A9 0A 5E A3 01 FB 06 F1            03   0B 69 68              4700 6D01 D700 7101 D700 6401
                // FC 0b1110_1110_0000_0011  68 00 80 A5 80 A5 00 00 25 FF 01            04   7D 02 06 7E           5E 0A 61 0A 61 24 5E 24
                // FC 0b1110_1110_0000_0110  98 00 03 E1 78 7B BD F2 FC 09 FB            03   0300 B800 1201        AA00 C101 C100 8D01 A900 4501
                // FC 0b1110_1110_0000_0111  98 00 47 2B 1C 88 69 F4 0D F8 C5            03   FD00 4200 0301        7A B3 7A A8 37 AC 1E
                */
                let flags_word = (code[0] & 0xFF00) | (code[1] & 0x00FF);
                //let flags_word = ((code2[1] as u16) << 8) | (code2[2] as u16);
                assert_eq!(flags_word & 0x00F0, 0u16);
                let flags = FacetFlags::from_u16(flags_word);
                let material_size = if flags.contains(FacetFlags::HAVE_MATERIAL){
                    if flags.contains(FacetFlags::USE_SHORT_MATERIAL) { 11 } else { 14 }
                } else {
                    2
                };
                let index_size = if flags.contains(FacetFlags::USE_SHORT_INDICES) { 2 } else { 1 };
                let have_tc = flags.contains(FacetFlags::HAVE_TEXCOORDS);
                let tc_size = if flags.contains(FacetFlags::USE_BYTE_TEXCOORDS) { 1 } else { 2 };

                let index_count = pe.code[offset + 3 + material_size] as usize;
                let mut length = 3 + material_size + 1 + index_count * index_size;
                if have_tc {
                    length += index_count * 2 * tc_size;
                }

                let mut facet = Facet { flags, indices: Vec::new() };
                let index_base = &pe.code[offset + 3 + material_size + 1..];
                if flags.contains(FacetFlags::USE_SHORT_INDICES) {
                    let indices: &[u16] = unsafe { mem::transmute(index_base) };
                    for i in 0..index_count {
                        facet.indices.push(indices[i]);
                    }
                } else {
                    for i in 0..index_count {
                        facet.indices.push(index_base[i] as u16);
                    }
                }
                shape.meshes.last_mut().unwrap().facets.push(facet);

                sections.push(Section::new(0xFC, offset, length));
                offset += length;
                //println!("FLAGS: {:08b} => off: {}, ctn: {}, have_tc: {}, tc_size: {} => length: {}", flags, index_count_offset, index_count, have_tc, tc_size, length);
            } else if code[0] & 0xFF == 0x00BC {
                let flags = code[1] & 0xFF;
                let length = match flags {
                    0x96 => 8,
                    0x72 => 6,
                    0x68 => 10,
                    0x08 => 6,
                    _ => { break; }
                };
                sections.push(Section::new(0xBC, offset, length));
                offset += length;
            } else if code[0] == 0x00F0 {
                let buf = &pe.code[offset + 2..];

                // Push the entire contents to disk so that when this scan inevitably fails we can disassemble to find out why.
                let mut buffer = fs::File::create("/tmp/F0.bin").unwrap();
                buffer.write(buf).unwrap();

                // Find the next ret opcode that is followed by a known section header.
                let mut end = 0;
                loop {
                    if end >= buf.len() {
                        break;
                    }
                    if buf[end] == 0xC3 {
                        end += 1;
                        let next_code: &[u16] = unsafe { mem::transmute(&pe.code[offset + 2 + end..]) };
                        /*
                        UNKNOWN
                        0x0000
                        0x0566
                        0x05EB
                        0xE850

                        MAYBE SECTION?
                        0x0066

                        KNOWN Sections
                        0x0010
                        0x0012
                        0x0048**
                        0x0082
                        0x00B8
                        0x00C4
                        0x00C8
                        0x00D0
                        0x00E2
                        0x0006
                        0x00F0

                        KNOWN with mod
                        0xXXFC
                        0xXX1E
                        */

                        if next_code[0] == 0x0048 || next_code[0] == 0x0000 || next_code[0] == 0x0566 || next_code[0] == 0x05EB || next_code[0] == 0xE850 {
                            end += 2;
                        } else {
                            // println!("0x{:04X}", next_code[0]);
                            break;
                        }
                    }

                    if buf[end] == 0x68 { // push dword
                        end += 5;
                    } else if buf[end] == 0x81 { // op reg imm32
                        end += 6;
                    } else {
                        end += 1;
                    }
                }
                let length = 2 + end;
                sections.push(Section::new(0xF0, offset, length));
                offset += length;
            } else if code[0] == 0x00B8 {
                // B8 00 01 00
                sections.push(Section::new(0xB8, offset, 4));
                offset += 4;
            } else if code[0] == 0x00DA {
                sections.push(Section::new(0xDA, offset, 4));
                offset += 4;
            } else if code[0] == 0x0066 {
                sections.push(Section::new(0x66, offset, 10));
                offset += 10;
            } else if code[0] == 0x0040 {
                // 40 00   04 00   08 00, 25 00, 42 00, 5F 00
                let cnt = code[1] as usize;
                let length = 4 + cnt * 2;
                sections.push(Section::new(0x40, offset, length));
                offset += length;
//            } else if code[0] == 0x0048 {
//                sections.push(Section::new(0x48, offset, 4));
//                offset += 4;
            } else if code[0] == 0x00D0 {
                sections.push(Section::new(0xD0, offset, 4));
                offset += 4;
            } else if code[0] == 0x00E0 {
                sections.push(Section::new(0xE0, offset, 4));
                offset += 4;
            } else if code[0] & 0xFF == 0x00F6 {
                sections.push(Section::new(0xF6, offset, 7));
                offset += 7;
            } else if code[0] & 0xFF == 0x0038 {
                sections.push(Section::new(0x38, offset, 3));
                offset += 3;
            } else if code[0] == 0x0012 {
                // 12 00 03 00 38 20 05
                // 12 00 03 00 38 FB 01
                // 12 00 03 00 38 FB 01
                // 12 00 03 00 38 FB 02
                // 12 00 03 00 38 FE 00
                // 12 00 20 00 1E
                // 12 00 4B 00 1E
                // 12 00 DD 00 1E
                // 12 00 F9 00 1E
                //let length = if (code[1] >> 8) == 3 { 7 } else { 4 };
                let length = 4;
                sections.push(Section::new(0x0012, offset, length));
                offset += length;
            } else if code[0] == 0x0006 {
                // 06 00 6F 4B 00 00 B8 46 00 00 6F 4B F2 FF 05 00 61 08 38 71 08
                sections.push(Section::new(0x06, offset, 21));
                offset += 21;
            } else if code[0] == 0x000C {
                // 0C 00 05 56 D3 FF C5 5E E4 FF 05 00 41 02 38 B6 02
                sections.push(Section::new(0x0C, offset, 17));
                offset += 17;
            } else if code[0] == 0x000E {
                // 0E 00 06 FF 00 00 04 80 F3 FF 05 00 BB 03 38 05 08
                sections.push(Section::new(0x0E, offset, 17));
                offset += 17;
            } else if code[0] == 0x0010 {
                // 10 00 07 5F 0F 00 43 AA FD FF 05 00 1F 00 38 2E 04
                sections.push(Section::new(0x10, offset, 17));
                offset += 17;
            } else if code[0] == 0x006C {
                // 6C 00 06 00 00 00 05 00 36 06 38 9B 06
                sections.push(Section::new(0x6C, offset, 13));
                offset += 13;
            } else if code[0] == 0x00C4 {
                // C4 00 00 00 2F 00 0E 00 00 00 00 00 00 00 17 08
                sections.push(Section::new(0x6C, offset, 16));
                offset += 16;
            } else {
                break;
            }
            cnt += 1;
        }

        if pe.code.len() < offset {
            println!("OVERFLOW before last section: {} < {} last section is: {:?}", pe.code.len(), offset, sections.last().unwrap());
            let last = sections.pop().unwrap();
            let replace = Section {
                kind: SectionKind::Invalid,
                offset: last.offset,
                length: pe.code.len() - last.offset,
            };
            sections.push(replace);
        } else if pe.code.len() > offset {
            sections.push(Section::unknown(offset, cmp::min(1024, pe.code.len() - offset)));
        }

        return Ok((shape, sections));
    }

    fn _apply_tags(pe: &peff::PE, sections: &Vec<Section>) -> Result<Vec<Tag>> {
        let mut tags = Vec::new();
        for &reloc in pe.relocs.iter() {
            assert!((reloc as usize) + 4 <= pe.code.len());
            let dwords: &[u32] = unsafe { mem::transmute(&pe.code[reloc as usize..]) };
            let thunk_ptr = dwords[0];
            if let Some(thunks) = pe.thunks.clone() {
                if thunks.contains_key(&thunk_ptr) || thunks.contains_key(&(thunk_ptr - 2)) {
                    // This relocation is for a pointer into the thunk table; store the name so
                    // that we can print the name instead of the address.
                    //println!("Relocating {:X} in code to {}", thunk_ptr, &thunks[&thunk_ptr].name);
                    tags.push(Tag { kind: TagKind::RelocatedCall(thunks[&thunk_ptr].name.clone()), offset: reloc as usize, length: 4 });
                } else {
                    // This relocation is to somewhere in code; mark both it and the target word
                    // of the pointer that is stored at the reloc position.
                    tags.push(Tag { kind: TagKind::RelocatedRef, offset: reloc as usize, length: 4 });

                    assert!(thunk_ptr > pe.code_vaddr, "thunked ptr before code");
                    assert!(thunk_ptr <= pe.code_vaddr + pe.code.len() as u32 - 4, "thunked ptr after code");
                    let code_offset = thunk_ptr - pe.code_vaddr;
                    let value_to_relocate_arr: &[u16] = unsafe { mem::transmute(&pe.code[code_offset as usize..]) };
                    let value_to_relocate = value_to_relocate_arr[0];
                    //println!("Relocating {:X} at offset {:X}", value_to_relocate, code_offset);
                    tags.push(Tag { kind: TagKind::RelocationTarget, offset: code_offset as usize, length: 2 });
                }
            }
        }
        return Ok(tags);
    }
}

fn format_sections(code: &[u8], sections: &Vec<Section>, tags: &mut Vec<Tag>, mode: ShowMode) -> Vec<String> {
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

    let mut out = Vec::new();

    // Simple view of all sections concatenated.
    match mode {
        ShowMode::AllOneLine => {
            let mut line: Vec<char> = Vec::new();
            for section in sections {
                accumulate_section(code, section, tags, &mut line);
            }
            out.push(line.iter().collect::<String>());
        },
        ShowMode::AllPerLine => {
            for section in sections {
                let mut line: Vec<char> = Vec::new();
                accumulate_section(code, section, tags, &mut line);
                out.push(line.iter().collect::<String>());
            }
        },
        ShowMode::Unknown => {
            for section in sections {
                if let SectionKind::Unknown = section.kind {
                    let mut line: Vec<char> = Vec::new();
                    accumulate_section(code, section, tags, &mut line);
                    out.push(line.iter().collect::<String>());
                }
            }
        },
        ShowMode::UnknownMinus => {
            for (i, section) in sections.iter().enumerate() {
                if let SectionKind::Unknown = section.kind {
                    let mut line: Vec<char> = Vec::new();
                    accumulate_section(code, section, tags, &mut line);
                    if i > 2 {
                        accumulate_section(code, &sections[i - 3], tags, &mut line);
                    }
                    if i > 1 {
                        accumulate_section(code, &sections[i - 2], tags, &mut line);
                    }
                    if i > 0 {
                        accumulate_section(code, &sections[i - 1], tags, &mut line);
                    }
                    out.push(line.iter().collect::<String>());
                }
            }
        },
        ShowMode::UnknownFacet => {
            for section in sections {
                if let SectionKind::Unknown = section.kind {
                    if section.length > 0 && code[section.offset] == 0xFC {
                        let mut line: Vec<char> = Vec::new();
                        //accumulate_section(code, section, tags, &mut line);
                        accumulate_facet_section(code, section, &mut line);
                        out.push(line.iter().collect::<String>());
                    }
                }
            }
        },
        ShowMode::Custom => {
            // Grab sections that we care about and stuff them into lines.
            for (i, section) in sections.iter().enumerate() {
                let mut line: Vec<char> = Vec::new();
                if i > 0 {
                    if let SectionKind::Main(k) = sections[i - 1].kind {
                        if k != 0xFC { continue; }
                        if let SectionKind::Unknown = sections[i].kind {
                            line.push('0');
                            line.push('|');
                            line.push(' ');
                            if k == 0xFC {
                                accumulate_facet_section(code, &sections[i - 1], &mut line);
                            } else {
                                accumulate_section(code, &sections[i - 1], tags, &mut line)
                            }
                            accumulate_section(code, &sections[i], tags, &mut line);
                            out.push(line.iter().collect::<String>());
                        } else {
                            line.push('1');
                            line.push('|');
                            line.push(' ');
                            if k == 0xFC {
                                accumulate_facet_section(code, &sections[i - 1], &mut line);
                            } else {
                                accumulate_section(code, &sections[i - 1], tags, &mut line)
                            }
                            out.push(line.iter().collect::<String>());
                        }
                    }
                }
            }
        }
    }

    return out;
}

const COLORIZE: bool = true;

fn tgt<'a>(x: &'a mut Vec<char>, y: &'a mut Vec<char>) -> &'a mut Vec<char> {
    if COLORIZE {
        return x;
    }
    return y;
}

fn accumulate_facet_section(code: &[u8], section: &Section, line: &mut Vec<char>) {
    if section.offset + section.length >= code.len() {
        println!("OVERFLOW at section: {:?}", section);
        return;
    }
    let mut nul = Vec::new();
    let n = &mut nul;

    Escape::new().bg(section.color()).put(tgt(line, n));
    b2h(code[section.offset + 0], line);
    Escape::new().put(tgt(line, n));

    Escape::new().fg(section.color()).put(tgt(line, n));
    line.push(' ');
    b2b(code[section.offset + 1], line);
    line.push('_');
    b2b(code[section.offset + 2], line);

    for &b in &code[section.offset + 3..section.offset + section.length] {
        line.push(' ');
        b2h(b, line);
    }

    Escape::new().put(tgt(line, n));
    line.push(' ');
}

fn accumulate_section(code: &[u8], section: &Section, tags: &Vec<Tag>, v: &mut Vec<char>) {
    if section.length == 0 {
        return;
    }
    if !section.show() {
        return;
    }
    if section.offset + section.length > code.len() {
        println!("OVERFLOW at section: {:?}", section);
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

    if section.length == 1 {
        Escape::new().bg(section.color()).put(tgt(v, n));
        b2h(code[section.offset + 0], v);
        Escape::new().put(tgt(v, n));
        v.push(' ');
        return;
    }

    Escape::new().bg(section.color()).put(tgt(v, n));
    b2h(code[section.offset + 0], v);
    v.push(' ');
    b2h(code[section.offset + 1], v);
//    v.push('_');
//    v.push('_');
    Escape::new().put(tgt(v, n));
    Escape::new().fg(section.color()).put(tgt(v, n));
    let mut off = section.offset + 2;
    for &b in &code[section.offset + 2..section.offset + section.length] {
        // Push any tag closers.
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

                match Shape::new(&data,&path, ShowMode::UnknownMinus) {
                    Ok((_verts, mut desc)) => {
                        rv.append(&mut desc);
                    },
                    Err(_) => {
                        rv.push(format!("SKIPPED {}", path));
                    }
                }
                //let (_verts, mut desc) = .unwrap();
            }

            //assert_eq!(format!("./test_data/{}", t.object.file_name), path);
            //rv.push(format!("{:?} <> {} <> {}",
            //                t.object.unk_explosion_type,
            //                t.object.long_name, path));
        }
        rv.sort();

        for v in rv {
            println!("{}", v);
        }
    }
}
