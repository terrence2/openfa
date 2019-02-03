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
#![cfg_attr(feature = "cargo-clippy", allow(transmute_ptr_to_ptr))]

extern crate bitflags;
extern crate failure;
extern crate i386;
extern crate lazy_static;
extern crate log;
extern crate peff;
extern crate reverse;
extern crate simplelog;

use bitflags::bitflags;
use failure::{bail, ensure, Fail, Fallible};
use lazy_static::lazy_static;
use log::{info, trace};
use reverse::{bs2s, p2s, Color, Escape};
use std::{cmp, collections::HashSet, fmt, mem, str};

#[derive(Debug, Fail)]
enum ShError {
    #[fail(display = "name ran off end of file")]
    NameUnending {},
}

const SHAPE_LOAD_BASE: u32 = 0xAA00_0000;

lazy_static! {
    // Virtual instructions that have a one-byte header instead of
    static ref ONE_BYTE_MAGIC: HashSet<u8> =
        { [0x1E, 0x66, 0xFC, 0xFF].iter().cloned().collect() };

    pub static ref DATA_RELOCATIONS: HashSet<String> = {
        [
            "_currentTicks",
            "_effectsAllowed",
            "brentObjId",
            "viewer_x",
            "viewer_z",
            "xv32",
            "zv32",
        ].iter()
            .map(|&n| n.to_owned())
            .collect()
    };
}

/// A version of the shape for slicing/dicing on the CPU for exploration. The normal
/// load path will go straight into GPU buffers.
pub struct CpuShape {
    pub instrs: Vec<Instr>,
    pub trampolines: Vec<X86Trampoline>,
    pub pe: peff::PE,
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

    pub fn to_u16(self) -> u16 {
        unsafe { mem::transmute(self) }
    }
}

fn read_name(n: &[u8]) -> Fallible<String> {
    let end_offset: usize = n
        .iter()
        .position(|&c| c == 0)
        .ok_or::<ShError>(ShError::NameUnending {})?;
    Ok(str::from_utf8(&n[..end_offset])?.to_owned())
}

#[derive(Debug)]
pub struct TextureRef {
    pub offset: usize,
    pub filename: String,
}

impl TextureRef {
    pub const MAGIC: u8 = 0xE2;
    pub const SIZE: usize = 16;

    fn from_bytes(offset: usize, code: &[u8]) -> Fallible<Self> {
        let data = &code[offset..];
        assert_eq!(data[0], Self::MAGIC);
        assert_eq!(data[1], 0);
        let filename = read_name(&data[2..Self::SIZE])?;
        Ok(TextureRef { offset, filename })
    }

    fn size(&self) -> usize {
        Self::SIZE
    }

    fn magic(&self) -> &'static str {
        "E2"
    }

    fn at_offset(&self) -> usize {
        self.offset
    }

    fn show(&self) -> String {
        format!(
            "@{:04X} {}TexRf{}: {}{}{}",
            self.offset,
            Escape::new().fg(Color::Yellow).bold(),
            Escape::new(),
            Escape::new().fg(Color::Yellow),
            self.filename,
            Escape::new(),
        )
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
    fn from_u16(kind: u16) -> Fallible<Self> {
        match kind {
            0 => Ok(TextureIndexKind::TailLeft),
            1 => Ok(TextureIndexKind::TailRight),
            2 => Ok(TextureIndexKind::Nose),
            3 => Ok(TextureIndexKind::WingLeft),
            4 => Ok(TextureIndexKind::WingRight),
            _ => bail!("unknown texture index"),
        }
    }
}

#[derive(Debug)]
pub struct TextureIndex {
    pub offset: usize,
    pub unk0: u8,
    pub kind: TextureIndexKind,
}

impl TextureIndex {
    pub const MAGIC: u8 = 0xE0;
    pub const SIZE: usize = 4;

    fn from_bytes(offset: usize, code: &[u8]) -> Fallible<Self> {
        let data = &code[offset..];
        assert_eq!(data[0], Self::MAGIC);
        let data2: &[u16] = unsafe { mem::transmute(&data[2..]) };
        Ok(TextureIndex {
            offset,
            unk0: data[1],
            kind: TextureIndexKind::from_u16(data2[0])?,
        })
    }

    fn size(&self) -> usize {
        Self::SIZE
    }

    fn magic(&self) -> &'static str {
        "E0"
    }

    fn at_offset(&self) -> usize {
        self.offset
    }

    fn show(&self) -> String {
        format!(
            "TextureIndexKind @ {:04X}: {}, {:?}",
            self.offset, self.unk0, self.kind
        )
    }
}

#[derive(Debug)]
pub struct SourceRef {
    pub offset: usize,
    pub source: String,
}

impl SourceRef {
    pub const MAGIC: u8 = 0x42;

    fn from_bytes(offset: usize, code: &[u8]) -> Fallible<Self> {
        let data = &code[offset..];
        assert_eq!(data[0], Self::MAGIC);
        ensure!(data[1] == 0x00, "unexpected non-nil in hi");
        let source = read_name(&data[2..])?;
        Ok(SourceRef { offset, source })
    }

    fn size(&self) -> usize {
        2 + self.source.len() + 1
    }

    fn magic(&self) -> &'static str {
        "42"
    }

    fn at_offset(&self) -> usize {
        self.offset
    }

    fn show(&self) -> String {
        format!(
            "@{:04X} {}SrcRf{}: {}{}{}",
            self.offset,
            Escape::new().fg(Color::Yellow).bold(),
            Escape::new(),
            Escape::new().fg(Color::Yellow),
            self.source,
            Escape::new(),
        )
    }
}

#[derive(Debug)]
pub struct VertexBuf {
    pub offset: usize,
    pub unk0: u16,
    pub raw_verts: Vec<[u16; 3]>,
    pub verts: Vec<[f32; 3]>,
}

impl VertexBuf {
    pub const MAGIC: u8 = 0x82;

    fn from_bytes(offset: usize, code: &[u8]) -> Fallible<Self> {
        let data = &code[offset..];
        assert_eq!(data[0], Self::MAGIC);
        assert_eq!(data[1], 0);
        let head: &[u16] = unsafe { mem::transmute(&data[2..6]) };
        let words: &[u16] = unsafe { mem::transmute(&data[6..]) };
        let mut buf = VertexBuf {
            offset,
            unk0: head[2],
            raw_verts: Vec::new(),
            verts: Vec::new(),
        };
        fn s2f(s: u16) -> f32 {
            f32::from(s as i16)
        }
        let nverts = head[0] as usize;
        for i in 0..nverts {
            let x = s2f(words[i * 3]);
            let y = s2f(words[i * 3 + 1]);
            let z = s2f(words[i * 3 + 2]);
            buf.verts.push([x, y, z]);

            let x = words[i * 3];
            let y = words[i * 3 + 1];
            let z = words[i * 3 + 2];
            buf.raw_verts.push([x, y, z]);
        }
        Ok(buf)
    }

    fn size(&self) -> usize {
        6 + self.verts.len() * 6
    }

    fn magic(&self) -> &'static str {
        "VertexBuf(82)"
    }

    fn at_offset(&self) -> usize {
        self.offset
    }

    fn show(&self) -> String {
        let s = self
            .raw_verts
            .iter()
            .map(|v| format!("({:04X},{:04X},{:04X})", v[0], v[1], v[2]))
            .collect::<Vec<String>>()
            .join(", ");
        format!(
            "@{:04X} {}VxBuf: 82 00{}| {}{} ({:b}){} => {}verts -> {}{}{}",
            self.offset,
            Escape::new().fg(Color::Magenta).bold(),
            Escape::new(),
            Escape::new().fg(Color::Magenta),
            self.unk0,
            self.unk0,
            Escape::new(),
            self.verts.len(),
            Escape::new().fg(Color::Magenta).dimmed(),
            s,
            Escape::new(),
        )
    }
}

#[derive(Debug)]
pub struct Facet {
    pub offset: usize,
    pub length: usize,
    pub flags: FacetFlags,
    pub mat_desc: String,
    pub indices: Vec<u16>,
    pub max_index: u16,
    pub min_index: u16,
    pub tex_coords: Vec<[u16; 2]>,
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
    fn from_bytes(offset: usize, code: &[u8]) -> Fallible<Self> {
        let data = &code[offset..];
        assert_eq!(data[0], Self::MAGIC);

        let flags_word = (u16::from(data[1]) << 8) | u16::from(data[2]);
        assert_eq!(flags_word & 0x00F0, 0u16);
        let flags = FacetFlags::from_u16(flags_word);

        let mut off = 3;

        // Material
        let material_size = if flags.contains(FacetFlags::HAVE_MATERIAL) {
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
        let index_u16: &[u16] = unsafe { mem::transmute(index_u8) };
        for i in 0..index_count {
            let index = if flags.contains(FacetFlags::USE_SHORT_INDICES) {
                off += 2;
                index_u16[i]
            } else {
                off += 1;
                u16::from(index_u8[i])
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
                    (u16::from(tc_u8[i * 2]), u16::from(tc_u8[i * 2 + 1]))
                } else {
                    off += 4;
                    (tc_u16[i * 2], tc_u16[i * 2 + 1])
                };
                tex_coords.push([u, v]);
            }

            assert_eq!(tex_coords.len(), indices.len());
        }

        Ok(Facet {
            offset,
            length: off,
            flags,
            mat_desc,
            max_index: *indices.iter().max().unwrap(),
            min_index: *indices.iter().min().unwrap(),
            indices,
            tex_coords,
        })
    }

