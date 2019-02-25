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
#![allow(clippy::transmute_ptr_to_ptr)]

use bitflags::bitflags;
use failure::{bail, ensure, err_msg, Fail, Fallible};
use i386::{ByteCode, Memonic, Operand};
use lazy_static::lazy_static;
use log::trace;
use reverse::{bs2s, p2s, Color, Escape};
use std::{
    cmp,
    collections::{HashMap, HashSet},
    fmt, mem, str,
};

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

bitflags! {
    pub struct FacetFlags : u16 {
        const UNK0                 = 0b0000_1000_0000_0000;
        const USE_SHORT_INDICES    = 0b0000_0100_0000_0000;
        const USE_SHORT_MATERIAL   = 0b0000_0010_0000_0000;
        const USE_BYTE_TEXCOORDS   = 0b0000_0001_0000_0000;
        const UNK1                 = 0b0000_0000_1000_0000;
        const HAVE_MATERIAL        = 0b0000_0000_0100_0000;
        const UNK2                 = 0b0000_0000_0010_0000;
        const UNK3                 = 0b0000_0000_0001_0000;
        const UNK4                 = 0b0000_0000_0000_1000;
        const HAVE_TEXCOORDS       = 0b0000_0000_0000_0100;
        const FILL_BACKGROUND      = 0b0000_0000_0000_0010;
        const UNK5                 = 0b0000_0000_0000_0001;
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
    pub unk0: i16,
    pub verts: Vec<[i16; 3]>,
}

impl VertexBuf {
    pub const MAGIC: u8 = 0x82;

    fn from_bytes(offset: usize, code: &[u8]) -> Fallible<Self> {
        let data = &code[offset..];
        assert_eq!(data[0], Self::MAGIC);
        assert_eq!(data[1], 0);
        let head: &[u16] = unsafe { mem::transmute(&data[2..6]) };
        let words: &[i16] = unsafe { mem::transmute(&data[6..]) };
        let mut buf = VertexBuf {
            offset,
            unk0: head[2] as i16,
            verts: Vec::new(),
        };
        let nverts = head[0] as usize;
        for i in 0..nverts {
            let x = words[i * 3];
            let y = words[i * 3 + 1];
            let z = words[i * 3 + 2];
            buf.verts.push([x, y, z]);
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
            .verts
            .iter()
            .map(|v| format!("({},{},{})", v[0], v[1], v[2]))
            .collect::<Vec<String>>()
            .join(", ");
        format!(
            "@{:04X} {}VxBuf: 82 00{}| {}{:04X} ({:b} | {}){} => {}verts -> {}{}{}",
            self.offset,
            Escape::new().fg(Color::Magenta).bold(),
            Escape::new(),
            Escape::new().fg(Color::Magenta),
            self.unk0,
            self.unk0,
            self.unk0 as i16,
            Escape::new(),
            self.verts.len(),
            Escape::new().fg(Color::Magenta).dimmed(),
            s,
            Escape::new(),
        )
    }
}

// No idea what this does, but there is a 16bit count in the middle with
// count bytes following it.
#[derive(Debug)]
pub struct Unk06 {
    pub offset: usize,
    pub length: usize,
    pub data: *const u8,

    pub count: usize,
}

impl Unk06 {
    pub const MAGIC: u8 = 0x06;

    fn from_bytes(offset: usize, code: &[u8]) -> Fallible<Self> {
        let data = &code[offset..];
        assert_eq!(data[0], Self::MAGIC);
        ensure!(data[1] == 0, "not a word code instruction");
        let words: &[u16] = unsafe { mem::transmute(&data[14..]) };
        let count = words[0] as usize;
        let length = 16 + count;
        Ok(Self {
            offset,
            length,
            data: data.as_ptr(),
            count,
        })
    }

    fn size(&self) -> usize {
        self.length
    }

    fn magic(&self) -> &'static str {
        "06"
    }

    fn at_offset(&self) -> usize {
        self.offset
    }

    fn show(&self) -> String {
        format!(
            "@{:04X} {}{}{}: {}{}{}| {}{}{}; {}cnt:{}{}; {}{}{}",
            self.offset,
            Escape::new().fg(Color::Red).bold(),
            stringify!(Unk06),
            Escape::new(),
            Escape::new().fg(Color::Red).bold(),
            p2s(self.data, 0, 2).trim(),
            Escape::new(),
            Escape::new().fg(Color::Red),
            p2s(self.data, 2, 14).trim(),
            Escape::new(),
            Escape::new().fg(Color::Cyan),
            self.count,
            Escape::new(),
            Escape::new().fg(Color::Red),
            p2s(self.data, 16, self.length),
            Escape::new()
        )
    }
}

// No idea what this does, but there is a 16bit count in the middle with
// count bytes following it.
#[derive(Debug)]
pub struct Unk0C {
    pub offset: usize,
    pub length: usize,
    pub data: *const u8,

    pub count: usize,
}

impl Unk0C {
    pub const MAGIC: u8 = 0x0C;

    fn from_bytes(offset: usize, code: &[u8]) -> Fallible<Self> {
        let data = &code[offset..];
        assert_eq!(data[0], Self::MAGIC);
        ensure!(data[1] == 0, "not a word code instruction");
        let words: &[u16] = unsafe { mem::transmute(&data[10..]) };
        let count = words[0] as usize;
        let length = 12 + count;
        Ok(Self {
            offset,
            length,
            data: data.as_ptr(),
            count,
        })
    }

    fn size(&self) -> usize {
        self.length
    }

    fn magic(&self) -> &'static str {
        "0C"
    }

    fn at_offset(&self) -> usize {
        self.offset
    }

    fn show(&self) -> String {
        format!(
            "@{:04X} {}{}{}: {}{}{}| {}{}{}; {}cnt:{}{}; {}{}{}",
            self.offset,
            Escape::new().fg(Color::Red).bold(),
            stringify!(Unk0C),
            Escape::new(),
            Escape::new().fg(Color::Red).bold(),
            p2s(self.data, 0, 2).trim(),
            Escape::new(),
            Escape::new().fg(Color::Red),
            p2s(self.data, 2, 10).trim(),
            Escape::new(),
            Escape::new().fg(Color::Cyan),
            self.count,
            Escape::new(),
            Escape::new().fg(Color::Red),
            p2s(self.data, 12, self.length),
            Escape::new()
        )
    }
}

// No idea what this does, but there is a 16bit count in the middle with
// count bytes following it.
// 0E 00| 17 81 05 00 95 10 C1 FF 08 00 E0 21 50 00 A6 00 00 00
#[derive(Debug)]
pub struct Unk0E {
    pub offset: usize,
    pub length: usize,
    pub data: *const u8,

    pub count: usize,
}

impl Unk0E {
    pub const MAGIC: u8 = 0x0E;

    fn from_bytes_after(offset: usize, data: &[u8]) -> Fallible<Self> {
        assert_eq!(data[0], Self::MAGIC);
        ensure!(data[1] == 0, "not a word code instruction");
        let words: &[u16] = unsafe { mem::transmute(&data[10..]) };
        let count = words[0] as usize;
        let length = 12 + count;
        Ok(Self {
            offset,
            length,
            data: data.as_ptr(),
            count,
        })
    }

    fn size(&self) -> usize {
        self.length
    }

    fn magic(&self) -> &'static str {
        "0C"
    }

    fn at_offset(&self) -> usize {
        self.offset
    }

    fn show(&self) -> String {
        format!(
            "@{:04X} {}{}{}: {}{}{}| {}{}{}; {}cnt:{}{}; {}{}{}",
            self.offset,
            Escape::new().fg(Color::Red).bold(),
            stringify!(Unk0E),
            Escape::new(),
            Escape::new().fg(Color::Red).bold(),
            p2s(self.data, 0, 2).trim(),
            Escape::new(),
            Escape::new().fg(Color::Red),
            p2s(self.data, 2, 10).trim(),
            Escape::new(),
            Escape::new().fg(Color::Cyan),
            self.count,
            Escape::new(),
            Escape::new().fg(Color::Red),
            p2s(self.data, 12, self.length),
            Escape::new()
        )
    }
}

