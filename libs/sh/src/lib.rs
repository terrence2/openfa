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
use std::{cmp, fs, mem, str};
use std::collections::{HashMap, HashSet};
use reverse::{bs2s, b2h, b2b, Escape, Color};

use std::fs::File;
use std::io::prelude::*;


/// A version of the shape for slicing/dicing on the CPU for exploration. The normal
/// load path will go straight into GPU buffers.
pub struct CpuShape {
    pub source: String,
    pub instrs: Vec<Instr>
}

bitflags! {
    pub struct FacetFlags : u16 {
        const HAVE_MATERIAL      = 0b0100_0000_0000_0000;
        const HAVE_TEXCOORDS     = 0b0000_0100_0000_0000;
        const USE_SHORT_INDICES  = 0b0000_0000_0000_0100;
        const USE_SHORT_MATERIAL = 0b0000_0000_0000_0010;
        const USE_BYTE_TEXCOORDS = 0b0000_0000_0000_0001;
        const UNK_MATERIAL_RELATED = 0b0000_0001_0000_0000;
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

use std::convert::AsMut;

fn clone_into_array<A, T>(slice: &[T]) -> A
    where A: Sized + Default + AsMut<[T]>,
          T: Clone
{
    let mut a = Default::default();
    <A as AsMut<[T]>>::as_mut(&mut a).clone_from_slice(slice);
    a
}

fn read_name(n: &[u8]) -> Result<String> {
    let end_offset: usize = n.iter().position(|&c| c == 0).chain_err(|| "no terminator")?;
    return Ok(str::from_utf8(&n[..end_offset]).chain_err(|| "names should be utf8 encoded")?.to_owned());
}

pub struct TextureRef {
    pub offset: usize,
    pub filename: String
}

impl TextureRef {
    pub const MAGIC: u8 = 0xE2;
    pub const SIZE: usize = 16;

    fn from_bytes(offset: usize, code: &[u8]) -> Result<Self> {
        let data = &code[offset..];
        assert_eq!(data[0], Self::MAGIC);
        assert_eq!(data[1], 0);
        let filename = read_name(&data[2..Self::SIZE]).chain_err(|| "read name")?;
        return Ok(TextureRef { offset, filename });
    }

    fn size(&self) -> usize {
        return Self::SIZE;
    }

    fn at_offset(&self) -> usize {
        self.offset
    }

    fn show(&self) -> String {
        format!("TextureRef @ {:04X} -> {}", self.offset, self.filename)
    }
}

// These are for code E0. They are used for nose and tail art and for the country
// insignia on wings. Our PICs appear to be NOSE__.PIC, ROUND__.PIC, RIGHT__.PIC,
// and LEFT__.PIC.
#[derive(Debug)]
pub enum TextureIndexKind {
    TailLeft,
    TailRight,
    Nose,
    WingLeft,
    WingRight,
}

impl TextureIndexKind {
    fn from_u16(kind: u16) -> Result<Self> {
        match kind {
            0 => Ok(TextureIndexKind::TailLeft),
            1 => Ok(TextureIndexKind::TailRight),
            2 => Ok(TextureIndexKind::Nose),
            3 => Ok(TextureIndexKind::WingLeft),
            4 => Ok(TextureIndexKind::WingRight),
            _ => bail!("unknown texture index")
        }
    }
}

pub struct TextureIndex {
    pub offset: usize,
    pub unk0: u8,
    pub kind: TextureIndexKind
}

impl TextureIndex {
    pub const MAGIC: u8 = 0xE0;
    pub const SIZE: usize = 4;

    fn from_bytes(offset: usize, code: &[u8]) -> Result<Self> {
        let data = &code[offset..];
        assert_eq!(data[0], Self::MAGIC);
        let data2: &[u16] = unsafe { mem::transmute(&data[2..]) };
        return Ok(TextureIndex { offset, unk0: data[1], kind: TextureIndexKind::from_u16(data2[0])? });
    }

    fn size(&self) -> usize {
        return Self::SIZE;
    }

    fn at_offset(&self) -> usize {
        self.offset
    }

    fn show(&self) -> String {
        format!("TextureIndexKind @ {:04X}: {}, {:?}", self.offset, self.unk0, self.kind)
    }
}

pub struct SourceRef {
    pub offset: usize,
    pub unk0: u8,
    pub source: String
}

impl SourceRef {
    pub const MAGIC: u8 = 0x42;

    fn from_bytes(offset: usize, code: &[u8]) -> Result<Self> {
        let data = &code[offset..];
        assert_eq!(data[0], Self::MAGIC);
        let source = read_name(&data[2..]).chain_err(|| "read name")?;
        return Ok(SourceRef { offset, unk0: data[1], source });
    }

    fn size(&self) -> usize {
        return 2 + self.source.len() + 1;
    }

    fn at_offset(&self) -> usize {
        self.offset
    }

    fn show(&self) -> String {
        format!("SourceRef @ {:04X}: {}, {}", self.offset, self.unk0, self.source)
    }
}

pub struct VertexBuf {
    pub offset: usize,
    pub unk0: u16,
    pub raw_verts: Vec<[u16; 3]>,
    pub verts: Vec<[f32; 3]>,
}

impl VertexBuf {
    pub const MAGIC: u8 = 0x82;

    fn from_bytes(offset: usize, code: &[u8]) -> Result<Self> {
        let data = &code[offset..];
        assert_eq!(data[0], Self::MAGIC);
        assert_eq!(data[1], 0);
        let head: &[u16] = unsafe { mem::transmute(&data[2..6]) };
        let words: &[u16] = unsafe { mem::transmute(&data[6..]) };
        let mut buf = VertexBuf { offset, unk0: head[2], raw_verts: Vec::new(), verts: Vec::new() };
        fn s2f(s: u16) -> f32 { (s as i16) as f32 }
        let nverts = head[0] as usize;
        for i in 0..nverts {
            let x = s2f(words[i * 3 + 0]);
            let y = s2f(words[i * 3 + 1]);
            let z = s2f(words[i * 3 + 2]);
            buf.verts.push([x, y, z]);

            let x = words[i * 3 + 0];
            let y = words[i * 3 + 1];
            let z = words[i * 3 + 2];
            buf.raw_verts.push([x, y, z]);
        }
        return Ok(buf);
    }

    fn size(&self) -> usize {
        return 6 + self.verts.len() * 6;
    }

    fn at_offset(&self) -> usize {
        self.offset
    }

    fn show(&self) -> String {
        let s = self.raw_verts.iter()
            .map(|v| { format!("({:04X},{:04X},{:04X})", v[0], v[1], v[2]) })
            .collect::<Vec<String>>()
            .join(", ");
        format!("VertexBuf @ {:04X}: {} ({:b}) => {}verts -> {}", self.offset, self.unk0,
                self.unk0, self.verts.len(), s)
    }
}

pub struct Facet {
    pub offset: usize,
    pub length: usize,
    pub flags: FacetFlags,
    pub mat_desc: String,
    pub indices: Vec<u16>,
    pub max_index: u16,
    pub min_index: u16,
    pub tex_coords: Vec<[u16;2]>,
}

impl Facet {
    pub const MAGIC: u8 = 0xFC;

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
    fn from_bytes(offset: usize, code: &[u8]) -> Result<Self> {
        let data = &code[offset..];
        assert_eq!(data[0], Self::MAGIC);

        let flags_word = ((data[1] as u16) << 8) | (data[2] as u16);
        assert_eq!(flags_word & 0x00F0, 0u16);
        let flags = FacetFlags::from_u16(flags_word);

        let mut off = 3;

        // Material
        let material_size = if flags.contains(FacetFlags::HAVE_MATERIAL){
            if flags.contains(FacetFlags::USE_SHORT_MATERIAL) {
                11
            } else {
                14
            }
        } else {
            2
        };
        let mat_desc = bs2s(&data[off..off + material_size]);
        off += material_size;

        // Index count.
        let index_count = data[off] as usize;
        off += 1;

        // Indexes.
        let mut indices = Vec::new();
        let index_u8 = &data[off..];
        let index_u16 : &[u16] = unsafe { mem::transmute(index_u8) };
        for i in 0..index_count {
            let index = if flags.contains(FacetFlags::USE_SHORT_INDICES) {
                off += 2;
                index_u16[i]
            } else {
                off += 1;
                index_u8[i] as u16
            };
            indices.push(index);
        }

        // Tex Coords
        let mut tex_coords = Vec::new();
        if flags.contains(FacetFlags::HAVE_TEXCOORDS) {
            let tc_u8 = &data[off..];
            let tc_u16: &[u16] = unsafe { mem::transmute(tc_u8) };
            for i in 0..index_count {
                let (u, v) = if flags.contains(FacetFlags::USE_BYTE_TEXCOORDS) {
                    off += 2;
                    (tc_u8[i * 2 + 0] as u16,
                     tc_u8[i * 2 + 1] as u16)
                } else {
                    off += 4;
                    (tc_u16[i * 2 + 0],
                     tc_u16[i * 2 + 1])
                };
                tex_coords.push([u, v]);
            }

            assert_eq!(tex_coords.len(), indices.len());
        }

        return Ok(Facet {
            offset,
            length: off,
            flags,
            mat_desc,
            max_index: *indices.iter().max().unwrap(),
            min_index: *indices.iter().min().unwrap(),
            indices,
            tex_coords,
        });
    }

    fn size(&self) -> usize {
        return self.length;
    }

    fn at_offset(&self) -> usize {
        self.offset
    }

    fn show(&self) -> String {
        let ind = self.indices.iter().map(|i| format!("{:X}", i))
            .collect::<Vec<String>>()
            .join(", ");
        format!("Facet @ {:04X} : {:016b} : [{}] : {:?}", self.offset, self.flags, ind, self.tex_coords)
    }
}

pub struct X86Code {
    pub offset: usize,
    pub code: Vec<u8>,
    pub formatted: String,
}

impl X86Code {
    pub const MAGIC: u8 = 0xF0;

    fn from_bytes(name: String, offset: usize, pe: &peff::PE) -> Result<Self> {
        let section = &pe.code[offset..];
        assert_eq!(section[0], Self::MAGIC);
        assert_eq!(section[1], 0);

        let code = &section[2..];
        // Find the next ret opcode that is followed by a known section header.
        let mut end = 0;
        let mut ip = 0;

        loop {
            if ip >= code.len() {
                panic!("we should ret well before here")
            }
            if code[ip] == 0xC3 {
                ip += 1;
                let next_code: &[u16] = unsafe { mem::transmute(&section[2 + ip..]) };
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

                // Our x86 virtual interpreter only supports a couple ops, so in order to get things
                // working for now, we're just going to fast-forward past anything that doesn't
                // look quite right.
                if next_code[0] == 0x0048 ||
                    next_code[0] == 0x0000 ||
                    next_code[0] == 0x0566 ||
                    next_code[0] == 0x05EB ||
                    next_code[0] == 0xE850 ||
                    next_code[0] == 0x8966 ||
                    next_code[0] == 0x002E
                {
                    ip += 2;
                } else {
                    // println!("0x{:04X}", next_code[0]);
                    end = ip;
                    break;
                }
            }

            if code[ip] == 0x68 { // push dword
                ip += 5;
            } else if code[ip] == 0x81 { // op reg imm32
                ip += 6;
            } else {
                ip += 1;
            }
        }

        let sec = reverse::Section::new(0xF0, offset, end + 2);
        let tags = reverse::get_all_tags(pe);
        let mut v = Vec::new();
        reverse::accumulate_section(&pe.code, &sec, &tags, &mut v);
        let fmt = v.iter().collect::<String>();

        let tmp_name = format!("/tmp/{}-{}.x86", name, offset);
        let mut file = File::create(tmp_name).unwrap();
        file.write_all(&code[0..end]).unwrap();

        return Ok(X86Code {
            offset,
            code: code[0..end].to_owned(),
            formatted: fmt
        });
    }

    fn size(&self) -> usize {
        return self.code.len() + 2;
    }

    fn at_offset(&self) -> usize {
        self.offset
    }

    fn show(&self) -> String {
        //format!("X86Code: {}", bs2s(&self.code))
        format!("X86Code @ {:04X}: {}", self.offset, self.formatted)
    }
}

pub struct UnkCE {
    pub offset: usize,
    pub data: [u8; 40 - 2]
}

impl UnkCE {
    pub const MAGIC: u8 = 0xCE;
    pub const SIZE: usize = 40;

    fn from_bytes(offset: usize, code: &[u8]) -> Result<Self> {
        let data = &code[offset..];
        assert_eq!(data[0], Self::MAGIC);
        assert_eq!(data[1], 0);
        // Note: no default for arrays larger than 32 elements.
        let s = &data[2..];
        return Ok(Self {
                    offset,
                    data: [s[00], s[01], s[02], s[03], s[04], s[05], s[06], s[07], s[08], s[09],
                           s[10], s[11], s[12], s[13], s[14], s[15], s[16], s[17], s[18], s[19],
                           s[20], s[21], s[22], s[23], s[24], s[25], s[26], s[27], s[28], s[29],
                           s[30], s[31], s[32], s[33], s[34], s[35], s[36], s[37]]
                    });
    }

    fn size(&self) -> usize {
        return Self::SIZE;
    }

    fn at_offset(&self) -> usize {
        self.offset
    }

    fn show(&self) -> String {
        format!("UnkCE @ {:04X}: {}", self.offset, bs2s(&self.data))
    }
}

pub struct UnkBC {
    pub offset: usize,
    flags: u8,
    unk0: u8,
    length: usize,
    data: Vec<u8>
}

impl UnkBC {
    pub const MAGIC: u8 = 0xBC;

    fn from_bytes(offset: usize, code: &[u8]) -> Result<Self> {
        let data = &code[offset..];
        assert_eq!(data[0], Self::MAGIC);

        let flags = data[2];
        let unk0 = data[3];
        let length = match flags {
            0x96 => 8,
            0x72 => 6,
            0x68 => 10,
            0x08 => 6,
            _ => bail!("unknown section BC flags: {}", flags)
        };
        let data = data[4..length].to_owned();
        return Ok(UnkBC {
           offset, flags, unk0, length, data
        });
    }

    fn size(&self) -> usize {
        return self.length;
    }

    fn at_offset(&self) -> usize {
        self.offset
    }

    fn show(&self) -> String {
        format!("UnkBC @ {:04X}: flags:{}, unk0:{}, len:{}, data:{}", self.offset, self.flags, self.unk0, self.length, bs2s(&self.data))
    }
}

pub struct Unk40 {
    pub offset: usize,
    count: usize,
    length: usize,
    data: Vec<u16>,
}

impl Unk40 {
    pub const MAGIC: u8 = 0x40;

    // 40 00   04 00   08 00, 25 00, 42 00, 5F 00
    fn from_bytes(offset: usize, code: &[u8]) -> Result<Self> {
        let data = &code[offset..];
        assert_eq!(data[0], Self::MAGIC);
        assert_eq!(data[1], 0);
        let words: &[u16] = unsafe { mem::transmute(&data[2..]) };
        let count = words[0] as usize;
        let length = 4 + count * 2;
        let data = words[1..count + 1].to_owned();
        return Ok(Unk40 { offset, count, length, data });
    }

    fn size(&self) -> usize {
        return self.length;
    }

    fn at_offset(&self) -> usize {
        self.offset
    }

    fn show(&self) -> String {
        format!("Unk40 @ {:04X}: cnt:{}, len:{}, data:{:?}", self.offset, self.count, self.length, self.data)
    }
}

pub struct UnkF6 {
    pub offset: usize,
    pub data: [u8; 6]
}

impl UnkF6 {
    pub const MAGIC: u8 = 0xF6;
    pub const SIZE: usize = 7;

    fn from_bytes(offset: usize, code: &[u8]) -> Result<Self> {
        let data = &code[offset..];
        assert_eq!(data[0], Self::MAGIC);
        return Ok(Self { offset, data: clone_into_array(&data[1..Self::SIZE]) });
    }

    fn size(&self) -> usize {
        return Self::SIZE;
    }

    fn at_offset(&self) -> usize {
        self.offset
    }

    fn show(&self) -> String {
        format!("UnkF6 @ {:04X}: {}", self.offset, bs2s(&self.data))
    }
}

pub struct UnkJumpIfLowDetail {
    pub offset: usize,
    pub offset_to_next: usize,
    pub data: [u8; 2]
}

// I think this probably means something like: fastforward to "next_offset" if
// we are running in low-detail mode. Seems to skip nose art and a couple random
// polys in F22, a big rock in ROCKB, and the textured polys in BUNK.
//
// This probably works with UNKC8 somehow: in BUNK
impl UnkJumpIfLowDetail {
    pub const MAGIC: u8 = 0x38;
    pub const SIZE: usize = 3;

    fn from_bytes(offset: usize, code: &[u8]) -> Result<Self> {
        let data = &code[offset..];
        assert_eq!(data[0], Self::MAGIC);
        let word_ref: &[u16] = unsafe { mem::transmute( &data[1..] ) };
        let offset_to_next = word_ref[0] as usize;
        return Ok(Self { offset, offset_to_next, data: clone_into_array(&data[1..Self::SIZE]) });
    }

    fn size(&self) -> usize {
        return Self::SIZE;
    }

    fn at_offset(&self) -> usize {
        self.offset
    }

    pub fn next_offset(&self) -> usize {
        // Our start offset + our size + 1 + offset_to_next.
        return self.offset + 4 + 2 + self.offset_to_next;
    }

    fn show(&self) -> String {
        format!("UnkJumpIfLowDetail @ {:04X}: {:X} => {:X}", self.offset, self.offset_to_next, self.next_offset())
    }
}

// At: 6 => UnkC8 @ 0039: 62 01 19 00 D5 00
// Offset + 0xD5 + sizeof(UnkC8) => start of next section.

// At: 5 => UnkAC @ 0035: 74 01
// Offset + 0x174 + sizeof(UnkAC) => points to after the normal version to the start of the destroyed version.

// At: 1 => UnkF2 @ 000E: 7B 07
// Seems to point to trailer: 0xe + 4 + 0x77B => 0x78D, which is the start of trailer.

// In BUNKB
// At: 7 => UnkA6 @ 0041: CF 00 01 00
// 0x41 + 0xCF + 6 => 0x116 points past textured polys.

pub struct UnkJumpIfNotShown {
    pub offset: usize,
    pub offset_to_next: usize,
    pub data: [u8; 2]
}

impl UnkJumpIfNotShown {
    pub const MAGIC: u8 = 0xF2;
    pub const SIZE: usize = 4;

    fn from_bytes(offset: usize, code: &[u8]) -> Result<Self> {
        let data = &code[offset..];
        assert_eq!(data[0], Self::MAGIC);
        assert_eq!(data[1], 0x00);
        let word_ref: &[u16] = unsafe { mem::transmute( &data[2..] ) };
        let offset_to_next = word_ref[0] as usize;
        return Ok(Self { offset, offset_to_next, data: clone_into_array(&data[2..Self::SIZE]) });
    }

    fn size(&self) -> usize {
        return Self::SIZE;
    }

    fn at_offset(&self) -> usize {
        self.offset
    }

    pub fn next_offset(&self) -> usize {
        // Our start offset + our size + 1 + offset_to_next.
        return self.offset + 4 + self.offset_to_next;
    }

    fn show(&self) -> String {
        format!("UnkJumpIfNotShown @ {:04X}: {:X} => {:X}", self.offset, self.offset_to_next, self.next_offset())
    }
}

// EE 00 E4 00 04 00 00 00 96 00 00 00 C7 00 1D 00 C7 00 1D 00 96 00 7A 00 00 00 00 00 06 00

pub struct TrailerUnknown {
    pub offset: usize,
    pub data: Vec<u8>
}

impl TrailerUnknown {
    fn from_bytes(offset: usize, code: &[u8]) -> Result<Self> {
        let data = &code[offset..];
        return Ok(Self { offset, data: data.to_owned() });
    }

    fn size(&self) -> usize {
        return self.data.len();
    }

    fn at_offset(&self) -> usize {
        self.offset
    }

    fn show(&self) -> String {
        use reverse::{format_sections, Section, ShowMode};
        let out = format_sections(&self.data, &vec![Section::new(0x0000, 0, self.data.len())], &mut vec![], ShowMode::AllPerLine);
        format!("Trailer @ {:04X}: {:6}b => {}", self.offset, self.data.len(), out[0])
    }
}


macro_rules! opaque_instr {
    ($name:ident, $magic:expr, $size:expr) => {
        pub struct $name {
            pub offset: usize,
            pub data: [u8; $size - 2]
        }

        impl $name {
            pub const MAGIC: u8 = $magic;
            pub const SIZE: usize = $size;

            fn from_bytes(offset: usize, code: &[u8]) -> Result<Self> {
                let data = &code[offset..];
                assert_eq!(data[0], Self::MAGIC);
                assert!(data[1] == 0 || data[1] == 0xFF);
                return Ok(Self { offset, data: clone_into_array(&data[2..Self::SIZE]) });
            }

            fn size(&self) -> usize {
                return Self::SIZE;
            }

            fn at_offset(&self) -> usize {
                self.offset
            }

            fn show(&self) -> String {
                format!("{} @ {:04X}: {}", stringify!($name), self.offset, bs2s(&self.data))
            }
        }
    }
}

opaque_instr!(Header, 0xFF, 14);
opaque_instr!(Unk46, 0x46, 2);
opaque_instr!(UnkB2, 0xB2, 2);
opaque_instr!(UnkEE, 0xEE, 2);
opaque_instr!(Unk12, 0x12, 4);
opaque_instr!(Unk48, 0x48, 4);
opaque_instr!(UnkAC, 0xAC, 4);
opaque_instr!(UnkB8, 0xB8, 4);
opaque_instr!(UnkCA, 0xCA, 4);
opaque_instr!(UnkD0, 0xD0, 4);
opaque_instr!(UnkDA, 0xDA, 4);
//opaque_instr!(UnkF2, 0xF2, 4);
opaque_instr!(UnkA6, 0xA6, 6);
opaque_instr!(UnkC8, 0xC8, 8);
opaque_instr!(Unk66, 0x66, 10);
opaque_instr!(Unk7A, 0x7A, 10);
opaque_instr!(Unk78, 0x78, 12);
opaque_instr!(UnkC4, 0xC4, 16);
opaque_instr!(Unk0C, 0x0C, 17);
opaque_instr!(Unk0E, 0x0E, 17);
opaque_instr!(Unk10, 0x10, 17);
opaque_instr!(Unk6C, 0x6C, 13);
opaque_instr!(Unk06, 0x06, 21);


pub enum Instr {
    Header(Header),

    // Fixed size, with wasted 0 byte.
    Unk46(Unk46),
    UnkB2(UnkB2),
    UnkEE(UnkEE),
    Unk12(Unk12),
    Unk48(Unk48),
    UnkAC(UnkAC),
    UnkB8(UnkB8),
    UnkCA(UnkCA),
    UnkD0(UnkD0),
    UnkDA(UnkDA),
    UnkJumpIfNotShown(UnkJumpIfNotShown),
    UnkA6(UnkA6),
    UnkC8(UnkC8),
    Unk66(Unk66),
    Unk7A(Unk7A),
    Unk78(Unk78),
    UnkC4(UnkC4),
    Unk0C(Unk0C),
    Unk0E(Unk0E),
    Unk10(Unk10),
    Unk6C(Unk6C),
    Unk06(Unk06),
    UnkCE(UnkCE),

    // Fixed size, without wasted 0 byte after header.
    UnkF6(UnkF6),
    UnkJumpIfLowDetail(UnkJumpIfLowDetail),

    // Variable size.
    UnkBC(UnkBC),
    Unk40(Unk40),
    TrailerUnknown(TrailerUnknown),

    // Known quantities.
    TextureRef(TextureRef),     // 0x00E2
    TextureIndex(TextureIndex), // 0x00E0
    SourceRef(SourceRef),   // 0x0042
    VertexBuf(VertexBuf),   // 0x0082
    Facet(Facet),           // 0x__FC

    // Wtf
    X86Code(X86Code),
}

macro_rules! impl_for_all_instr {
    ($self:ident, $f:ident) => {
        match $self {
            &Instr::Header(ref i) => i.$f(),
            &Instr::Unk46(ref i) => i.$f(),
            &Instr::UnkB2(ref i) => i.$f(),
            &Instr::UnkEE(ref i) => i.$f(),
            &Instr::Unk12(ref i) => i.$f(),
            &Instr::Unk48(ref i) => i.$f(),
            &Instr::UnkAC(ref i) => i.$f(),
            &Instr::UnkB8(ref i) => i.$f(),
            &Instr::UnkCA(ref i) => i.$f(),
            &Instr::UnkD0(ref i) => i.$f(),
            &Instr::UnkDA(ref i) => i.$f(),
            &Instr::UnkJumpIfNotShown(ref i) => i.$f(),
            &Instr::UnkA6(ref i) => i.$f(),
            &Instr::UnkC8(ref i) => i.$f(),
            &Instr::Unk66(ref i) => i.$f(),
            &Instr::Unk7A(ref i) => i.$f(),
            &Instr::Unk78(ref i) => i.$f(),
            &Instr::UnkC4(ref i) => i.$f(),
            &Instr::Unk0C(ref i) => i.$f(),
            &Instr::Unk0E(ref i) => i.$f(),
            &Instr::Unk10(ref i) => i.$f(),
            &Instr::Unk6C(ref i) => i.$f(),
            &Instr::Unk06(ref i) => i.$f(),
            &Instr::UnkCE(ref i) => i.$f(),
            &Instr::UnkF6(ref i) => i.$f(),
            &Instr::UnkJumpIfLowDetail(ref i) => i.$f(),
            &Instr::UnkBC(ref i) => i.$f(),
            &Instr::Unk40(ref i) => i.$f(),
            &Instr::TrailerUnknown(ref i) => i.$f(),
            &Instr::TextureIndex(ref i) => i.$f(),
            &Instr::TextureRef(ref i) => i.$f(),
            &Instr::SourceRef(ref i) => i.$f(),
            &Instr::VertexBuf(ref i) => i.$f(),
            &Instr::Facet(ref i) => i.$f(),
            &Instr::X86Code(ref i) => i.$f(),
         }
    }
}

impl Instr {
    pub fn show(&self) -> String {
        impl_for_all_instr!(self, show)
    }

    pub fn at_offset(&self) -> usize {
        impl_for_all_instr!(self, at_offset)
    }
}

macro_rules! consume_instr {
    ($name:ident, $instr:ident, $pe:ident, $offset:ident) => {
        let instr = $name::from_bytes($offset, &$pe.code)?;
        let sz = instr.size();
        $instr.push(Instr::$name(instr));
        $offset += sz;
    }
}

impl CpuShape {
    pub fn new(data: &[u8]) -> Result<Self> {
        let pe = peff::PE::parse(data).chain_err(|| "parse pe")?;

        let shape = Self::_read_sections(&pe).chain_err(|| "read sections")?;
        return Ok(shape);
    }

    fn _read_sections(pe: &peff::PE) -> Result<Self> {

        let mut offset = 0;
        let mut n_coords = 0;

        let mut instrs = Vec::new();

        loop {
            assert!(offset < pe.code.len());

            let _code: &[u16] = unsafe { mem::transmute(&pe.code[offset..]) };
            //println!("AT: {:04X}", _code[0]);
            let code: &[u8] = &pe.code[offset..];

            if code[0] == 0x1E {
                offset += 1;

            } else if code[0] == Header::MAGIC {
                consume_instr!(Header, instrs, pe, offset);

            } else if code[0] == Unk46::MAGIC {
                consume_instr!(Unk46, instrs, pe, offset);

            } else if code[0] == UnkB2::MAGIC {
                consume_instr!(UnkB2, instrs, pe, offset);

            } else if code[0] == UnkEE::MAGIC {
                consume_instr!(UnkEE, instrs, pe, offset);

            } else if code[0] == Unk12::MAGIC {
                consume_instr!(Unk12, instrs, pe, offset);

            } else if code[0] == Unk48::MAGIC {
                consume_instr!(Unk48, instrs, pe, offset);

            } else if code[0] == UnkAC::MAGIC {
                consume_instr!(UnkAC, instrs, pe, offset);

            } else if code[0] == UnkB8::MAGIC {
                consume_instr!(UnkB8, instrs, pe, offset);

            } else if code[0] == UnkCA::MAGIC {
                consume_instr!(UnkCA, instrs, pe, offset);

            } else if code[0] == UnkD0::MAGIC {
                consume_instr!(UnkD0, instrs, pe, offset);

            } else if code[0] == UnkDA::MAGIC {
                consume_instr!(UnkDA, instrs, pe, offset);

            } else if code[0] == UnkJumpIfNotShown::MAGIC {
                consume_instr!(UnkJumpIfNotShown, instrs, pe, offset);

            } else if code[0] == UnkA6::MAGIC {
                consume_instr!(UnkA6, instrs, pe, offset);

            } else if code[0] == UnkC8::MAGIC {
                consume_instr!(UnkC8, instrs, pe, offset);

            } else if code[0] == Unk66::MAGIC {
                consume_instr!(Unk66, instrs, pe, offset);

            } else if code[0] == Unk78::MAGIC {
                consume_instr!(Unk78, instrs, pe, offset);

            } else if code[0] == Unk7A::MAGIC {
                consume_instr!(Unk7A, instrs, pe, offset);

            } else if code[0] == UnkC4::MAGIC {
                consume_instr!(UnkC4, instrs, pe, offset);

            } else if code[0] == Unk0C::MAGIC {
                consume_instr!(Unk0C, instrs, pe, offset);

            } else if code[0] == Unk0E::MAGIC {
                consume_instr!(Unk0E, instrs, pe, offset);

            } else if code[0] == Unk10::MAGIC {
                consume_instr!(Unk10, instrs, pe, offset);

            } else if code[0] == Unk6C::MAGIC {
                consume_instr!(Unk6C, instrs, pe, offset);

            } else if code[0] == Unk06::MAGIC {
                consume_instr!(Unk06, instrs, pe, offset);

            } else if code[0] == UnkCE::MAGIC {
                consume_instr!(UnkCE, instrs, pe, offset);

            } else if code[0] == UnkBC::MAGIC {
                consume_instr!(UnkBC, instrs, pe, offset);

            } else if code[0] == UnkF6::MAGIC {
                consume_instr!(UnkF6, instrs, pe, offset);

            } else if code[0] == UnkJumpIfLowDetail::MAGIC {
                consume_instr!(UnkJumpIfLowDetail, instrs, pe, offset);

            } else if code[0] == Unk40::MAGIC {
                consume_instr!(Unk40, instrs, pe, offset);

            } else if code[0] == TextureRef::MAGIC {
                consume_instr!(TextureRef, instrs, pe, offset);

            } else if code[0] == TextureIndex::MAGIC {
                consume_instr!(TextureIndex, instrs, pe, offset);

            } else if code[0] == SourceRef::MAGIC {
                consume_instr!(SourceRef, instrs, pe, offset);

            } else if code[0] == VertexBuf::MAGIC {
                consume_instr!(VertexBuf, instrs, pe, offset);

            } else if code[0] == Facet::MAGIC {
                consume_instr!(Facet, instrs, pe, offset);

            } else if code[0] == X86Code::MAGIC {
                let mut name = "unknown_source".to_owned();
                {
                    if let Some(&Instr::SourceRef(ref source)) = find_first_instr(0x42, &instrs) {
                        name = source.source.clone();
                    }
                }
                //consume_instr!(X86Code, instrs, pe, offset);
                let x86 = X86Code::from_bytes(name, offset, pe)?;
                let sz = x86.size();
                instrs.push(Instr::X86Code(x86));
                offset += sz;

            } else {
                // Trailer / Unknown remaining.
                consume_instr!(TrailerUnknown, instrs, pe, offset);

                break;
            }
        }

        // Assertions.
        {
            let instr = find_first_instr(0xF2, &instrs);
            if let Some(&Instr::UnkJumpIfNotShown(ref jmp)) = instr {
                let tgt = find_instr_at_offset(jmp.next_offset(), &instrs);
                //assert!(tgt.is_some());
            }
        }

        let mut shape = CpuShape { source: "".to_owned(), instrs: instrs };
        return Ok(shape);
    }
}

fn find_first_instr(kind: u8, instrs: &[Instr]) -> Option<&Instr> {
    for instr in instrs.iter() {
        match kind {
            0xF2 => {
                if let &Instr::UnkJumpIfNotShown(ref _x) = instr {
                    return Some(instr);
                }
            }
            0x42 => {
                if let &Instr::SourceRef(ref _x) = instr {
                    return Some(instr);
                }
            }
            _ => {}
        }
    }
    return None;
}

fn find_instr_at_offset(offset: usize, instrs: &[Instr]) -> Option<&Instr> {
    for instr in instrs.iter() {
        if instr.at_offset() == offset {
            return Some(instr);
        }
    }
    return None;

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
            println!("AT: {}", path);

            //if path == "./test_data/MIG21.SH" {
            if true {

                let mut fp = fs::File::open(entry.path()).unwrap();
                let mut data = Vec::new();
                fp.read_to_end(&mut data).unwrap();

                let shape = CpuShape::new(&data).unwrap();
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