    fn size(&self) -> usize {
        self.length
    }

    fn magic(&self) -> &'static str {
        "Facet(FC)"
    }

    fn at_offset(&self) -> usize {
        self.offset
    }

    fn show(&self) -> String {
        let mut flags = format!("{:016b}", self.flags)
            .chars()
            .collect::<Vec<char>>();
        flags[8] = 'z';
        flags[9] = 'z';
        flags[10] = 'z';
        flags[11] = 'z';
        flags[5] = 'x';
        flags[13] = 'x';
        flags[14] = 'x';
        flags[14] = 'x';
        // const HAVE_MATERIAL      = 0b0100_0000_0000_0000;
        // const HAVE_TEXCOORDS     = 0b0000_0100_0000_0000;
        // const USE_SHORT_INDICES  = 0b0000_0000_0000_0100;
        // const USE_SHORT_MATERIAL = 0b0000_0000_0000_0010;
        // const USE_BYTE_TEXCOORDS = 0b0000_0000_0000_0001;
        // const UNK_MATERIAL_RELATED = 0b0000_0001_0000_0000;
        let ind = self
            .indices
            .iter()
            .map(|i| format!("{:X}", i))
            .collect::<Vec<String>>()
            .join(", ");
        format!(
            "@{:04X} {}Facet: FC{}   | {}{}{} - {}{}{} - [{}{}{}] - {}{:?}{}",
            self.offset,
            Escape::new().fg(Color::Cyan).bold(),
            Escape::new(),
            Escape::new().fg(Color::Cyan),
            flags.iter().collect::<String>(),
            Escape::new(),
            Escape::new().fg(Color::Cyan).dimmed(),
            self.mat_desc,
            Escape::new(),
            Escape::new().fg(Color::Cyan),
            ind,
            Escape::new(),
            Escape::new().fg(Color::Cyan),
            self.tex_coords,
            Escape::new(),
        )
    }
}

#[derive(Clone, Debug)]
pub struct X86Trampoline {
    pub offset: usize,

    // The name attached to the thunk that would populate this trampoline.
    pub name: String,

    // Where this trampoline would indirect to, if called.
    pub target: u32,

    // The jump or data reference that will be baked into code to call or load
    // from this trampoline (e.g. offset but including the relocation).
    pub location: u32,

    // Whatever tool was used to link .SH's bakes in a direct pointer to the GOT
    // PLT base (e.g. target) as the data location. Presumably when doing
    // runtime linking, it uses the IAT's name as a tag and rewrites the direct
    // load to the real address of the symbol (and not a split read of the code
    // and reloc in the GOT). These appear to be both global and per-object data
    // depending on the data -- e.g. brentObjectId is probably per-object and
    // _currentTicks is probably global?
    //
    // A concrete example; if the inline assembly does:
    //    `mov %ebp, [<addr of GOT of data>]`
    //
    // The runtime would presumably use the relocation of the above addr as an
    // opportunity to rewrite the load as a reference to the real memory. We
    // need to take all of this into account when interpreting the referencing
    // code.
    pub is_data: bool,
}

impl X86Trampoline {
    const SIZE: usize = 6;

    fn from_pe(offset: usize, pe: &peff::PE) -> Fallible<Self> {
        ensure!(
            pe.code.len() >= offset + 6,
            "not enough code in X86Trampoline::from_pe"
        );
        ensure!(
            pe.code[offset] == 0xFF && pe.code[offset + 1] == 0x25,
            "not at a trampoline"
        );
        ensure!(
            pe.section_info.contains_key(".idata"),
            "no .idata section (thunks) in pe"
        );
        let target = {
            let vp: &[u32] = unsafe { mem::transmute(&pe.code[offset + 2..offset + 6]) };
            vp[0]
        };
        let name = Self::find_matching_thunk(target, pe)?;
        let is_data = DATA_RELOCATIONS.contains(&name);
        Ok(X86Trampoline {
            offset,
            name,
            target,
            location: SHAPE_LOAD_BASE + offset as u32,
            is_data,
        })
    }

    fn find_matching_thunk(target: u32, pe: &peff::PE) -> Fallible<String> {
        trace!(
            "looking for target 0x{:X} in {} thunks",
            target,
            pe.thunks.len()
        );
        for thunk in pe.thunks.iter() {
            trace!("    {:20} @ 0x{:X}", thunk.name, thunk.vaddr);
            if target == thunk.vaddr {
                return Ok(thunk.name.clone());
            }
        }
        // Not all SH files contain relocations for the thunk targets(!). This
        // is yet more evidence that they're not actually using LoadLibrary to
        // put shapes in memory. They're probably only using the relocation list
        // to rewrite data access with the thunks as tags. We're using
        // relocation, however, to help debug. So if the thunks are not
        // relocated automatically we have to check the relocated value
        // manually.
        let target = pe.relocate_pointer(target);
        for thunk in pe.thunks.iter() {
            if target == thunk.vaddr {
                return Ok(thunk.name.clone());
            }
        }
        bail!("did not find thunk with a target of {:08X}", target)
    }

    fn size(&self) -> usize {
        6
    }

    fn magic(&self) -> &'static str {
        "Tramp"
    }

    fn at_offset(&self) -> usize {
        self.offset
    }

    fn show(&self) -> String {
        format!(
            "@{:04X} {}Tramp{}: {}{}{} = {:04X}",
            self.offset,
            Escape::new().fg(Color::Yellow).bold(),
            Escape::new(),
            Escape::new().fg(Color::Yellow),
            self.name,
            Escape::new(),
            self.target
        )
    }
}

#[derive(Debug)]
pub struct X86Code {
    pub offset: usize,
    pub length: usize,
    pub code_offset: usize,
    pub code: Vec<u8>,
    pub formatted: String,
    pub bytecode: i386::ByteCode,
    pub have_header: bool,
}

impl X86Code {
    pub const MAGIC: u8 = 0xF0;

    // External, but excluding trampolines.
    fn find_external_jumps(
        base: usize,
        pe: &peff::PE,
        bc: &i386::ByteCode,
        external_jumps: &mut HashSet<usize>,
    ) {
        Self::find_external_relative(base, pe, bc, external_jumps);
        //Self::find_external_absolute(base, pe, bc, external_jumps);
    }

    fn instr_is_relative_jump(instr: &i386::Instr) -> bool {
        match instr.memonic {
            i386::Memonic::Call => true,
            i386::Memonic::Jump => true,
            i386::Memonic::Jcc(ref _cc) => true,
            _ => false,
        }
    }

    fn operand_to_offset(op: &i386::Operand) -> usize {
        match op {
            i386::Operand::Imm32s(delta) => {
                if *delta < 0 {
                    info!("Skipping loop of {} bytes", *delta);
                    0
                } else {
                    *delta as u32 as usize
                }
            }
            i386::Operand::Imm32(delta) => *delta as usize,
            _ => {
                trace!("Detected indirect jump target: {}", op);
                0
            }
        }
    }

    fn find_external_relative(
        base: usize,
        _pe: &peff::PE,
        bc: &i386::ByteCode,
        external_jumps: &mut HashSet<usize>,
    ) {
        let mut ip = 0;
        let ip_end = bc.size as usize;
        for instr in bc.instrs.iter() {
            ip += instr.size();
            if Self::instr_is_relative_jump(&instr) {
                let delta = Self::operand_to_offset(&instr.operands[0]);
                let ip_target = ip + delta;
                if ip_target >= ip_end {
                    external_jumps.insert(base + ip_target);
                }
            };
        }
    }

    /*
    fn find_external_absolute(
        _base: usize,
        pe: &peff::PE,
        bc: &i386::ByteCode,
        external_jumps: &mut HashSet<usize>,
    ) {
        for s in bc.instrs.windows(2) {
            match s {
                [ref a, ref b] => {
                    if a.memonic == i386::Memonic::Push && b.memonic == i386::Memonic::Return {
                        if let i386::Operand::Imm32s(delta) = a.operands[0] {
                            let absolute_ip = (delta as u32).checked_sub(pe.code_addr).unwrap_or(0);
                            external_jumps.insert(absolute_ip as usize);
                        }
                    }
                }
                _ => panic!("expected 2 elements in window"),
            }
        }
    }
    */

    fn lowest_jump(jumps: &HashSet<usize>) -> usize {
        let mut lowest = usize::max_value();
        for &jump in jumps {
            if jump < lowest {
                lowest = jump;
            }
        }
        lowest
    }