// No idea what this does, but there is a 16bit count in the middle with
// count bytes following it.
// 0E 00| 17 81 05 00 95 10 C1 FF 08 00 E0 21 50 00 A6 00 00 00
#[derive(Debug)]
pub struct Unk10 {
    pub offset: usize,
    pub length: usize,
    pub data: *const u8,

    pub count: usize,
}

impl Unk10 {
    pub const MAGIC: u8 = 0x10;

    fn from_bytes_after(offset: usize, data: &[u8]) -> Fallible<Self> {
        assert_eq!(data[0], Self::MAGIC);
        ensure!(data[1] == 0, "not a word code instruction");
        let words: &[u16] = unsafe { mem::transmute(&data[10..]) };
        let count = words[0] as usize;
        let length = 12 + count;
        Ok(Self {
            offset,
            length,
            data: data.as_ptr(),
            count,
        })
    }

    fn size(&self) -> usize {
        self.length
    }

    fn magic(&self) -> &'static str {
        "0C"
    }

    fn at_offset(&self) -> usize {
        self.offset
    }

    fn show(&self) -> String {
        format!(
            "@{:04X} {}{}{}: {}{}{}| {}{}{}; {}cnt:{}{}; {}{}{}",
            self.offset,
            Escape::new().fg(Color::Red).bold(),
            stringify!(Unk10),
            Escape::new(),
            Escape::new().fg(Color::Red).bold(),
            p2s(self.data, 0, 2).trim(),
            Escape::new(),
            Escape::new().fg(Color::Red),
            p2s(self.data, 2, 10).trim(),
            Escape::new(),
            Escape::new().fg(Color::Cyan),
            self.count,
            Escape::new(),
            Escape::new().fg(Color::Red),
            p2s(self.data, 12, self.length),
            Escape::new()
        )
    }
}

// 6C has a variable length
#[derive(Debug)]
pub struct Unk6C {
    pub offset: usize,
    pub length: usize,
    pub data: *const u8,

    pub flag: u8,
}

impl Unk6C {
    pub const MAGIC: u8 = 0x6C;

    fn from_bytes_after(offset: usize, data: &[u8]) -> Fallible<Self> {
        assert_eq!(data[0], Self::MAGIC);
        ensure!(data[1] == 0, "not a word code instruction");
        let flag = data[10];
        let length = match flag {
            0x38 => 13, // Normal
            0x48 => 14, // F18 -- one of our errata?
            0x50 => 16, // F8
            _ => bail!("unexpected flag byte in 6C instruction: {:02X}", flag)
        };
        Ok(Self {
            offset,
            length,
            data: data.as_ptr(),
            flag,
        })
    }

    fn size(&self) -> usize {
        self.length
    }

    fn magic(&self) -> &'static str {
        "6C"
    }

    fn at_offset(&self) -> usize {
        self.offset
    }

    fn show(&self) -> String {
        format!(
            "@{:04X} {}{}{}: {}{}{}| {}{}{}; {}flag:{:02X}{}; {}{}{}",
            self.offset,
            Escape::new().fg(Color::Red).bold(),
            stringify!(Unk6C),
            Escape::new(),
            Escape::new().fg(Color::Red).bold(),
            p2s(self.data, 0, 2).trim(),
            Escape::new(),
            Escape::new().fg(Color::Red),
            p2s(self.data, 2, 10).trim(),
            Escape::new(),
            Escape::new().fg(Color::Cyan),
            self.flag,
            Escape::new(),
            Escape::new().fg(Color::Red),
            p2s(self.data, 11, self.length),
            Escape::new()
        )
    }
}

#[derive(Debug)]
pub struct Facet {
    pub offset: usize,
    pub data: *const u8,
    pub length: usize,

    flags_pointer: *const u8,
    pub flags: FacetFlags,

    color_pointer: *const u8,
    pub color: u8,

    material_pointer: *const u8,
    material_size: usize,
    pub raw_material: Vec<u8>,

    indices_count_pointer: *const u8,
    indices_pointer: *const u8,
    indices_size: usize,
    pub indices: Vec<u16>,

    tc_pointer: *const u8,
    tc_size: usize,
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

        let mut off = 1;

        let flags_offset = off;
        let flags_arr: &[u16] = unsafe { mem::transmute(&data[flags_offset..]) };
        assert_eq!(flags_arr[0] & 0xF000, 0u16);
        let flags = FacetFlags::from_u16(flags_arr[0]);
        off += 2;

        let color_offset = off;
        let color = data[off];
        off += 1;

        // Material
        let material_offset = off;
        let material_size = if flags.contains(FacetFlags::HAVE_MATERIAL) {
            if flags.contains(FacetFlags::USE_SHORT_MATERIAL) {
                10
            } else {
                13
            }
        } else {
            1
        };
        let raw_material = data[off..off + material_size].to_vec();
        off += material_size;

        // Index count.
        let index_count_offset = off;
        let index_count = data[off] as usize;
        off += 1;

        // Indexes.
        let indices_offset = off;
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
        let indices_size = off - indices_offset;

        // Tex Coords
        let tc_offset = off;
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
        let tc_size = off - tc_offset;

        Ok(Facet {
            offset,
            data: data.as_ptr(),
            length: off,

            flags_pointer: (&data[flags_offset..]).as_ptr(),
            flags,

            color_pointer: (&data[color_offset..]).as_ptr(),
            color,

            material_pointer: (&data[material_offset..]).as_ptr(),
            material_size,
            raw_material,

            indices_count_pointer: (&data[index_count_offset..]).as_ptr(),
            indices_pointer: (&data[indices_offset..]).as_ptr(),
            indices_size,
            indices,

            tc_pointer: (&data[tc_offset..]).as_ptr(),
            tc_size,
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
        let flags = format!("{:016b}", self.flags)
            .chars()
            .skip(4)
            .collect::<Vec<char>>();

        // const USE_SHORT_INDICES    = 0b0000_0100_0000_0000;
        // const USE_SHORT_MATERIAL   = 0b0000_0010_0000_0000;
        // const USE_BYTE_TEXCOORDS   = 0b0000_0001_0000_0000;
        // const HAVE_MATERIAL        = 0b0000_0000_0100_0000;
        // const HAVE_TEXCOORDS       = 0b0000_0000_0000_0100;
        // const FILL_BACKGROUND      = 0b0000_0000_0000_0010;
        // const UNK_MATERIAL_RELATED = 0b0000_0000_0000_0001;

        let ind = self
            .indices
            .iter()
            .map(|i| format!("{:X}", i))
            .collect::<Vec<String>>()
            .join(",");

        let tcs = self
            .tex_coords
            .iter()
            .map(|a| format!("({:X},{:X})", a[0], a[1]))
            .collect::<Vec<String>>()
            .join(",");

        format!(
            "@{:04X} {}Facet: FC{}   | {}{}{}({}{}{}{}{}{}_{}{}{}{}{}{}{}_{}{}{}{}{}{}{}); {}{}{}; {}{}{}; {}{:02X}{}; {}{}{} ({}{}{}); {}{}{}[{}{}{}]",
            self.offset,
            Escape::new().fg(Color::Cyan).bold(),
            Escape::new(),

            // Flags
            Escape::new().fg(Color::Cyan),
            p2s(self.flags_pointer, 0, 2),
            Escape::new().fg(Color::White), // (

            Escape::new().fg(Color::Red),
            flags[0],
            Escape::new().fg(Color::Cyan),
            flags[1],
            flags[2],
            flags[3],

            Escape::new().fg(Color::Red),
            flags[4],
            Escape::new().fg(Color::Cyan),
            flags[5],
            Escape::new().fg(Color::Red),
            flags[6],
            flags[7],

            flags[8],
            Escape::new().fg(Color::Cyan),
            flags[9],
            flags[10],
            Escape::new().fg(Color::Magenta),
            flags[11],
            Escape::new().fg(Color::White), // )

            // Color
            Escape::new().fg(Color::Cyan),
            p2s(self.color_pointer, 0, 1).trim(),
            Escape::new(),

            // Material
            Escape::new().fg(Color::Red).dimmed(),
            p2s(self.material_pointer, 0, self.material_size),
            Escape::new(),

            // Index count
            Escape::new().fg(Color::Cyan).bold(),
            self.indices.len(),
            Escape::new(),

            // Indices
            Escape::new().fg(Color::Cyan).dimmed(),
            p2s(self.indices_pointer, 0, self.indices_size).trim(),
            Escape::new().fg(Color::White),
            Escape::new().fg(Color::Cyan).bold(),
            ind,
            Escape::new(),

            // Texture Coordinates
            Escape::new().fg(Color::Cyan).dimmed(),
            p2s(self.tc_pointer, 0, self.tc_size),
            Escape::new(),

            Escape::new().fg(Color::Cyan).bold(),
            tcs,
            Escape::new(),
        )
    }
}

#[derive(Clone, Debug)]
pub struct X86Trampoline {
    // Offset is from the start of the code section.
    pub offset: usize,