    /*
    fn from_bytes(
        _name: &str,
        offset: &mut usize,
        pe: &peff::PE,
        trampolines: &[X86Trampoline],
        vinstrs: &mut Vec<Instr>,
    ) -> Fallible<()> {
        let section = &pe.code[*offset..];
        assert_eq!(section[0], Self::MAGIC);
        assert_eq!(section[1], 0);
        *offset += 2;

        // Decode the block of instructions up to ret.
        let code = &pe.code[*offset..];
        let maybe_bc =
            i386::ByteCode::disassemble_to_ret(SHAPE_LOAD_BASE as usize + *offset, code);
        if let Err(e) = maybe_bc {
            i386::DisassemblyError::maybe_show(&e, &pe.code[*offset..]);
            bail!("Don't know how to disassemble at {}", *offset);
        }
        let bc = maybe_bc?;

        // Insert the instruction.
        let bc_size = bc.size as usize;
        let have_header = pe.code[*offset - 2] == 0xF0;
        let section_offset = if have_header { *offset - 2 } else { *offset };
        let section_length = bc_size + if have_header { 2 } else { 0 };
        vinstrs.push(Instr::X86Code(X86Code {
            offset: section_offset,
            length: section_length,
            code_offset: *offset,
            code: code[0..bc_size].to_owned(),
            formatted: Self::format_section(section_offset, section_length, &bc, pe),
            bytecode: bc,
            have_header,
        }));
        *offset += bc_size;

        // If the next block is an interpreter instruction...
    }
    */

    fn from_bytes(
        _name: &str,
        offset: &mut usize,
        pe: &peff::PE,
        trampolines: &[X86Trampoline],
        vinstrs: &mut Vec<Instr>,
    ) -> Fallible<()> {
        let section = &pe.code[*offset..];
        assert_eq!(section[0], Self::MAGIC);
        assert_eq!(section[1], 0);
        *offset += 2;

        // Seed external jumps with our implicit F0 section jump.
        let mut external_jumps = HashSet::new();
        external_jumps.insert(*offset);

        while !external_jumps.is_empty() {
            trace!(
                "We are currently at {} with external jumps: {:?}",
                *offset,
                external_jumps
            );
            if external_jumps.contains(&offset) {
                external_jumps.remove(&offset);
                trace!("IP REACHED EXTERNAL JUMP");

                let code = &pe.code[*offset..];
                let maybe_bc =
                    i386::ByteCode::disassemble_to_ret(SHAPE_LOAD_BASE as usize + *offset, code);
                if let Err(e) = maybe_bc {
                    i386::DisassemblyError::maybe_show(&e, &pe.code[*offset..]);
                    bail!("Don't know how to disassemble at {}", *offset);
                }
                let bc = maybe_bc?;

                // Dump all decoded segments for testing the disassembler.
                if false {
                    use std::{fs::File, io::Write};
                    let src = match find_first_instr(0x42, vinstrs) {
                        Some(Instr::SourceRef(ref sr)) => sr.source.clone(),
                        _ => "unknown".to_owned(),
                    };
                    let actual_code = &pe.code[*offset..*offset + bc.size as usize];
                    let mut file =
                        File::create(&format!("../i386/test_data/{}-{}.x86", src, *offset))?;
                    file.write_all(actual_code)?;
                }

                trace!("decoded {} instructions", bc.instrs.len());
                Self::find_external_jumps(*offset, pe, &bc, &mut external_jumps);
                trace!("new external jumps: {:?}", external_jumps);

                // Insert the instruction.
                let bc_size = bc.size as usize;
                let have_header = pe.code[*offset - 2] == 0xF0;
                let section_offset = if have_header { *offset - 2 } else { *offset };
                let section_length = bc_size + if have_header { 2 } else { 0 };
                vinstrs.push(Instr::X86Code(X86Code {
                    offset: section_offset,
                    length: section_length,
                    code_offset: *offset,
                    code: code[0..bc_size].to_owned(),
                    formatted: Self::format_section(section_offset, section_length, &bc, pe),
                    bytecode: bc,
                    have_header,
                }));
                *offset += bc_size;

                // The block after the ret may be a word code, or a data block, or another
                // instruction for us to decode.
                if pe.code[*offset + 1] != 0 {
                    trace!("trying next offset");
                    let maybe_bc = i386::ByteCode::disassemble_one(SHAPE_LOAD_BASE as usize + *offset, &pe.code[*offset..]);
                    if let Err(e) = maybe_bc {
                        trace!("offset after RET is probably external data; check this to see if it might actually be bytecode");
                        i386::DisassemblyError::maybe_show(&e, &pe.code[*offset..]);
                    } else {
                        // Create an external jump to ourself to continue decoding.
                        external_jumps.insert(*offset);
                    }
                }

                if external_jumps.is_empty() {
                    /*
                    trace!("trying next offset");
                    let maybe_bc = i386::ByteCode::disassemble_one(SHAPE_LOAD_BASE as usize + *offset, &pe.code[*offset..]);
                    if let Err(e) = maybe_bc {
                        trace!("offset after RET is not code");
                        i386::DisassemblyError::maybe_show(&e, &pe.code[*offset..]);
                        return Ok(());
                    } else {
                        external_jumps.insert(*offset);
                    }
                    */
                    return Ok(());
                }
            }

            // If we have no more jumps, continue looking for instructions.
            if external_jumps.is_empty()
                || Self::lowest_jump(&external_jumps) < *offset
                || *offset >= pe.code.len()
            {
                break;
            }

            // Otherwise, we are between code segments. There may be instructions
            // here, or maybe just some raw data. Look for an instruction and if
            // there is one, decode it. Otherwise treat it as raw data.

            // We do not expect another F0 while we have external jumps to find.
            let saved_offset = *offset;
            let mut have_raw_data = false;
            let maybe = CpuShape::read_instr(offset, pe, trampolines, vinstrs);
            if let Err(_e) = maybe {
                have_raw_data = true;
            } else if let Some(&Instr::UnknownUnknown(_)) = vinstrs.last() {
                vinstrs.pop();
                *offset = saved_offset;
                have_raw_data = true;
            } else if let Some(&Instr::TrailerUnknown(_)) = vinstrs.last() {
                //bail!("found trailer while we still have external jumps to track down")
                return Ok(());
            }

            if have_raw_data && *offset < Self::lowest_jump(&external_jumps) {
                // There is no instruction here, so assume data. Find the closest jump
                // target remaining and fast-forward there.
                // FIXME: insert this as an instruction?
                trace!(
                    "Adding data block @{:04X}: {}",
                    *offset,
                    bs2s(&pe.code[*offset..cmp::min(pe.code.len(), *offset + 80)])
                );
                let end = Self::lowest_jump(&external_jumps);
                vinstrs.push(Instr::UnknownData(UnknownData {
                    offset: *offset,
                    length: end - *offset,
                    data: pe.code[*offset..end].to_vec(),
                }));
                *offset = end;
            }
        }

        Ok(())
    }

    fn format_section(offset: usize, length: usize, _bc: &i386::ByteCode, pe: &peff::PE) -> String {
        let sec = reverse::Section::new(0xF0, offset, length);
        let tags = reverse::get_all_tags(pe);
        let mut v = Vec::new();
        reverse::accumulate_section(&pe.code, &sec, &tags, &mut v);
        v.iter().collect::<String>()
    }

    fn size(&self) -> usize {
        self.code.len() + 2
    }

    fn magic(&self) -> &'static str {
        "F0"
    }

    fn at_offset(&self) -> usize {
        self.offset
    }

    fn show(&self) -> String {
        let show_offset = if self.have_header {
            self.offset + 2
        } else {
            self.offset
        };
        format!(
            "@{:04X} X86Code: {}\n  {}",
            self.offset,
            self.formatted,
            self.bytecode.show_relative(show_offset).trim()
        )
    }
}

pub struct UnkCE {
    pub offset: usize,
    pub data: [u8; 40 - 2],
}

impl UnkCE {
    pub const MAGIC: u8 = 0xCE;
    pub const SIZE: usize = 40;

    fn from_bytes(offset: usize, code: &[u8]) -> Fallible<Self> {
        let data = &code[offset..];
        assert_eq!(data[0], Self::MAGIC);
        assert_eq!(data[1], 0);
        // Note: no default for arrays larger than 32 elements.
        let s = &data[2..];
        Ok(Self {
            offset,
            data: [
                s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7], s[8], s[9], s[10], s[11], s[12],
                s[13], s[14], s[15], s[16], s[17], s[18], s[19], s[20], s[21], s[22], s[23], s[24],
                s[25], s[26], s[27], s[28], s[29], s[30], s[31], s[32], s[33], s[34], s[35], s[36],
                s[37],
            ],
        })
    }

    fn size(&self) -> usize {
        Self::SIZE
    }

    fn magic(&self) -> &'static str {
        "CE"
    }

    fn at_offset(&self) -> usize {
        self.offset
    }

    fn show(&self) -> String {
        format!("UnkCE @ {:04X}: {}", self.offset, bs2s(&self.data))
    }
}

impl fmt::Debug for UnkCE {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "UnkCE @{:04X}: {}", self.offset, bs2s(&self.data))
    }
}

#[derive(Debug)]
pub struct UnkBC {
    pub offset: usize,
    pub unk_header: u8,
    data: *const u8,
}

impl UnkBC {
    pub const MAGIC: u8 = 0xBC;

    fn from_bytes(offset: usize, code: &[u8]) -> Fallible<Self> {
        let data = &code[offset..];
        assert_eq!(data[0], Self::MAGIC);

        let unk_header = data[1];
        Ok(UnkBC {
            offset,
            unk_header,
            data: data.as_ptr(),
        })
    }

    fn size(&self) -> usize {
        2
    }

    fn magic(&self) -> &'static str {
        "BC"
    }

    fn at_offset(&self) -> usize {
        self.offset
    }

    pub fn show(&self) -> String {
        format!(
            "@{:04X} {}UnkBC{}: {}{}{}| (hdr:{:02X})",
            self.offset,
            Escape::new().fg(Color::Red).bold(),
            Escape::new(),
            Escape::new().fg(Color::Red).bold(),
            p2s(self.data, 0, 2).trim(),
            Escape::new(),
            self.unk_header,
        )
    }
}

/*
#[derive(Debug)]
pub struct UnkBC {
    pub offset: usize,
    unk_header: u8,
    flags: u8,
    unk0: u8,
    length: usize,
    data: *const u8,
}

impl UnkBC {
    pub const MAGIC: u8 = 0xBC;

    fn from_bytes(offset: usize, code: &[u8]) -> Fallible<Self> {
        let data = &code[offset..];
        assert_eq!(data[0], Self::MAGIC);

        let unk_header = data[1];
        let flags = data[2];
        let unk0 = data[3];
        let length = match flags {
            0x96 => 8,
            0x72 => 6,
            0x68 => 10,
            0x08 => 6,
            _ => bail!("unknown section BC flags: {}", flags),
        };
        Ok(UnkBC {
            offset,
            unk_header,
            flags,
            unk0,
            length,
            //data: data[4..length].to_owned(),
            data: data.as_ptr(),
        })
    }

    fn size(&self) -> usize {
        self.length
    }

    fn magic(&self) -> &'static str {
        "BC"
    }

    fn at_offset(&self) -> usize {
        self.offset
    }

    pub fn show(&self) -> String {
        format!(
            "@{:04X} {}UnkBC{}: {}{}{}   | {}{}{} (hdr:{:02X}, flags:{:02X}, ?unk0?:{:02X})",
            self.offset,
            Escape::new().fg(Color::Red).bold(),
            Escape::new(),
            Escape::new().fg(Color::Red).bold(),
            p2s(self.data, 0, 1).trim(),
            Escape::new(),
            Escape::new().fg(Color::Red),
            p2s(self.data, 1, self.length),
            Escape::new(),
            self.unk_header,
            self.flags,
            self.unk0,
        )
    }
}
*/

#[derive(Debug)]
pub struct Unk40 {
    pub offset: usize,
    count: usize,
    length: usize,
    data: Vec<u16>,
}

impl Unk40 {
    pub const MAGIC: u8 = 0x40;

    // 40 00   04 00   08 00, 25 00, 42 00, 5F 00
    fn from_bytes(offset: usize, code: &[u8]) -> Fallible<Self> {
        let data = &code[offset..];
        assert_eq!(data[0], Self::MAGIC);
        assert_eq!(data[1], 0x00);
        let words: &[u16] = unsafe { mem::transmute(&data[2..]) };
        let count = words[0] as usize;
        let length = 4 + count * 2;
        let data = words[1..=count].to_owned();
        Ok(Unk40 {
            offset,
            count,
            length,
            data,
        })
    }

    fn size(&self) -> usize {
        self.length
    }

    fn magic(&self) -> &'static str {
        "40"
    }

    fn at_offset(&self) -> usize {
        self.offset
    }

    fn show(&self) -> String {
        format!(
            "Unk40 @ {:04X}: cnt:{}, len:{}, data:{:?}",
            self.offset, self.count, self.length, self.data
        )
    }
}

#[derive(Debug)]
pub struct UnkF6 {
    pub offset: usize,
    data: *const u8,
}

impl UnkF6 {
    pub const MAGIC: u8 = 0xF6;
    pub const SIZE: usize = 7;

    fn from_bytes(offset: usize, code: &[u8]) -> Fallible<Self> {
        let data = &code[offset..];
        assert_eq!(data[0], Self::MAGIC);
        Ok(Self {
            offset,
            data: data.as_ptr(),
        })
    }

    fn size(&self) -> usize {
        Self::SIZE
    }

    fn magic(&self) -> &'static str {
        "F6"
    }

    fn at_offset(&self) -> usize {
        self.offset
    }

    pub fn show(&self) -> String {
        format!(
            "@{:04X} {}UnkF6{}: {}{}{}   | {}{}{}",
            self.offset,
            Escape::new().fg(Color::Red).bold(),
            Escape::new(),
            Escape::new().fg(Color::Red).bold(),
            p2s(self.data, 0, 1).trim(),
            Escape::new(),
            Escape::new().fg(Color::Red),
            p2s(self.data, 1, Self::SIZE),
            Escape::new(),
        )
    }
}

#[derive(Debug)]
pub struct Unk38 {
    pub offset: usize,
    pub unk0: usize,
    data: *const u8,
}

// I think this probably means something like: fastforward to "next_offset" if
// we are running in low-detail mode. Seems to skip nose art and a couple random
// polys in F22, a big rock in ROCKB, and the textured polys in BUNK.
//
// This probably works with UNKC8 somehow: in BUNK
impl Unk38 {
    pub const MAGIC: u8 = 0x38;
    pub const SIZE: usize = 3;

    fn from_bytes(offset: usize, code: &[u8]) -> Fallible<Self> {
        let data = &code[offset..];
        assert_eq!(data[0], Self::MAGIC);
        let word_ref: &[u16] = unsafe { mem::transmute(&data[1..]) };
        let unk0 = word_ref[0] as usize;
        Ok(Self {
            offset,
            unk0,
            data: data.as_ptr(),
        })
    }

    fn size(&self) -> usize {
        Self::SIZE
    }

    fn magic(&self) -> &'static str {
        "38"
    }

    fn at_offset(&self) -> usize {
        self.offset
    }

    pub fn show(&self) -> String {
        format!(
            "@{:04X} {}Unk38{}: {}{}{}   | {}{}{} (?unk0?:{:04X})",
            self.offset,
            Escape::new().fg(Color::Red).bold(),
            Escape::new(),
            Escape::new().fg(Color::Red).bold(),
            p2s(self.data, 0, 1).trim(),
            Escape::new(),
            Escape::new().fg(Color::Red),
            p2s(self.data, 1, Self::SIZE),
            Escape::new(),
            self.unk0,
        )
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

#[derive(Debug)]
#[allow(non_camel_case_types)]
pub struct F2_JumpIfNotShown {
    pub offset: usize,
    data: *const u8,
    pub offset_to_next: usize,
}

impl F2_JumpIfNotShown {
    pub const MAGIC: u8 = 0xF2;
    pub const SIZE: usize = 4;

    fn from_bytes(offset: usize, code: &[u8]) -> Fallible<Self> {
        let data = &code[offset..];
        assert_eq!(data[0], Self::MAGIC);
        assert_eq!(data[1], 0x00);
        let word_ref: &[u16] = unsafe { mem::transmute(&data[2..]) };
        let offset_to_next = word_ref[0] as usize;
        Ok(Self {
            offset,
            data: data.as_ptr(),
            offset_to_next,
        })
    }

    fn size(&self) -> usize {
        Self::SIZE
    }

    fn magic(&self) -> &'static str {
        "F2"
    }

    fn at_offset(&self) -> usize {
        self.offset
    }

    pub fn next_offset(&self) -> usize {
        // Our start offset + our size + offset_to_next.
        self.offset + Self::SIZE + self.offset_to_next
    }

    pub fn show(&self) -> String {
        format!(
            "@{:04X} {}UnkF2{}: {}{}{}| {}{}{} (delta:{:04X}, target:{:04X})",
            self.offset,
            Escape::new().fg(Color::BrightBlue).bold(),
            Escape::new(),
            Escape::new().fg(Color::BrightBlue).bold(),
            p2s(self.data, 0, 2).trim(),
            Escape::new(),
            Escape::new().fg(Color::BrightBlue),
            p2s(self.data, 2, Self::SIZE),
            Escape::new(),
            self.offset_to_next,
            self.next_offset()
        )
    }
}

#[derive(Debug)]
#[allow(non_camel_case_types)]
pub struct UnkC8_JumpOnDetailLevel {
    pub offset: usize,
    pub unk0: u16,
    pub unk1: u16,
    pub offset_to_next: usize,
    data: *const u8,
}

impl UnkC8_JumpOnDetailLevel {
    pub const MAGIC: u8 = 0xC8;
    pub const SIZE: usize = 8;

    // Unk1 Values are used in files:
    // 4+5  chaff/crater/flare/smoke
    // 6    rock + vom
    // 9    rocks
    // A    guide
    // C    moth+que+wtrbuf  (not in the game?)
    // F    bridges, docks, and rigs
    // 10   planes... all of them
    // 12   vehicles, trees, buildings
    // 14   bullet, tracer, wtrbuf
    // 18   moth, que
    // 19   missles, buildings, bridges, mooses, ships... basically everything?
    // 1E   buildings, missiles, ships, etc.
    // 21   planes... all of them
    // distance at which it should be shown?

    fn from_bytes(offset: usize, code: &[u8]) -> Fallible<Self> {
        let data = &code[offset..];
        assert_eq!(data[0], Self::MAGIC);
        assert_eq!(data[1], 0x00);
        let word_ref: &[u16] = unsafe { mem::transmute(&data[2..]) };
        let unk0 = word_ref[0];
        let unk1 = word_ref[1];
        let offset_to_next = word_ref[2] as usize;
        Ok(Self {
            offset,
            unk0,
            unk1,
            offset_to_next,
            data: data[0..Self::SIZE].as_ptr(),
        })
    }