    // The name attached to the thunk that would populate this trampoline.
    pub name: String,

    // Where this trampoline would indirect to, if jumped to.
    pub target: u32,

    // Shape files call into engine functions by setting up a stack frame
    // and then returning. The target of this is always one of these trampolines
    // stored at the tail of the PE. Store the in-memory location of the
    // thunk for fast comparison with relocated addresses.
    pub mem_location: u32,

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

    fn has_trampoline(offset: usize, pe: &peff::PE) -> bool {
        pe.section_info.contains_key(".idata")
            && pe.code.len() >= offset + 6
            && pe.code[offset] == 0xFF
            && pe.code[offset + 1] == 0x25
    }

    fn from_pe(offset: usize, pe: &peff::PE) -> Fallible<Self> {
        ensure!(Self::has_trampoline(offset, pe), "not a trampoline");
        let target = {
            let vp: &[u32] = unsafe { mem::transmute(&pe.code[offset + 2..offset + 6]) };
            vp[0]
        };

        let thunk = Self::find_matching_thunk(target, pe)?;
        let is_data = DATA_RELOCATIONS.contains(&thunk.name);
        Ok(X86Trampoline {
            offset,
            name: thunk.name.clone(),
            target,
            mem_location: SHAPE_LOAD_BASE + offset as u32,
            is_data,
        })
    }

    fn find_matching_thunk<'a>(addr: u32, pe: &'a peff::PE) -> Fallible<&'a peff::Thunk> {
        // The thunk table is code and therefore should have had a relocation entry
        // to move those pointers when we called relocate on the PE.
        trace!(
            "looking for target 0x{:X} in {} thunks",
            addr,
            pe.thunks.len()
        );
        for thunk in pe.thunks.iter() {
            if addr == thunk.vaddr {
                return Ok(thunk);
            }
        }

        // That said, not all SH files actually contain relocations for the thunk
        // targets(!). This is yet more evidence that they're not actually using
        // LoadLibrary to put shapes in memory. They're probably only using the
        // relocation list to rewrite data access with the thunks as tags. We're
        // using relocation, however, to help decode. So if the thunks are not
        // relocated automatically we have to check the relocated value
        // manually.
        let thunk_target = pe.relocate_thunk_pointer(0xAA000000, addr);
        trace!(
            "looking for target 0x{:X} in {} thunks",
            thunk_target,
            pe.thunks.len()
        );
        for thunk in pe.thunks.iter() {
            if thunk_target == thunk.vaddr {
                return Ok(thunk);
            }
        }

        // Also, in USNF, some of the thunks contain the base address already,
        // so treat them like a normal code pointer.
        let thunk_target = pe.relocate_pointer(0xAA000000, addr);
        trace!(
            "looking for target 0x{:X} in {} thunks",
            thunk_target,
            pe.thunks.len()
        );
        for thunk in pe.thunks.iter() {
            if thunk_target == thunk.vaddr {
                return Ok(thunk);
            }
        }

        bail!("did not find thunk with a target of {:08X}", thunk_target)
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

#[derive(Debug, Eq, PartialEq)]
enum ReturnKind {
    Interp,
    Exec,
    Error,
}

#[derive(Debug)]
pub struct X86Code {
    pub offset: usize,
    pub length: usize,
    pub code_offset: usize,
    pub code: Vec<u8>,
    pub formatted: String,
    pub bytecode: ByteCode,
    pub have_header: bool,
}

impl X86Code {
    pub const MAGIC: u8 = 0xF0;

    fn instr_is_relative_jump(instr: &i386::Instr) -> bool {
        match instr.memonic {
            Memonic::Call => true,
            Memonic::Jump => true,
            Memonic::Jcc(ref _cc) => true,
            _ => false,
        }
    }

    fn operand_to_offset(op: &Operand) -> usize {
        // Note that we cannot rely on negative jumps being encoded with a signed instr.
        let delta = match op {
            Operand::Imm32s(delta) => *delta as isize,
            Operand::Imm32(delta) => *delta as i32 as isize,
            _ => {
                trace!("Detected indirect jump target: {}", op);
                0
            }
        };
        if delta < 0 {
            trace!("Skipping loop of {} bytes", delta);
            return 0usize;
        }
        delta as usize
    }