    fn size(&self) -> usize {
        Self::SIZE
    }

    fn magic(&self) -> &'static str {
        "C8"
    }

    fn at_offset(&self) -> usize {
        self.offset
    }

    pub fn next_offset(&self) -> usize {
        // Our start offset + our size + 1 + offset_to_next.
        self.offset + Self::SIZE + self.offset_to_next
    }

    pub fn show(&self) -> String {
        format!(
            "@{:04X} {}UnkC8{}: {}{}{}| {}{}{} (jump-on-detail-level: ?dist?:{:04X}, kind:{:04X} delta:{:04X}, target:{:04X})",
            self.offset,
            Escape::new().fg(Color::BrightBlue).bold(),
            Escape::new(),

            Escape::new().fg(Color::BrightBlue).bold(),
            p2s(self.data, 0, 2).trim(),
            Escape::new(),

            Escape::new().fg(Color::BrightBlue),
            p2s(self.data, 2, Self::SIZE),
            Escape::new(),

            self.unk0,
            self.unk1,
            self.offset_to_next,
            self.next_offset()
        )
    }
}

// EE 00 E4 00 04 00 00 00 96 00 00 00 C7 00 1D 00 C7 00 1D 00 96 00 7A 00 00 00 00 00 06 00

#[derive(Debug)]
pub struct TrailerUnknown {
    pub offset: usize,
    pub data: Vec<u8>,
}

impl TrailerUnknown {
    pub const MAGIC: u8 = 0x00;

    fn from_bytes(offset: usize, code: &[u8], trampolines: &[X86Trampoline]) -> Fallible<Self> {
        let data = &code[offset..code.len() - trampolines.len() * X86Trampoline::SIZE];
        Ok(Self {
            offset,
            data: data.to_owned(),
        })
    }

    fn size(&self) -> usize {
        self.data.len()
    }

    fn magic(&self) -> &'static str {
        "Trailer"
    }

    fn at_offset(&self) -> usize {
        self.offset
    }

    fn show(&self) -> String {
        use reverse::{format_sections, Section, ShowMode};
        let mut sections = Vec::new();
        let mut sec_start = self.offset;
        // make a first section to align to word boundary if needed
        if sec_start % 4 != 0 {
            let sec_end = self.offset + 4 - (sec_start % 4);
            sections.push(Section::new(
                0x0000,
                sec_start - self.offset,
                sec_end - sec_start,
            ));
            sec_start = sec_end;
        }

        while sec_start < self.offset + self.data.len() {
            let sec_end = cmp::min(sec_start + 16, self.offset + self.data.len());
            sections.push(Section::new(
                0x0000,
                sec_start - self.offset,
                sec_end - sec_start,
            ));
            //println!("{} => {}", sec_start - self.offset, sec_end - self.offset,);
            sec_start = sec_end;
        }

        let out = format_sections(&self.data, &sections, &mut vec![], &ShowMode::AllPerLine);
        let mut s = format!("Trailer @ {:04X}: {:6}b =>\n", self.offset, self.data.len(),);
        let mut off = 0;
        for (line, section) in out.iter().zip(sections) {
            s += &format!("  @{:02X}|{:04X}: {}\n", off, self.offset + off, line);
            off += section.length;
        }
        s
    }
}

#[derive(Debug)]
pub struct UnknownData {
    pub offset: usize,
    pub length: usize,
    pub data: Vec<u8>,
}

impl UnknownData {
    fn size(&self) -> usize {
        self.data.len()
    }

    fn magic(&self) -> &'static str {
        "Data"
    }

    fn at_offset(&self) -> usize {
        self.offset
    }

    fn show(&self) -> String {
        use reverse::{format_sections, Section, ShowMode};
        let mut sections = Vec::new();
        let mut sec_start = self.offset;
        // make a first section to align to word boundary if needed
        if sec_start % 4 != 0 {
            let sec_end = self.offset + 4 - (sec_start % 4);
            sections.push(Section::new(
                0x0000,
                sec_start - self.offset,
                sec_end - sec_start,
            ));
            sec_start = sec_end;
        }

        while sec_start < self.offset + self.data.len() {
            let sec_end = cmp::min(sec_start + 16, self.offset + self.data.len());
            sections.push(Section::new(
                0x0000,
                sec_start - self.offset,
                sec_end - sec_start,
            ));
            //println!("{} => {}", sec_start - self.offset, sec_end - self.offset,);
            sec_start = sec_end;
        }

        let out = format_sections(&self.data, &sections, &mut vec![], &ShowMode::AllPerLine);
        let mut s = format!("Unknown @ {:04X}: {:6}b =>\n", self.offset, self.data.len(),);
        let mut off = 0;
        for (line, section) in out.iter().zip(sections) {
            s += &format!("  @{:02X}|{:04X}: {}\n", off, self.offset + off, line);
            off += section.length;
        }
        s
    }
}

#[derive(Debug)]
pub struct UnknownUnknown {
    pub offset: usize,
    pub data: Vec<u8>,
}

impl UnknownUnknown {
    fn size(&self) -> usize {
        self.data.len()
    }

    fn magic(&self) -> &'static str {
        "Unknown"
    }

    fn at_offset(&self) -> usize {
        self.offset
    }

    fn show(&self) -> String {
        // let msg = if let Ok(msg) = str::from_utf8(&self.data) {
        //     msg
        // } else {
        //     ""
        // };
        // format!(
        //     "Unknown @ {:04X}: {:6} => {} ({})",
        //     self.offset,
        //     self.data.len(),
        //     bs2s(&self.data),
        //     msg
        // )
        use reverse::{format_sections, Section, ShowMode};
        let mut sections = Vec::new();
        let mut sec_start = self.offset;
        // make a first section to align to word boundary if needed
        if sec_start % 4 != 0 {
            let sec_end = self.offset + 4 - (sec_start % 4);
            sections.push(Section::new(
                0x0000,
                sec_start - self.offset,
                sec_end - sec_start,
            ));
            sec_start = sec_end;
        }

        while sec_start < self.offset + self.data.len() {
            let sec_end = cmp::min(sec_start + 16, self.offset + self.data.len());
            sections.push(Section::new(
                0x0000,
                sec_start - self.offset,
                sec_end - sec_start,
            ));
            //println!("{} => {}", sec_start - self.offset, sec_end - self.offset,);
            sec_start = sec_end;
        }

        let out = format_sections(&self.data, &sections, &mut vec![], &ShowMode::AllPerLine);
        let mut s = format!("Unknown @ {:04X}: {:6}b =>\n", self.offset, self.data.len(),);
        let mut off = 0;
        for (line, section) in out.iter().zip(sections) {
            s += &format!("  @{:02X}|{:04X}: {}\n", off, self.offset + off, line);
            off += section.length;
        }
        s
    }
}

#[derive(Debug)]
pub struct Pad1E {
    offset: usize,
    length: usize,
    data: *const u8,
}
impl Pad1E {
    pub const MAGIC: u8 = 0x1E;

    fn from_bytes(offset: usize, code: &[u8]) -> Fallible<Self> {
        let mut cnt = 0;
        while code[offset + cnt] == 0x1E {
            cnt += 1;
        }
        assert!(cnt > 0);
        Ok(Pad1E { offset, length: cnt, data: (&code[offset..]).as_ptr() })
    }

    fn size(&self) -> usize {
        self.length
    }

    fn magic(&self) -> &'static str {
        "1E"
    }

    fn at_offset(&self) -> usize {
        self.offset
    }

    fn show(&self) -> String {
        if self.length == 1 {
            format!(
                "@{:04X} {}Pad1E: 1E{}   |",
                self.offset,
                Escape::new().dimmed(),
                Escape::new()
            )
        } else if self.length == 2 {
            format!(
                "@{:04X} {}Pad1E: 1E 1E{}|",
                self.offset,
                Escape::new().dimmed(),
                Escape::new()
            )
        } else {
            format!(
                "@{:04X} {}Pad1E: 1E 1E{}| {}{}{}",
                self.offset,
                Escape::new().dimmed(),
                Escape::new(),
                Escape::new().dimmed(),
                p2s(self.data, 2, self.size()),
                Escape::new()
            )

        }
    }
}

macro_rules! opaque_instr {
    ($name:ident, $magic_str: expr, $magic:expr, $size:expr) => {
        pub struct $name {
            pub offset: usize,
            pub data: *const u8,
        }

        impl $name {
            pub const MAGIC: u8 = $magic;
            pub const SIZE: usize = $size;

            fn from_bytes(offset: usize, code: &[u8]) -> Fallible<Self> {
                let data = &code[offset..];
                assert_eq!(data[0], Self::MAGIC);
                ensure!(data[1] == 0 || data[1] == 0xFF, "not a word code instruction");
                ensure!(
                    ONE_BYTE_MAGIC.contains(&Self::MAGIC) || data[1] == 0,
                    "expected 1-byte instr or 0 in hi byte"
                );
                Ok(Self {
                    offset,
                    data: data.as_ptr(),
                })
            }

            fn size(&self) -> usize {
                Self::SIZE
            }

            fn magic(&self) -> &'static str {
                $magic_str
            }

            fn at_offset(&self) -> usize {
                self.offset
            }

            fn show(&self) -> String {
                let clr = if stringify!($name) == "Header" {
                    Color::Green
                } else {
                    Color::Red
                };
                format!(
                    "@{:04X} {}{}{}: {}{}{}| {}{}{}",
                    self.offset,
                    Escape::new().fg(clr).bold(),
                    stringify!($name),
                    Escape::new(),
                    Escape::new().fg(clr).bold(),
                    p2s(self.data, 0, 2).trim(),
                    Escape::new(),
                    Escape::new().fg(clr),
                    p2s(self.data, 2, Self::SIZE),
                    Escape::new()
                )
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(
                    f,
                    "{} @{:04X}: {}",
                    stringify!($name),
                    self.offset,
                    p2s(self.data, 0, 2),
                )
            }
        }
    };
}

opaque_instr!(Unk4E, "4E", 0x4E, 2); // 6 instances
opaque_instr!(Unk08, "08", 0x08, 4); // 7 instances
opaque_instr!(UnkB2, "B2", 0xB2, 2); // 9 instances

opaque_instr!(Header, "Header", 0xFF, 14);
opaque_instr!(Unk06, "06", 0x06, 21);
opaque_instr!(Unk0C, "0C", 0x0C, 17);
opaque_instr!(Unk0E, "0E", 0x0E, 17);
opaque_instr!(Unk10, "10", 0x10, 17);
opaque_instr!(Unk12, "12", 0x12, 4);
opaque_instr!(Unk2E, "2E", 0x2E, 4);
opaque_instr!(Unk3A, "3A", 0x3A, 6);
opaque_instr!(Unk44, "44", 0x44, 4);
opaque_instr!(Unk46, "46", 0x46, 2);
opaque_instr!(Unk48, "48", 0x48, 4);
opaque_instr!(Unk66, "66", 0x66, 10);
opaque_instr!(Unk6C, "6C", 0x6C, 13);
opaque_instr!(Unk72, "72", 0x72, 4);
opaque_instr!(Unk78, "78", 0x78, 12);
opaque_instr!(Unk7A, "7A", 0x7A, 10);
opaque_instr!(Unk96, "96", 0x96, 6);
opaque_instr!(UnkA6, "A6", 0xA6, 6);
opaque_instr!(UnkAC, "AC", 0xAC, 4);
opaque_instr!(UnkB8, "B8", 0xB8, 4);
opaque_instr!(UnkC4, "C4", 0xC4, 16);
//opaque_instr!(UnkC8, "C8", 0xC8, 8);
opaque_instr!(UnkCA, "CA", 0xCA, 4);
opaque_instr!(UnkD0, "D0", 0xD0, 4);
opaque_instr!(UnkD2, "D2", 0xD2, 8);
opaque_instr!(UnkDA, "DA", 0xDA, 4);
opaque_instr!(UnkDC, "DC", 0xDC, 12);
opaque_instr!(UnkE4, "E4", 0xE4, 20);
opaque_instr!(UnkE6, "E6", 0xE6, 10);
opaque_instr!(UnkE8, "E8", 0xE8, 6);
opaque_instr!(UnkEA, "EA", 0xEA, 8);
opaque_instr!(UnkEE, "EE", 0xEE, 2);
//opaque_instr!(UnkF2, 0xF2, 4);

#[derive(Debug)]
#[allow(non_camel_case_types)]
pub enum Instr {
    Header(Header),

    // Fixed size, with wasted 0 byte.
    Unk06(Unk06),
    Unk08(Unk08),
    Unk0C(Unk0C),
    Unk0E(Unk0E),
    Unk10(Unk10),
    Unk12(Unk12),
    Unk2E(Unk2E),
    Unk3A(Unk3A),
    Unk44(Unk44),
    Unk46(Unk46),
    Unk48(Unk48),
    Unk4E(Unk4E),
    Unk66(Unk66),
    Unk6C(Unk6C),
    Unk72(Unk72),
    Unk78(Unk78),
    Unk7A(Unk7A),
    Unk96(Unk96),
    UnkA6(UnkA6),
    UnkAC(UnkAC),
    UnkB2(UnkB2),
    UnkB8(UnkB8),
    UnkC4(UnkC4),
    UnkC8_JumpOnDetailLevel(UnkC8_JumpOnDetailLevel),
    UnkCA(UnkCA),
    UnkCE(UnkCE),
    UnkD0(UnkD0),
    UnkD2(UnkD2),
    UnkDA(UnkDA),
    UnkDC(UnkDC),
    UnkE4(UnkE4),
    UnkE6(UnkE6),
    UnkE8(UnkE8),
    UnkEA(UnkEA),
    UnkEE(UnkEE),
    F2_JumpIfNotShown(F2_JumpIfNotShown),

    // Fixed size, without wasted 0 byte after header.
    Pad1E(Pad1E),
    UnkF6(UnkF6),
    Unk38(Unk38),

    // Variable size.
    UnkBC(UnkBC),
    Unk40(Unk40),
    TrailerUnknown(TrailerUnknown),

    // Known quantities.
    TextureRef(TextureRef),
    // 0x00E2
    TextureIndex(TextureIndex),
    // 0x00E0
    SourceRef(SourceRef),
    // 0x0042
    VertexBuf(VertexBuf),
    // 0x0082
    Facet(Facet), // 0x__FC

    // Raw i386 bitcode used as a scripting language.
    X86Code(X86Code),
    X86Trampoline(X86Trampoline),
    UnknownUnknown(UnknownUnknown),
    UnknownData(UnknownData),
}

macro_rules! impl_for_all_instr {
    ($self:ident, $f:ident) => {
        match $self {
            Instr::Header(ref i) => i.$f(),
            Instr::Unk06(ref i) => i.$f(),
            Instr::Unk08(ref i) => i.$f(),
            Instr::Unk0C(ref i) => i.$f(),
            Instr::Unk0E(ref i) => i.$f(),
            Instr::Unk10(ref i) => i.$f(),
            Instr::Unk12(ref i) => i.$f(),
            Instr::Pad1E(ref i) => i.$f(),
            Instr::Unk2E(ref i) => i.$f(),
            Instr::Unk3A(ref i) => i.$f(),
            Instr::Unk44(ref i) => i.$f(),
            Instr::Unk46(ref i) => i.$f(),
            Instr::Unk48(ref i) => i.$f(),
            Instr::Unk4E(ref i) => i.$f(),
            Instr::Unk66(ref i) => i.$f(),
            Instr::Unk6C(ref i) => i.$f(),
            Instr::Unk72(ref i) => i.$f(),
            Instr::Unk78(ref i) => i.$f(),
            Instr::Unk7A(ref i) => i.$f(),
            Instr::Unk96(ref i) => i.$f(),
            Instr::UnkA6(ref i) => i.$f(),
            Instr::UnkAC(ref i) => i.$f(),
            Instr::UnkB2(ref i) => i.$f(),
            Instr::UnkB8(ref i) => i.$f(),
            Instr::UnkC4(ref i) => i.$f(),
            Instr::UnkC8_JumpOnDetailLevel(ref i) => i.$f(),
            Instr::UnkCA(ref i) => i.$f(),
            Instr::UnkCE(ref i) => i.$f(),
            Instr::UnkD0(ref i) => i.$f(),
            Instr::UnkD2(ref i) => i.$f(),
            Instr::UnkDA(ref i) => i.$f(),
            Instr::UnkDC(ref i) => i.$f(),
            Instr::UnkE4(ref i) => i.$f(),
            Instr::UnkE6(ref i) => i.$f(),
            Instr::UnkE8(ref i) => i.$f(),
            Instr::UnkEA(ref i) => i.$f(),
            Instr::UnkEE(ref i) => i.$f(),
            Instr::F2_JumpIfNotShown(ref i) => i.$f(),
            Instr::UnkF6(ref i) => i.$f(),
            Instr::Unk38(ref i) => i.$f(),
            Instr::UnkBC(ref i) => i.$f(),
            Instr::Unk40(ref i) => i.$f(),
            Instr::TrailerUnknown(ref i) => i.$f(),
            Instr::TextureIndex(ref i) => i.$f(),
            Instr::TextureRef(ref i) => i.$f(),
            Instr::SourceRef(ref i) => i.$f(),
            Instr::VertexBuf(ref i) => i.$f(),
            Instr::Facet(ref i) => i.$f(),
            Instr::X86Code(ref i) => i.$f(),
            Instr::X86Trampoline(ref i) => i.$f(),
            Instr::UnknownUnknown(ref i) => i.$f(),
            Instr::UnknownData(ref i) => i.$f(),
        }
    };
}

impl Instr {
    pub fn show(&self) -> String {
        impl_for_all_instr!(self, show)
    }

    pub fn size(&self) -> usize {
        impl_for_all_instr!(self, size)
    }

    pub fn magic(&self) -> &'static str {
        impl_for_all_instr!(self, magic)
    }

    pub fn at_offset(&self) -> usize {
        impl_for_all_instr!(self, at_offset)
    }
}

macro_rules! consume_instr {
    ($name:ident, $pe:ident, $offset:ident, $instrs:ident) => {{
        let instr = $name::from_bytes(*$offset, &$pe.code)?;
        *$offset += instr.size();
        $instrs.push(Instr::$name(instr));
    }};
}