    // Note: excluding return-to-trampoline and loops.
    fn find_external_jumps(base: usize, bc: &ByteCode, external_jumps: &mut HashSet<usize>) {
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

    fn lowest_jump(jumps: &HashSet<usize>) -> usize {
        let mut lowest = usize::max_value();
        for &jump in jumps {
            if jump < lowest {
                lowest = jump;
            }
        }
        lowest
    }

    fn find_pushed_address(target: &i386::Instr) -> Fallible<u32> {
        ensure!(target.memonic == i386::Memonic::Push, "expected push");
        ensure!(target.operands.len() == 1, "expected one operand");
        if let Operand::Imm32s(addr) = target.operands[0] {
            Ok(addr as u32)
        } else {
            bail!("expected imm32s operand")
        }
    }

    fn find_trampoline_for_target(
        target_addr: u32,
        trampolines: &[X86Trampoline],
    ) -> Fallible<&X86Trampoline> {
        for tramp in trampolines {
            trace!(
                "checking {:08X} against {:20} @ loc:{:08X}",
                target_addr,
                tramp.name,
                tramp.mem_location
            );
            if target_addr == tramp.mem_location {
                return Ok(tramp);
            }
        }
        bail!("no matching trampoline for exit")
    }

    fn disassemble_to_ret(
        code: &[u8],
        offset: usize,
        trampolines: &[X86Trampoline],
    ) -> Fallible<(ByteCode, ReturnKind)> {
        let maybe_bc = i386::ByteCode::disassemble_to_ret(SHAPE_LOAD_BASE as usize + offset, code);
        if let Err(e) = maybe_bc {
            i386::DisassemblyError::maybe_show(&e, &code);
            bail!("Don't know how to disassemble at {}: {:?}", offset, e);
        }
        let mut bc = maybe_bc?;
        ensure!(bc.instrs.len() >= 3, "expected at least 3 instructions");

        // Annotate any memory read in this block with the source.
        for instr in bc.instrs.iter_mut() {
            let mut context = None;
            for op in &instr.operands {
                match op {
                    Operand::Memory(ref mr) => {
                        let mt = Self::find_trampoline_for_target(mr.displacement as u32, trampolines);
                        if let Ok(tramp) = mt {
                            context = Some(format!("{}", tramp.name));
                        }
                    }
                    _ => {}
                }
            }
            if let Some(s) = context {
                instr.set_context(&s);
            }
        }

        // Look for the jump target to figure out where we need to continue decoding.
        let target = &bc.instrs[bc.instrs.len() - 2];
        let target_addr = Self::find_pushed_address(target)?;
        let tramp = Self::find_trampoline_for_target(target_addr, trampolines)?;

        let ret_pos = bc.instrs.len() - 1;
        let ret = &mut bc.instrs[ret_pos];
        ensure!(ret.memonic == i386::Memonic::Return, "expected ret");
        ret.set_context(&tramp.name);

        // The argument pointer always points to just after the code segment.
        let arg0 = &bc.instrs[bc.instrs.len() - 3];
        let arg0_ptr = Self::find_pushed_address(arg0)? - SHAPE_LOAD_BASE;
        ensure!(
            arg0_ptr as usize == offset + bc.size as usize,
            "expected second stack arg to point after code block"
        );

        let state = if tramp.name == "do_start_interp" {
            ReturnKind::Interp
        } else if tramp.name == "_ErrorExit" {
            ReturnKind::Error
        } else {
            ReturnKind::Exec
        };

        Ok((bc, state))
    }

    fn make_code(bc: ByteCode, pe: &peff::PE, offset: usize) -> Instr {
        let bc_size = bc.size as usize;
        let have_header = pe.code[offset - 2] == 0xF0;
        let section_offset = if have_header { offset - 2 } else { offset };
        let section_length = bc_size + if have_header { 2 } else { 0 };
        Instr::X86Code(X86Code {
            offset: section_offset,
            length: section_length,
            code_offset: offset,
            code: pe.code[offset..offset + bc_size].to_owned(),
            formatted: Self::format_section(section_offset, section_length, &bc, pe),
            bytecode: bc,
            have_header,
        })
    }

    fn from_bytes(
        _name: &str,
        offset: &mut usize,
        pe: &peff::PE,
        trampolines: &[X86Trampoline],
        trailer: &[Instr],
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
                "top of loop at {:04X} with external jumps: {:?}",
                *offset,
                external_jumps
                    .iter()
                    .map(|n| format!("{:04X}", n))
                    .collect::<Vec<_>>()
            );

            // If we've found an external jump, that's good evidence that this is x86 code, so just
            // go ahead and decode that.
            if external_jumps.contains(&offset) {
                external_jumps.remove(&offset);
                trace!("ip reached external jump");

                let (bc, return_state) =
                    Self::disassemble_to_ret(&pe.code[*offset..], *offset, trampolines)?;
                trace!("decoded {} instructions", bc.instrs.len());

                Self::find_external_jumps(*offset, &bc, &mut external_jumps);
                trace!(
                    "new external jumps: {:?}",
                    external_jumps
                        .iter()
                        .map(|n| format!("{:04X}", n))
                        .collect::<Vec<_>>(),
                );

                let bc_size = bc.size as usize;
                vinstrs.push(Self::make_code(bc, pe, *offset));
                *offset += bc_size;

                // If we're jumping into normal x86 code we should expect to resume
                // running more code right below the call.
                if return_state == ReturnKind::Exec {
                    external_jumps.insert(*offset);
                }

                // TODO: on Error's try to print the message.
            }

            // If we have no more jumps, continue looking for virtual instructions.
            if external_jumps.is_empty()
                || Self::lowest_jump(&external_jumps) < *offset
                || *offset >= pe.code.len()
            {
                trace!("no more external jumps: breaking");
                break;
            }

            // Otherwise, we are between code segments. There may be vinstrs
            // here, or maybe more x86 instructions, or maybe some raw data.
            // Look for a vinstr and it there is one, decode it. Otherwise
            // treat it as raw data.

            // Note: We do not expect another F0 while we have external jumps to find.
            trace!(
                "trying vinstr at: {}",
                bs2s(&pe.code[*offset..(*offset + 10).min(pe.code.len())])
            );
            let saved_offset = *offset;
            let mut have_vinstr = true;
            let maybe = CpuShape::read_instr(offset, pe, trampolines, trailer, vinstrs);
            if let Err(_e) = maybe {
                have_vinstr = false;
            } else if let Some(&Instr::UnknownUnknown(_)) = vinstrs.last() {
                vinstrs.pop();
                *offset = saved_offset;
                have_vinstr = false;
            } else if let Some(&Instr::TrailerUnknown(_)) = vinstrs.last() {
                // We still have external jumps to track down, so our data blob just
                // happened to contain zeros. Keep going.
                vinstrs.pop();
                *offset = saved_offset;
                have_vinstr = false;
            }

            if !have_vinstr && *offset < Self::lowest_jump(&external_jumps) {
                /*
                let maybe_bc = i386::ByteCode::disassemble_one(SHAPE_LOAD_BASE as usize + *offset, &pe.code[*offset..]);
                if let Err(e) = maybe_bc {
                    trace!("offset {:04X}: treating as external data; check this to see if it might actually be bytecode", *offset);
                    i386::DisassemblyError::maybe_show(&e, &pe.code[*offset..]);
                */

                // There is no instruction here, so assume data. Find the closest jump
                // target remaining and fast-forward there.
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
                /*
                } else {
                    // Create an external jump to ourself to continue decoding.
                    external_jumps.insert(*offset);
                }
                */
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

// Something to do with animated textures, probably.
#[derive(Debug)]
pub struct Unk40 {
    pub offset: usize,
    length: usize,
    data: *const u8,

    count: usize,
    frame_offsets: Vec<u16>,
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
        Ok(Unk40 {
            offset,
            length,
            data: data.as_ptr(),

            count,
            frame_offsets: words[1..=count].to_owned(),
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

    pub fn num_frames(&self) -> usize {
        self.count
    }

    pub fn target_for_frame(&self, n: usize) -> usize {
        let n = n % self.count;
        // Base of instr + magic and count + up to this frame + offset at this frame.
        self.offset + 4 + (2 * n) + self.frame_offsets[n] as usize
    }

    fn show(&self) -> String {
        let targets = (0..self.count)
            .map(|i| format!("{:02X}", self.target_for_frame(i)))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "@{:04X} {}Unk40{}: {}{}{}| {}{}{} (cnt:{}, data:({}))",
            self.offset,
            Escape::new().fg(Color::Red).bold(),
            Escape::new(),
            Escape::new().fg(Color::Red).bold(),
            p2s(self.data, 0, 2).trim(),
            Escape::new(),
            Escape::new().fg(Color::Red).dimmed(),
            p2s(self.data, 2, self.length),
            Escape::new(),
            self.count,
            targets
        )
    }
}

#[derive(Debug)]
pub struct UnkF6 {
    pub offset: usize,
    pub data: *const u8,
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
            "@{:04X} {}Unk38{}: {}{}{}   | {}{}{} (?unk0?:{:04X}, ?tgt?:{:04X})",
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
            self.offset + Self::SIZE + self.unk0
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
pub struct PointerToObjectTrailer {
    pub offset: usize,
    data: *const u8,
    pub delta_to_end: usize,
}

impl PointerToObjectTrailer {
    pub const MAGIC: u8 = 0xF2;
    pub const SIZE: usize = 4;

    fn from_bytes(offset: usize, code: &[u8]) -> Fallible<Self> {
        let data = &code[offset..];
        assert_eq!(data[0], Self::MAGIC);
        assert_eq!(data[1], 0x00);
        let word_ref: &[u16] = unsafe { mem::transmute(&data[2..]) };
        let delta_to_end = word_ref[0] as usize;
        Ok(Self {
            offset,
            data: data.as_ptr(),
            delta_to_end,
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

    pub fn end_byte_offset(&self) -> usize {
        // Our start offset + our size + offset_to_next.
        self.offset + Self::SIZE + self.delta_to_end
    }

    pub fn show(&self) -> String {
        format!(
            "@{:04X} {}2EndO{}: {}{}{}| {}{}{} (delta:{:04X}, target:{:04X})",
            self.offset,
            Escape::new().fg(Color::BrightBlue).bold(),
            Escape::new(),
            Escape::new().fg(Color::BrightBlue).bold(),
            p2s(self.data, 0, 2).trim(),
            Escape::new(),
            Escape::new().fg(Color::BrightBlue),
            p2s(self.data, 2, Self::SIZE),
            Escape::new(),
            self.delta_to_end,
            self.end_byte_offset()
        )
    }
}

#[derive(Debug)]
#[allow(non_camel_case_types)]
pub struct UnkAC_ToDamage {
    pub offset: usize,
    data: *const u8,
    pub delta_to_damage: usize,
}

impl UnkAC_ToDamage {
    pub const MAGIC: u8 = 0xAC;
    pub const SIZE: usize = 4;

    fn from_bytes(offset: usize, code: &[u8]) -> Fallible<Self> {
        let data = &code[offset..];
        assert_eq!(data[0], Self::MAGIC);
        assert_eq!(data[1], 0x00);
        let word_ref: &[u16] = unsafe { mem::transmute(&data[2..]) };
        let delta_to_damage = word_ref[0] as usize;
        Ok(Self {
            offset,
            data: data.as_ptr(),
            delta_to_damage,
        })
    }

    fn size(&self) -> usize {
        Self::SIZE
    }

    fn magic(&self) -> &'static str {
        "AC"
    }

    fn at_offset(&self) -> usize {
        self.offset
    }

    pub fn damage_byte_offset(&self) -> usize {
        // Our start offset + our size + offset_to_next.
        self.offset + Self::SIZE + self.delta_to_damage
    }

    pub fn show(&self) -> String {
        format!(
            "@{:04X} {}ToDam{}: {}{}{}| {}{}{} (delta:{:04X}, target:{:04X})",
            self.offset,
            Escape::new().fg(Color::BrightBlue).bold(),
            Escape::new(),
            Escape::new().fg(Color::BrightBlue).bold(),
            p2s(self.data, 0, 2).trim(),
            Escape::new(),
            Escape::new().fg(Color::BrightBlue),
            p2s(self.data, 2, Self::SIZE),
            Escape::new(),
            self.delta_to_damage,
            self.damage_byte_offset()
        )
    }
}

#[derive(Debug)]
#[allow(non_camel_case_types)]
pub struct UnkC8_ToLOD {
    pub offset: usize,
    pub unk0: u16,
    pub unk1: u16,
    pub offset_to_next: usize,
    data: *const u8,
}

impl UnkC8_ToLOD {
    pub const MAGIC: u8 = 0xC8;
    pub const SIZE: usize = 8;

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
        self.offset + Self::SIZE + self.offset_to_next
    }

    pub fn show(&self) -> String {
        format!(
            "@{:04X} {}ToLOD{}: {}{}{}| {}{}{} (unk0:{:04X}, unk1:{:04X} target:{:04X})",
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
            self.next_offset()
        )
    }
}

#[derive(Debug)]
#[allow(non_camel_case_types)]
pub struct UnkA6_ToDetail {
    pub offset: usize,
    data: *const u8,

    pub offset_to_next: usize,

    // This is in the range 1-3, so is probably the game detail level control, rather
    // than a Level-of-Detail control.
    pub level: u16,
}

impl UnkA6_ToDetail {
    pub const MAGIC: u8 = 0xA6;
    pub const SIZE: usize = 6;

    fn from_bytes_after(offset: usize, data: &[u8]) -> Fallible<Self> {
        assert_eq!(data[0], Self::MAGIC);
        assert_eq!(data[1], 0x00);
        let word_ref: &[u16] = unsafe { mem::transmute(&data[2..]) };
        let level = word_ref[1];
        assert!(level >= 1 && level <= 3);
        let offset_to_next = word_ref[0] as usize;
        Ok(Self {
            offset,
            level,
            offset_to_next,
            data: data[0..Self::SIZE].as_ptr(),
        })
    }

    fn size(&self) -> usize {
        Self::SIZE
    }

    fn magic(&self) -> &'static str {
        "A6"
    }

    fn at_offset(&self) -> usize {
        self.offset
    }

    pub fn next_offset(&self) -> usize {
        self.offset + Self::SIZE + self.offset_to_next
    }

    pub fn show(&self) -> String {
        format!(
            "@{:04X} {}ToDtl{}: {}{}{}| {}{}{} (level:{:04X}, target:{:04X})",
            self.offset,
            Escape::new().fg(Color::BrightBlue).bold(),
            Escape::new(),
            Escape::new().fg(Color::BrightBlue).bold(),
            p2s(self.data, 0, 2).trim(),
            Escape::new(),
            Escape::new().fg(Color::BrightBlue),
            p2s(self.data, 2, Self::SIZE),
            Escape::new(),
            self.level,
            self.next_offset()
        )
    }
}

// Always points to a valid instruction. Seems to take some part in
// toggling on parts of the file that are used for showing e.g. gear
// or afterburners in the low-poly versions.
#[derive(Debug)]
pub struct Unk12 {
    pub offset: usize,
    data: *const u8,
    pub offset_to_next: usize,
}

impl Unk12 {
    pub const MAGIC: u8 = 0x12;
    pub const SIZE: usize = 4;

    fn from_bytes(offset: usize, code: &[u8]) -> Fallible<Self> {
        let data = &code[offset..];
        assert_eq!(data[0], Self::MAGIC);
        assert_eq!(data[1], 0x00);
        let word_ref: &[u16] = unsafe { mem::transmute(&data[2..]) };
        let offset_to_next = word_ref[0] as usize;
        Ok(Self {
            offset,
            data: data[0..Self::SIZE].as_ptr(),
            offset_to_next,
        })
    }

    fn size(&self) -> usize {
        Self::SIZE
    }

    fn magic(&self) -> &'static str {
        "12"
    }

    fn at_offset(&self) -> usize {
        self.offset
    }

    pub fn next_offset(&self) -> usize {
        self.offset + Self::SIZE + self.offset_to_next
    }

    pub fn show(&self) -> String {
        format!(
            "@{:04X} {}Unk12{}: {}{}{}| {}{}{} (delta:{:04X}, target:{:04X})",
            self.offset,
            Escape::new().fg(Color::Red).bold(),
            Escape::new(),
            Escape::new().fg(Color::Red).bold(),
            p2s(self.data, 0, 2).trim(),
            Escape::new(),
            Escape::new().fg(Color::Red),
            p2s(self.data, 2, Self::SIZE),
            Escape::new(),
            self.offset_to_next,
            self.next_offset()
        )
    }
}

// Possibly an unconditional jump?
#[derive(Debug)]
pub struct Unk48 {
    pub offset: usize,
    data: *const u8,
    pub offset_to_target: isize,
}

impl Unk48 {
    pub const MAGIC: u8 = 0x48;
    pub const SIZE: usize = 4;

    fn from_bytes(offset: usize, code: &[u8]) -> Fallible<Self> {
        let data = &code[offset..];
        assert_eq!(data[0], Self::MAGIC);
        assert_eq!(data[1], 0x00);
        let word_ref: &[i16] = unsafe { mem::transmute(&data[2..]) };
        let offset_to_target = word_ref[0] as isize;
        Ok(Self {
            offset,
            data: data[0..Self::SIZE].as_ptr(),
            offset_to_target,
        })
    }

    fn size(&self) -> usize {
        Self::SIZE
    }

    fn magic(&self) -> &'static str {
        "48"
    }

    fn at_offset(&self) -> usize {
        self.offset
    }

    pub fn target_offset(&self) -> usize {
        ((self.offset + Self::SIZE) as isize + self.offset_to_target) as usize
    }

    pub fn show(&self) -> String {
        format!(
            "@{:04X} {}Unk48{}: {}{}{}| {}{}{} (tgt:{:04X})",
            self.offset,
            Escape::new().fg(Color::Red).bold(),
            Escape::new(),
            Escape::new().fg(Color::Red).bold(),
            p2s(self.data, 0, 2).trim(),
            Escape::new(),
            Escape::new().fg(Color::Red),
            p2s(self.data, 2, Self::SIZE),
            Escape::new(),
            self.target_offset()
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

    // NOTE: we've got good evidence that this section is intended to be 18
    // bytes long because the weird one in the middle of CITY1 is that long
    // and the vast majority of file endings are 18 bytes followed by 12321.
    pub const SIZE: usize = 18;

    fn from_bytes_after(offset: usize, data: &[u8]) -> Fallible<Self> {
        Ok(Self {
            offset,
            data: data.to_owned(),
        })
        /*
        Ok(Self {
            offset,
            data: (&code[offset..offset + 18]).to_owned()
        })
        */
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

    #[allow(dead_code)]
    fn show_block(&self) -> String {
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

    fn show(&self) -> String {
        format!(
            "{}Trailer{}: {:04} {}{}{}",
            Escape::new().fg(Color::Red).bold(),
            Escape::new(),
            self.data.len(),
            Escape::new().fg(Color::Red).bold(),
            bs2s(&self.data),
            Escape::new(),
        )
    }
}

// Typically 00+ 01 02 03 02 01 00*, but not always
#[derive(Debug)]
pub struct EndOfShape {
    pub offset: usize,
    pub data: Vec<u8>,
}

impl EndOfShape {
    fn from_bytes_after(offset: usize, data: &[u8]) -> Fallible<Self> {
        Ok(Self {
            offset,
            data: data.to_owned(),
        })
    }

    fn size(&self) -> usize {
        self.data.len()
    }

    fn magic(&self) -> &'static str {
        "EndOfShape"
    }

    fn at_offset(&self) -> usize {
        self.offset
    }

    fn show(&self) -> String {
        format!(
            "@{:04X} {}EndSh{}: {}{}{}| {}{}{}",
            self.offset,
            Escape::new().fg(Color::Green).bold(),
            Escape::new(),
            Escape::new().fg(Color::Green).bold(),
            bs2s(&self.data[0..2]).trim(),
            Escape::new(),
            Escape::new().fg(Color::Green),
            bs2s(&self.data[2..]),
            Escape::new()
        )
    }
}

// Typically 00+ 01 02 03 02 01 00*, but not always
#[derive(Debug)]
pub struct EndOfObject {
    pub offset: usize,
    pub data: *const u8,
}

impl EndOfObject {
    const SIZE: usize = 18;

    fn from_bytes_after(offset: usize, data: &[u8]) -> Fallible<Self> {
        Ok(Self {
            offset,
            data: data.as_ptr(),
        })
    }

    fn size(&self) -> usize {
        Self::SIZE
    }

    fn magic(&self) -> &'static str {
        "EndOfObject"
    }

    fn at_offset(&self) -> usize {
        self.offset
    }

    fn show(&self) -> String {
        format!(
            "@{:04X} {}EdObj{}: {}{}{}| {}{}{}",
            self.offset,
            Escape::new().fg(Color::Green).bold(),
            Escape::new(),
            Escape::new().fg(Color::Green).bold(),
            p2s(self.data, 0, 2).trim(),
            Escape::new(),
            Escape::new().fg(Color::Green),
            p2s(self.data, 2, Self::SIZE),
            Escape::new()
        )
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
        if self.length < 4 {
            let sections = vec![Section::new(0x0000, 0, self.length)];
            let bytes =
                format_sections(&self.data, &sections, &mut vec![], &ShowMode::AllPerLine).join("");
            return format!(
                "@{:04X} {}Datas{}: {}",
                self.offset,
                Escape::new().fg(Color::Red),
                Escape::new(),
                bytes
            );
        }

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
        while offset + cnt < code.len() && code[offset + cnt] == 0x1E {
            cnt += 1;
        }
        assert!(cnt > 0);
        Ok(Pad1E {
            offset,
            length: cnt,
            data: (&code[offset..]).as_ptr(),
        })
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

            fn from_bytes_after(offset: usize, data: &[u8]) -> Fallible<Self> {
                assert_eq!(data[0], Self::MAGIC);
                ensure!(
                    data[1] == 0 || data[1] == 0xFF,
                    "not a word code instruction"
                );
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
opaque_instr!(Unk68, "68", 0x68, 8); // CHAFF / CATGUY (2 instances)
opaque_instr!(Unk74, "74", 0x74, 8); // CHAFF / DEBRIS (3 instance)

// 6E 00| A5 30 00 00 
// 6E 00| 06 00 00 00 50 00 73 00 00 00
opaque_instr!(Unk6E, "6E", 0x6E, 6); // FA:F8.SH
opaque_instr!(Unk50, "50", 0x50, 6); // FA:F8.SH

opaque_instr!(Header, "Header", 0xFF, 14);
opaque_instr!(Unk2E, "2E", 0x2E, 4);
opaque_instr!(Unk3A, "3A", 0x3A, 6);
opaque_instr!(Unk44, "44", 0x44, 4);
opaque_instr!(Unk46, "46", 0x46, 2);
opaque_instr!(Unk66, "66", 0x66, 10);
opaque_instr!(Unk72, "72", 0x72, 4);
opaque_instr!(Unk78, "78", 0x78, 12);
opaque_instr!(Unk7A, "7A", 0x7A, 10);
opaque_instr!(Unk96, "96", 0x96, 6);
opaque_instr!(UnkB8, "B8", 0xB8, 4);
opaque_instr!(UnkC4, "C4", 0xC4, 16);
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
    Unk68(Unk68),
    Unk6C(Unk6C),
    Unk6E(Unk6E),
    Unk50(Unk50),
    Unk72(Unk72),
    Unk74(Unk74),
    Unk78(Unk78),
    Unk7A(Unk7A),
    Unk96(Unk96),
    UnkA6_ToDetail(UnkA6_ToDetail),
    UnkAC_ToDamage(UnkAC_ToDamage),
    UnkB2(UnkB2),
    UnkB8(UnkB8),
    UnkC4(UnkC4),
    UnkC8_ToLOD(UnkC8_ToLOD),
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
    PointerToObjectTrailer(PointerToObjectTrailer),

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

    EndOfObject(EndOfObject),
    EndOfShape(EndOfShape),
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
            Instr::Unk68(ref i) => i.$f(),
            Instr::Unk6C(ref i) => i.$f(),
            Instr::Unk6E(ref i) => i.$f(),
            Instr::Unk50(ref i) => i.$f(),
            Instr::Unk72(ref i) => i.$f(),
            Instr::Unk74(ref i) => i.$f(),
            Instr::Unk78(ref i) => i.$f(),
            Instr::Unk7A(ref i) => i.$f(),
            Instr::Unk96(ref i) => i.$f(),
            Instr::UnkA6_ToDetail(ref i) => i.$f(),
            Instr::UnkAC_ToDamage(ref i) => i.$f(),
            Instr::UnkB2(ref i) => i.$f(),
            Instr::UnkB8(ref i) => i.$f(),
            Instr::UnkC4(ref i) => i.$f(),
            Instr::UnkC8_ToLOD(ref i) => i.$f(),
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
            Instr::PointerToObjectTrailer(ref i) => i.$f(),
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
            Instr::EndOfObject(ref i) => i.$f(),
            Instr::EndOfShape(ref i) => i.$f(),
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
    ($name:ident, $pe:ident, $offset:ident, $end_offset:ident, $instrs:ident) => {{
        let instr = $name::from_bytes(*$offset, &$pe.code[..$end_offset])?;
        *$offset += instr.size();
        $instrs.push(Instr::$name(instr));
    }};
}

macro_rules! consume_instr_simple {
    ($name:ident, $offset:ident, $data:expr, $instrs:ident) => {{
        let instr = $name::from_bytes_after(*$offset, $data)?;
        *$offset += instr.size();
        $instrs.push(Instr::$name(instr));
    }};
}

pub struct CpuShape {
    pub instrs: Vec<Instr>,
    pub trampolines: Vec<X86Trampoline>,
    offset_map: HashMap<usize, usize>,
    pub pe: peff::PE,
}

impl CpuShape {
    pub fn from_bytes(data: &[u8]) -> Fallible<Self> {
        let mut pe = peff::PE::parse(data)?;

        // Do default relocation to a high address. This makes offsets appear
        // 0-based and tags all local pointers with an obvious flag.
        pe.relocate(SHAPE_LOAD_BASE)?;
        let trampolines = Self::find_trampolines(&pe)?;
        let eos = Self::find_end_of_shape(&pe, &trampolines)?;
        let mut trailer = trampolines
            .iter()
            .map(|t| Instr::X86Trampoline(t.to_owned()))
            .collect::<Vec<_>>();
        trailer.insert(0, Instr::EndOfShape(eos));

        let mut instrs = Self::read_sections(&pe, &trampolines, &trailer)?;
        instrs.append(&mut trailer);

        // References inside shape are relative byte offsets. We map these
        // to absolute byte offsets using the instruction offset and size
        // which we can look up in the following table to get the instr
        // index we need to jump to.
        let mut offset_map = HashMap::new();
        for (i, instr) in instrs.iter().enumerate() {
            offset_map.insert(instr.at_offset(), i);
        }

        Ok(CpuShape {
            instrs,
            trampolines,
            offset_map,
            pe,
        })
    }

    pub fn bytes_to_index(&self, absolute_byte_offset: usize) -> Fallible<usize> {
        // FIXME: we need to handle ERRATA here?
        Ok(*self.offset_map.get(&absolute_byte_offset).ok_or_else(|| {
            err_msg(format!(
                "absolute byte offset {:04X} does not point to an instruction",
                absolute_byte_offset
            ))
        })?)
    }

    pub fn all_textures(&self) -> HashSet<String> {
        let mut uniq = HashSet::new();
        for instr in &self.instrs {
            if let Instr::TextureRef(tex) = instr {
                uniq.insert(tex.filename.to_owned());
            }
        }
        uniq
    }

    fn find_trampolines(pe: &peff::PE) -> Fallible<Vec<X86Trampoline>> {
        if !pe.thunks.is_empty() {
            trace!("Looking for thunks in the following table:");
            for thunk in &pe.thunks {
                trace!("    {:20} @ 0x{:X}", thunk.name, thunk.vaddr);
            }
        }
        let mut offset = pe.code.len() - 6;
        let mut trampolines = Vec::new();
        while offset > 0 {
            if X86Trampoline::has_trampoline(offset, pe) {
                let tramp = X86Trampoline::from_pe(offset, pe)?;
                trace!("found trampoline: {}", tramp.show());
                trampolines.push(tramp);
            } else {
                break;
            }
            offset -= 6;
        }
        trampolines.reverse();
        Ok(trampolines)
    }

    fn find_end_of_shape(pe: &peff::PE, trampolines: &[X86Trampoline]) -> Fallible<EndOfShape> {
        let end_offset = pe.code.len() - trampolines.len() * X86Trampoline::SIZE;
        let mut offset = end_offset - 1;
        while pe.code[offset] == 0 {
            offset -= 1;
        }
        fn is_end(p: &[u8]) -> bool {
            p[0] == 1 && p[1] == 2 && p[2] == 3 && p[3] == 2 && p[4] == 1
        }
        offset -= 4;
        ensure!(is_end(&pe.code[offset..]), "expected 12321 sequence right before trampolines");
        while is_end(&pe.code[offset - 4..]) {
            offset -= 4;
        }
        EndOfShape::from_bytes_after(offset, &pe.code[offset..end_offset])
    }

    fn end_size(trailer: &[Instr]) -> usize {
        let mut sum = 0;
        for i in trailer {
            sum += i.size();
        }
        sum
    }

    fn read_sections(pe: &peff::PE, trampolines: &[X86Trampoline], trailer: &[Instr]) -> Fallible<Vec<Instr>> {
        let mut offset = 0;
        let mut instrs = Vec::new();
        let end_offset = pe.code.len() - Self::end_size(trailer);
        while offset < end_offset {
            // trace!(
            //     "Decoding At: {:04X}: {}",
            //     offset,
            //     bs2s(&pe.code[offset..cmp::min(pe.code.len(), offset + 20)])
            // );
            //assert!(ALL_OPCODES.contains(&pe.code[offset]));
            Self::read_instr(&mut offset, pe, trampolines, trailer, &mut instrs)?;
            trace!("=>: {}", instrs.last().unwrap().show());
        }

        // Assertions.
        //        {
        //            let instr = find_first_instr(0xF2, &instrs);
        //            if let Some(&Instr::PointerToObjectTrailer(ref jmp)) = instr {
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
        trailer: &[Instr],
        instrs: &mut Vec<Instr>,
    ) -> Fallible<()> {
        let end_offset = pe.code.len() - Self::end_size(trailer);
        match pe.code[*offset] {
            Header::MAGIC => consume_instr_simple!(Header, offset, &pe.code[*offset..end_offset], instrs),
            Unk08::MAGIC => consume_instr_simple!(Unk08, offset, &pe.code[*offset..end_offset], instrs),
            Unk0E::MAGIC => consume_instr_simple!(Unk0E, offset, &pe.code[*offset..end_offset], instrs),
            Unk10::MAGIC => consume_instr_simple!(Unk10, offset, &pe.code[*offset..end_offset], instrs),
            Unk2E::MAGIC => consume_instr_simple!(Unk2E, offset, &pe.code[*offset..end_offset], instrs),
            Unk3A::MAGIC => consume_instr_simple!(Unk3A, offset, &pe.code[*offset..end_offset], instrs),
            Unk44::MAGIC => consume_instr_simple!(Unk44, offset, &pe.code[*offset..end_offset], instrs),
            Unk46::MAGIC => consume_instr_simple!(Unk46, offset, &pe.code[*offset..end_offset], instrs),
            Unk4E::MAGIC => consume_instr_simple!(Unk4E, offset, &pe.code[*offset..end_offset], instrs),
            Unk66::MAGIC => consume_instr_simple!(Unk66, offset, &pe.code[*offset..end_offset], instrs),
            Unk68::MAGIC => consume_instr_simple!(Unk68, offset, &pe.code[*offset..end_offset], instrs),
            Unk6C::MAGIC => consume_instr_simple!(Unk6C, offset, &pe.code[*offset..end_offset], instrs),
            Unk6E::MAGIC => consume_instr_simple!(Unk6E, offset, &pe.code[*offset..end_offset], instrs),
            Unk50::MAGIC => consume_instr_simple!(Unk50, offset, &pe.code[*offset..end_offset], instrs),
            Unk72::MAGIC => consume_instr_simple!(Unk72, offset, &pe.code[*offset..end_offset], instrs),
            Unk74::MAGIC => consume_instr_simple!(Unk74, offset, &pe.code[*offset..end_offset], instrs),
            Unk78::MAGIC => consume_instr_simple!(Unk78, offset, &pe.code[*offset..end_offset], instrs),
            Unk7A::MAGIC => consume_instr_simple!(Unk7A, offset, &pe.code[*offset..end_offset], instrs),
            Unk96::MAGIC => consume_instr_simple!(Unk96, offset, &pe.code[*offset..end_offset], instrs),
            UnkA6_ToDetail::MAGIC => consume_instr_simple!(UnkA6_ToDetail, offset, &pe.code[*offset..end_offset], instrs),
            UnkB2::MAGIC => consume_instr_simple!(UnkB2, offset, &pe.code[*offset..end_offset], instrs),
            UnkB8::MAGIC => consume_instr_simple!(UnkB8, offset, &pe.code[*offset..end_offset], instrs),
            UnkC4::MAGIC => consume_instr_simple!(UnkC4, offset, &pe.code[*offset..end_offset], instrs),
            UnkCA::MAGIC => consume_instr_simple!(UnkCA, offset, &pe.code[*offset..end_offset], instrs),
            UnkD0::MAGIC => consume_instr_simple!(UnkD0, offset, &pe.code[*offset..end_offset], instrs),
            UnkD2::MAGIC => consume_instr_simple!(UnkD2, offset, &pe.code[*offset..end_offset], instrs),
            UnkDA::MAGIC => consume_instr_simple!(UnkDA, offset, &pe.code[*offset..end_offset], instrs),
            UnkDC::MAGIC => consume_instr_simple!(UnkDC, offset, &pe.code[*offset..end_offset], instrs),
            UnkE4::MAGIC => consume_instr_simple!(UnkE4, offset, &pe.code[*offset..end_offset], instrs),
            UnkE6::MAGIC => consume_instr_simple!(UnkE6, offset, &pe.code[*offset..end_offset], instrs),
            UnkE8::MAGIC => consume_instr_simple!(UnkE8, offset, &pe.code[*offset..end_offset], instrs),
            UnkEA::MAGIC => consume_instr_simple!(UnkEA, offset, &pe.code[*offset..end_offset], instrs),
            UnkEE::MAGIC => consume_instr_simple!(UnkEE, offset, &pe.code[*offset..end_offset], instrs),

            Unk06::MAGIC => consume_instr!(Unk06, pe, offset, end_offset, instrs),
            Unk0C::MAGIC => consume_instr!(Unk0C, pe, offset, end_offset, instrs),
            Unk12::MAGIC => consume_instr!(Unk12, pe, offset, end_offset, instrs),
            Pad1E::MAGIC => consume_instr!(Pad1E, pe, offset, end_offset, instrs),
            Unk40::MAGIC => consume_instr!(Unk40, pe, offset, end_offset, instrs),
            Unk48::MAGIC => consume_instr!(Unk48, pe, offset, end_offset, instrs),
            UnkAC_ToDamage::MAGIC => consume_instr!(UnkAC_ToDamage, pe, offset, end_offset, instrs),
            UnkBC::MAGIC => consume_instr!(UnkBC, pe, offset, end_offset, instrs),
            UnkC8_ToLOD::MAGIC => {
                consume_instr!(UnkC8_ToLOD, pe, offset, end_offset, instrs)
            }
            UnkCE::MAGIC => consume_instr!(UnkCE, pe, offset, end_offset, instrs),
            UnkF6::MAGIC => consume_instr!(UnkF6, pe, offset, end_offset, instrs),
            PointerToObjectTrailer::MAGIC => consume_instr!(PointerToObjectTrailer, pe, offset, end_offset, instrs),
            Unk38::MAGIC => consume_instr!(Unk38, pe, offset, end_offset, instrs),
            TextureRef::MAGIC => consume_instr!(TextureRef, pe, offset, end_offset, instrs),
            TextureIndex::MAGIC => consume_instr!(TextureIndex, pe, offset, end_offset, instrs),
            SourceRef::MAGIC => consume_instr!(SourceRef, pe, offset, end_offset, instrs),
            VertexBuf::MAGIC => consume_instr!(VertexBuf, pe, offset, end_offset, instrs),
            Facet::MAGIC => consume_instr!(Facet, pe, offset, end_offset, instrs),
            X86Code::MAGIC => {
                let name = if let Some(&Instr::SourceRef(ref source)) = find_first_instr(0x42, &instrs) {
                    source.source.clone()
                } else {
                    "unknown_source".to_owned()
                };
                X86Code::from_bytes(&name, offset, pe, trampolines, trailer, instrs)?;
            }
            // Zero is the magic for the trailer (sans trampolines).
            0 => {
                // ERRATA: in SOLDIER.SH, and USNF:CATGUY.SH, the F2 trailer target indicator
                // is 1 word after the real trailer start. Dump an UnknownData to put in sync.
                if pe.code[*offset + 1] == 0x00 {
                    let mut target = None;
                    {
                        if let Some(&Instr::PointerToObjectTrailer(ref end_ptr)) = find_first_instr(0xF2, &instrs) {
                            target = Some(end_ptr.end_byte_offset());
                        }
                    }
                    if target == Some(*offset + 2) {
                        trace!("skipping two null bytes before trailer");
                        instrs.push(Instr::UnknownData(UnknownData {
                            offset: *offset,
                            length: 2,
                            data: pe.code[*offset..*offset + 2].to_vec(),
                        }));
                        *offset += 2;
                        return Ok(());
                    }
                }

                // ERRATA: in CATGUY.SH after USNF, there is a big block of ??? between the last
                // recognizable instruction and the target of the F2. These are all the same file
                // so we can just look for the offset 0x182 and 0x208.
                if pe.code[*offset + 1] == 0x00 {
                    let mut target = None;
                    {
                        if let Some(&Instr::PointerToObjectTrailer(ref end_ptr)) = find_first_instr(0xF2, &instrs) {
                            target = Some(end_ptr.end_byte_offset());
                        }
                    }

                    if *offset == 0x182 && target == Some(0x208) {
                        trace!("skipping the weird bit of CATGUY.SH that we don't understand");
                        instrs.push(Instr::UnknownData(UnknownData {
                            offset: *offset,
                            length: target.unwrap() - *offset,
                            data: pe.code[*offset..target.unwrap()].to_vec(),
                        }));
                        *offset = target.unwrap();
                        return Ok(());
                    }
                }

                let remaining = &pe.code[*offset..end_offset];
                if remaining.len() < 18 {
                    // If we're just out of space... :shrug:
                    let unk = TrailerUnknown::from_bytes_after(*offset, remaining)?;
                    instrs.push(Instr::TrailerUnknown(unk));
                    *offset = end_offset;
                } else if remaining[16] == 0 && remaining[17] == 0 {
                    // Cases were the block should stop rendering look more or less like:
                    // 00 .. .. .. .. .. .. .. .. .. .. .. .. .. .. .. 00 00
                    let obj_end = EndOfObject::from_bytes_after(*offset, &remaining[..EndOfObject::SIZE])?;
                    instrs.push(Instr::EndOfObject(obj_end));
                    // These may occur in the middle of shapes, so we need to keep going.
                    *offset += EndOfObject::SIZE;
                } else {
                    // Other instances of 0 we expect to end the file.
                    let unk = TrailerUnknown::from_bytes_after(*offset, remaining)?;
                    instrs.push(Instr::TrailerUnknown(unk));
                    *offset = end_offset;
                }
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
        for (instr_offset, instr) in self.instrs.iter().enumerate() {
            if instr.at_offset() == abs_offset {
                return Ok(instr_offset);
            }
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
    let expect = format!("{:02X}", kind);
    for instr in instrs.iter() {
        if expect == instr.magic() {
            return Some(instr);
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
    use std::io::prelude::*;
    use std::{collections::HashMap, fs};

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

    fn find_f2_target(shape: &CpuShape) -> Option<usize> {
        for instr in shape.instrs.iter().rev() {
            if let Instr::PointerToObjectTrailer(f2) = instr {
                return Some(f2.end_byte_offset());
            }
        }
        None
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
        let mut freqs = freq
            .iter()
            .map(|(&k, &v)| (k, v))
            .collect::<Vec<(&'static str, usize)>>();
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
            let shape = CpuShape::from_bytes(&data)?;

            //compute_instr_freqs(&shape, &mut freq);

            // Ensure that f2 points to the trailer if it exists.
            // And conversely that we found the trailer in the right place.
            if let Some(offset) = offset_of_trailer(&shape) {
                if let Some(f2_target) = find_f2_target(&shape) {
                    assert_eq!(offset, f2_target);
                }
            }

            // Ensure that all Unk12 and Unk48 point to a valid instruction.
            for instr in &shape.instrs {
                match instr {
                    Instr::Unk12(unk) => {
                        let index = shape.bytes_to_index(unk.next_offset())?;
                        let _target_instr = &shape.instrs[index];
                    }
                    Instr::Unk48(unk) => {
                        let index = shape.bytes_to_index(unk.target_offset())?;
                        let _target_instr = &shape.instrs[index];
                    }
                    _ => {}
                }
            }

            /*
            let mut offset = 0;
            let mut offsets = Vec::new();
            for instr in &shape.instrs {
                if let Instr::UnkC8_ToLOD(c8) = instr {
                    offsets.push(c8.next_offset());
                }
                if offsets.contains(&offset) {
                    println!("TARGET: {}", instr.show());
                }
                offset += instr.size();
            }
            */
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
    #[ignore] // We don't actually know what the memory should look like past this.
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

        let shape = CpuShape::from_bytes(&data).unwrap();
        let mut interp = i386::Interpreter::new();
        for tramp in shape.trampolines.iter() {
            if !tramp.is_data {
                interp.add_trampoline(tramp.mem_location, &tramp.name, 1);
                continue;
            }
            match tramp.name.as_ref() {
                "brentObjId" => interp.add_read_port(
                    tramp.mem_location,
                    Box::new(move || {
                        println!("LOOKUP brentObjectId");
                        exp_base // Lowest valid (shown?) object id is 0x3E8, at least by the check EXP.sh does.
                    }),
                ),
                "_currentTicks" => interp.add_read_port(
                    tramp.mem_location,
                    Box::new(move || {
                        println!("LOOKUP _currentTicks");
                        0
                    }),
                ),
                "viewer_x" => interp.add_read_port(
                    tramp.mem_location,
                    Box::new(move || {
                        println!("LOOKUP viewer_x");
                        0
                    }),
                ),
                "viewer_z" => interp.add_read_port(
                    tramp.mem_location,
                    Box::new(move || {
                        println!("LOOKUP viewer_z");
                        0
                    }),
                ),
                "xv32" => interp.add_read_port(
                    tramp.mem_location,
                    Box::new(move || {
                        println!("LOOKUP xv32");
                        0
                    }),
                ),
                "zv32" => interp.add_read_port(
                    tramp.mem_location,
                    Box::new(move || {
                        println!("LOOKUP zv32");
                        0
                    }),
                ),
                "_effectsAllowed" => {
                    interp.add_read_port(
                        tramp.mem_location,
                        Box::new(move || {
                            println!("LOOKUP _effectsAllowed");
                            0
                        }),
                    );
                    interp.add_write_port(
                        tramp.mem_location,
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