impl CpuShape {
    pub fn from_data(data: &[u8]) -> Fallible<Self> {
        let mut pe = peff::PE::parse(data)?;

        // Do default relocation to a high address. This makes offsets appear
        // 0-based and tags all local pointers with an obvious flag.
        pe.relocate(SHAPE_LOAD_BASE)?;
        let trampolines = Self::find_trampolines(&pe)?;

        let mut instrs = Self::read_sections(&pe, &trampolines)?;
        let mut tramp_instr = trampolines
            .iter()
            .map(|t| Instr::X86Trampoline(t.to_owned()))
            .collect::<Vec<_>>();
        instrs.append(&mut tramp_instr);

        Ok(CpuShape {
            instrs,
            trampolines,
            pe,
        })
    }

    pub fn all_textures(&self) -> HashSet<String> {
        let mut uniq = HashSet::new();
        for instr in &self.instrs {
            if let Instr::TextureRef(tex) = instr {
                uniq.insert(tex.filename.to_owned());
            }
        }
        return uniq;
    }

    fn find_trampolines(pe: &peff::PE) -> Fallible<Vec<X86Trampoline>> {
        let mut offset = pe.code.len() - 6;
        let mut trampolines = Vec::new();
        while offset > 0 {
            let maybe_tramp = X86Trampoline::from_pe(offset, pe);
            if let Ok(tramp) = maybe_tramp {
                trampolines.push(tramp);
            } else {
                break;
            }
            offset -= 6;
        }
        trampolines.reverse();
        Ok(trampolines)
    }

    fn read_sections(pe: &peff::PE, trampolines: &[X86Trampoline]) -> Fallible<Vec<Instr>> {
        let mut offset = 0;
        let mut instrs = Vec::new();
        while offset < pe.code.len() {
            // trace!(
            //     "Decoding At: {:04X}: {}",
            //     offset,
            //     bs2s(&pe.code[offset..cmp::min(pe.code.len(), offset + 20)])
            // );
            //assert!(ALL_OPCODES.contains(&pe.code[offset]));
            Self::read_instr(&mut offset, pe, trampolines, &mut instrs)?;
            trace!("=>: {}", instrs.last().unwrap().show());
        }

        // Assertions.
        //        {
        //            let instr = find_first_instr(0xF2, &instrs);
        //            if let Some(&Instr::F2_JumpIfNotShown(ref jmp)) = instr {
        //                let tgt = _find_instr_at_offset(jmp.next_offset(), &instrs);
        //                assert!(tgt.is_some());
        //            }
        //        }

        Ok(instrs)
    }

    fn read_instr(
        offset: &mut usize,
        pe: &peff::PE,
        trampolines: &[X86Trampoline],
        instrs: &mut Vec<Instr>,
    ) -> Fallible<()> {
        match pe.code[*offset] {
            Header::MAGIC => consume_instr!(Header, pe, offset, instrs),
            Unk06::MAGIC => consume_instr!(Unk06, pe, offset, instrs),
            Unk08::MAGIC => consume_instr!(Unk08, pe, offset, instrs),
            Unk0C::MAGIC => consume_instr!(Unk0C, pe, offset, instrs),
            Unk0E::MAGIC => consume_instr!(Unk0E, pe, offset, instrs),
            Unk10::MAGIC => consume_instr!(Unk10, pe, offset, instrs),
            Unk12::MAGIC => consume_instr!(Unk12, pe, offset, instrs),
            Pad1E::MAGIC => consume_instr!(Pad1E, pe, offset, instrs),
            Unk2E::MAGIC => consume_instr!(Unk2E, pe, offset, instrs),
            Unk3A::MAGIC => consume_instr!(Unk3A, pe, offset, instrs),
            Unk40::MAGIC => consume_instr!(Unk40, pe, offset, instrs),
            Unk44::MAGIC => consume_instr!(Unk44, pe, offset, instrs),
            Unk46::MAGIC => consume_instr!(Unk46, pe, offset, instrs),
            Unk48::MAGIC => consume_instr!(Unk48, pe, offset, instrs),
            Unk4E::MAGIC => consume_instr!(Unk4E, pe, offset, instrs),
            Unk66::MAGIC => consume_instr!(Unk66, pe, offset, instrs),
            Unk6C::MAGIC => consume_instr!(Unk6C, pe, offset, instrs),
            Unk72::MAGIC => consume_instr!(Unk72, pe, offset, instrs),
            Unk78::MAGIC => consume_instr!(Unk78, pe, offset, instrs),
            Unk7A::MAGIC => consume_instr!(Unk7A, pe, offset, instrs),
            Unk96::MAGIC => consume_instr!(Unk96, pe, offset, instrs),
            UnkA6::MAGIC => consume_instr!(UnkA6, pe, offset, instrs),
            UnkAC::MAGIC => consume_instr!(UnkAC, pe, offset, instrs),
            UnkB2::MAGIC => consume_instr!(UnkB2, pe, offset, instrs),
            UnkB8::MAGIC => consume_instr!(UnkB8, pe, offset, instrs),
            UnkBC::MAGIC => consume_instr!(UnkBC, pe, offset, instrs),
            UnkC4::MAGIC => consume_instr!(UnkC4, pe, offset, instrs),
            UnkC8_JumpOnDetailLevel::MAGIC => {
                consume_instr!(UnkC8_JumpOnDetailLevel, pe, offset, instrs)
            }
            UnkCA::MAGIC => consume_instr!(UnkCA, pe, offset, instrs),
            UnkCE::MAGIC => consume_instr!(UnkCE, pe, offset, instrs),
            UnkD0::MAGIC => consume_instr!(UnkD0, pe, offset, instrs),
            UnkD2::MAGIC => consume_instr!(UnkD2, pe, offset, instrs),
            UnkDA::MAGIC => consume_instr!(UnkDA, pe, offset, instrs),
            UnkDC::MAGIC => consume_instr!(UnkDC, pe, offset, instrs),
            UnkE4::MAGIC => consume_instr!(UnkE4, pe, offset, instrs),
            UnkE6::MAGIC => consume_instr!(UnkE6, pe, offset, instrs),
            UnkE8::MAGIC => consume_instr!(UnkE8, pe, offset, instrs),
            UnkEA::MAGIC => consume_instr!(UnkEA, pe, offset, instrs),
            UnkEE::MAGIC => consume_instr!(UnkEE, pe, offset, instrs),
            UnkF6::MAGIC => consume_instr!(UnkF6, pe, offset, instrs),
            F2_JumpIfNotShown::MAGIC => consume_instr!(F2_JumpIfNotShown, pe, offset, instrs),
            Unk38::MAGIC => consume_instr!(Unk38, pe, offset, instrs),
            TextureRef::MAGIC => consume_instr!(TextureRef, pe, offset, instrs),
            TextureIndex::MAGIC => consume_instr!(TextureIndex, pe, offset, instrs),
            SourceRef::MAGIC => consume_instr!(SourceRef, pe, offset, instrs),
            VertexBuf::MAGIC => consume_instr!(VertexBuf, pe, offset, instrs),
            Facet::MAGIC => consume_instr!(Facet, pe, offset, instrs),
            X86Code::MAGIC => {
                let mut name = "unknown_source".to_owned();
                {
                    if let Some(&Instr::SourceRef(ref source)) = find_first_instr(0x42, &instrs) {
                        name = source.source.clone();
                    }
                }
                X86Code::from_bytes(&name, offset, pe, trampolines, instrs)?;
            }
            // Zero is the magic for the trailer (sans trampolines).
            0 => {
                let unk = TrailerUnknown::from_bytes(*offset, &pe.code, trampolines)?;
                instrs.push(Instr::TrailerUnknown(unk));
                *offset = pe.code.len();
            }
            // if we find something we don't recognize, add it as an unknown-unknown for
            // the entire rest of the file. If this is nested under x86 because it is between
            // regions, the caller will remove this and re-add it with a limited size.
            _vop => {
                let instr = UnknownUnknown {
                    offset: *offset,
                    data: pe.code[*offset..].to_owned(),
                };
                *offset = pe.code.len();
                instrs.push(Instr::UnknownUnknown(instr));

                // Someday we'll be able to turn on this bail.
                //bail!("unknown instruction 0x{:02X} at 0x{:04X}: {}", vop, offset, bs2s(&pe.code[*offset..])),
            }
        }
        Ok(())
    }

    // Map an offset in bytes from the beginning of the virtual instruction stream
    // to an offset into the virtual instructions.
    pub fn map_absolute_offset_to_instr_offset(&self, abs_offset: usize) -> Fallible<usize> {
        let mut cur_offset = 0;
        for (instr_offset, instr) in self.instrs.iter().enumerate() {
            if cur_offset == abs_offset {
                return Ok(instr_offset);
            }
            cur_offset += instr.size();
        }
        bail!("no instruction at absolute offset: {:08X}", abs_offset)
    }

    pub fn map_interpreter_offset_to_instr_offset(&self, x86_offset: u32) -> Fallible<usize> {
        let mut b_offset = 0u32;
        for (offset, instr) in self.instrs.iter().enumerate() {
            if SHAPE_LOAD_BASE + b_offset == x86_offset {
                return Ok(offset);
            }
            b_offset += instr.size() as u32;
        }
        bail!("no instruction at x86_offset: {:08X}", x86_offset)
    }
}

fn find_first_instr(kind: u8, instrs: &[Instr]) -> Option<&Instr> {
    for instr in instrs.iter() {
        match kind {
            0xF2 => {
                if let Instr::F2_JumpIfNotShown(ref _x) = instr {
                    return Some(instr);
                }
            }
            0x42 => {
                if let Instr::SourceRef(ref _x) = instr {
                    return Some(instr);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
extern crate omnilib;

#[cfg(test)]
mod tests {
    use super::*;
    use failure::Error;
    use i386::ExitInfo;
    use omnilib::OmniLib;
    use simplelog::{Config, LevelFilter, TermLogger};
    use std::{collections::HashMap, fs};
    use std::io::prelude::*;

    fn offset_of_trailer(shape: &CpuShape) -> Option<usize> {
        let mut offset = None;
        for (_i, instr) in shape.instrs.iter().enumerate() {
            if let Instr::TrailerUnknown(trailer) = instr {
                assert_eq!(offset, None, "multiple trailers");
                offset = Some(trailer.offset);
            }
        }
        return offset;
    }

    fn last_non_tramp_instr(shape: &CpuShape) -> &Instr {
        for instr in shape.instrs.iter().rev() {
            if let Instr::X86Trampoline(_tramp) = instr {
                continue;
            } else {
                return instr;
            }
        }
        panic!("no non-trampoline instructions");
    }

    #[allow(dead_code)]
    fn compute_instr_freqs(shape: &CpuShape, freq: &mut HashMap<&'static str, usize>) {
        for instr in &shape.instrs {
            let name = instr.magic();
            let cnt = if let Some(cnt) = freq.get(name) {
                cnt + 1
            } else {
                1
            };
            freq.insert(name, cnt);
        }
    }

    #[allow(dead_code)]
    fn show_instr_freqs(freq: &HashMap<&'static str, usize>) {
        let mut freqs = freq.iter().map(|(&k, &v)| (k, v)).collect::<Vec<(&'static str, usize)>>();
        freqs.sort_by_key(|(_, f)| *f);
        for (k, v) in freqs {
            println!("{}: {}", k, v);
        }
    }

    #[test]
    fn it_works() -> Fallible<()> {
        let _ = TermLogger::init(LevelFilter::Info, Config::default()).unwrap();

        let omni = OmniLib::new_for_test_in_games(&[
            "FA", "ATFGOLD", "USNF97", "ATF", "ATFNATO", "MF", "USNF",
        ])?;

        #[allow(unused_variables, unused_mut)]
        let mut freq: HashMap<&'static str, usize> = HashMap::new();

        for (game, name) in omni.find_matching("*.SH")?.iter() {
            println!(
                "At: {}:{:13} @ {}",
                game,
                name,
                omni.path(game, name)
                    .or::<Error>(Ok("<none>".to_string()))?
            );

            let lib = omni.library(game);
            let data = lib.load(name)?;
            let shape = CpuShape::from_data(&data)?;

            //compute_instr_freqs(&shape, &mut freq);

            if let Some(_offset) = offset_of_trailer(&shape) {
                // TODO: check that any UnkF2's target is equal to this offset.
            } else {
                // There must be a trailing unknown unknowns.
                let last = last_non_tramp_instr(&shape);
                if let Instr::UnknownUnknown(unk) = last {
                    // ok
                    println!("Unknown {:02X} {:02X}: {} {}", unk.data[0], unk.data[1], name, bs2s(&unk.data));
                } else {
                    assert!(false, "no trailing unknown when no trailer");
                }
            }
        }

        //show_instr_freqs(&freq);

        Ok(())
    }

    // struct ObjectInfo {
    //     field0: u8,
    //     field2: u8,

    // USED BY FLARE.ASM
    //     // eax = -field22 >> 10
    //     // if eax < 90 {
    //     //     if eax < -90 {
    //     //        eax = -90
    //     //     }
    //     // } else { eax = 90 }
    //     field22: u32,

    // USED BY EXP.ASM
    //     // ax = (field49 - field48) & 0x00FF
    //     // cx = _currentTicks - field40
    //     // dx:ax = ax * cx
    //     // cx = field44 - field40
    //     // ax, dx = dx:ax / cx, dx:ax % cx
    //     // ax += field48
    //     // if ax > 0x316 { _ErrorExit("Bad value (around line 133) in exp.asm!") }
    //     field40: u16,
    //     field44: u16,
    //     field48: u8,
    //     field49: u8,
    // }

    #[test]
    fn virtual_interp() {
        let _ = TermLogger::init(LevelFilter::Trace, Config::default()).unwrap();

        let path = "./test_data/EXP.SH";
        //let path = "./test_data/FLARE.SH";
        //let path = "./test_data/WNDMLL.SH";
        let mut fp = fs::File::open(path).unwrap();
        let mut data = Vec::new();
        fp.read_to_end(&mut data).unwrap();

        // let obj = ExplosionInfo {
        //     field0: 0x11,
        //     field2: 0x10,
        //     field40: 0,
        //     field44: 4,
        //     field48: 25,
        //     field49: 44,
        // };
        let exp_base = 0x77000000;

        let shape = CpuShape::from_data(&data).unwrap();
        let mut interp = i386::Interpreter::new();
        for tramp in shape.trampolines.iter() {
            if !tramp.is_data {
                interp.add_trampoline(tramp.location, &tramp.name, 1);
                continue;
            }
            match tramp.name.as_ref() {
                "brentObjId" => interp.add_read_port(
                    tramp.location,
                    Box::new(move || {
                        println!("LOOKUP brentObjectId");
                        exp_base // Lowest valid (shown?) object id is 0x3E8, at least by the check EXP.sh does.
                    }),
                ),
                "_currentTicks" => interp.add_read_port(
                    tramp.location,
                    Box::new(move || {
                        println!("LOOKUP _currentTicks");
                        0
                    }),
                ),
                "viewer_x" => interp.add_read_port(
                    tramp.location,
                    Box::new(move || {
                        println!("LOOKUP viewer_x");
                        0
                    }),
                ),
                "viewer_z" => interp.add_read_port(
                    tramp.location,
                    Box::new(move || {
                        println!("LOOKUP viewer_z");
                        0
                    }),
                ),
                "xv32" => interp.add_read_port(
                    tramp.location,
                    Box::new(move || {
                        println!("LOOKUP xv32");
                        0
                    }),
                ),
                "zv32" => interp.add_read_port(
                    tramp.location,
                    Box::new(move || {
                        println!("LOOKUP zv32");
                        0
                    }),
                ),
                "_effectsAllowed" => {
                    interp.add_read_port(
                        tramp.location,
                        Box::new(move || {
                            println!("LOOKUP _effectsAllowed");
                            0
                        }),
                    );
                    interp.add_write_port(
                        tramp.location,
                        Box::new(move |value| {
                            println!("SET _effectsAllowed to {}", value);
                        }),
                    );
                }
                _ => panic!("unknown data reference to {}", tramp.name),
            }
        }

        // FLARE
        interp.add_read_port(
            exp_base + 0x22,
            Box::new(|| {
                return 0x10 as u32;
            }),
        );

        // EXP
        interp.add_read_port(
            exp_base,
            Box::new(|| {
                return 0x11 as u32;
            }),
        );
        interp.add_read_port(
            exp_base + 0x2,
            Box::new(|| {
                return 0x10 as u32;
            }),
        );
        interp.add_read_port(
            exp_base + 0x40,
            Box::new(|| {
                return 0 as u32;
            }),
        );
        interp.add_read_port(
            exp_base + 0x44,
            Box::new(|| {
                return 4 as u32;
            }),
        );
        interp.add_read_port(
            exp_base + 0x48,
            Box::new(|| {
                return 25 as u32;
            }),
        );
        interp.add_read_port(
            exp_base + 0x49,
            Box::new(move || {
                return 44 as u32;
            }),
        );

        for instr in shape.instrs.iter() {
            match instr {
                // Written into by windmill with (_currentTicks & 0xFF) << 2.
                // The frame of animation to show, maybe?
                Instr::UnkC4(ref c4) => interp.add_write_port(
                    SHAPE_LOAD_BASE + c4.offset as u32 + 2 + 0xA,
                    Box::new(move |value| {
                        println!("WOULD UPDATE C4 Internals with {:02X}", value);
                    }),
                ),
                Instr::UnknownUnknown(ref unk) => {
                    interp
                        .map_writable((0xAA000000 + unk.offset) as u32, unk.data.clone())
                        .unwrap();
                }
                Instr::X86Code(ref code) => {
                    interp.add_code(&code.bytecode);
                }
                _ => {}
            }
            println!("{}", instr.show());
        }
        let mut offset = 0;
        while offset < shape.instrs.len() {
            if let Instr::X86Code(ref code) = shape.instrs[offset] {
                let rv = interp
                    .interpret(0xAA000000u32 + code.code_offset as u32)
                    .unwrap();
                match rv {
                    ExitInfo::OutOfInstructions => break,
                    ExitInfo::Trampoline(ref name, ref args) => {
                        println!("Got trampoline return to {} with args {:?}", name, args);
                        offset = shape
                            .map_interpreter_offset_to_instr_offset(args[0])
                            .unwrap();
                        println!("Resuming at instruction {}", offset);
                    }
                }
            } else {
                offset += 1;
            }
        }
    }
}
