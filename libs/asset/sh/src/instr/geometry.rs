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
use crate::instr::read_name;
use ansi::ansi;
use anyhow::{bail, ensure, Result};
use bitflags::bitflags;
use packed_struct::packed_struct;
use reverse::p2s;
use std::{mem, slice::Iter};

#[derive(Debug)]
pub struct TextureRef {
    pub offset: usize,
    pub filename: String,
}

impl TextureRef {
    pub const MAGIC: u8 = 0xE2;
    pub const SIZE: usize = 16;

    pub fn from_bytes_after(offset: usize, data: &[u8]) -> Result<Self> {
        assert_eq!(data[0], Self::MAGIC);
        assert_eq!(data[1], 0);
        let filename = read_name(&data[2..Self::SIZE])?;
        Ok(TextureRef { offset, filename })
    }

    pub fn size(&self) -> usize {
        Self::SIZE
    }

    pub fn magic(&self) -> &'static str {
        "E2"
    }

    pub fn at_offset(&self) -> usize {
        self.offset
    }

    pub fn show(&self) -> String {
        format!(
            "@{:04X} {}TexRf{}: {}{}{}",
            self.offset,
            ansi().yellow().bold(),
            ansi(),
            ansi().yellow(),
            self.filename,
            ansi(),
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
    pub fn from_u16(kind: u16) -> Result<Self> {
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

    pub fn from_bytes_after(offset: usize, data: &[u8]) -> Result<Self> {
        assert_eq!(data[0], Self::MAGIC);
        let data2: &[u16] = unsafe { mem::transmute(&data[2..]) };
        Ok(TextureIndex {
            offset,
            unk0: data[1],
            kind: TextureIndexKind::from_u16(data2[0])?,
        })
    }

    pub fn size(&self) -> usize {
        Self::SIZE
    }

    pub fn magic(&self) -> &'static str {
        "E0"
    }

    pub fn at_offset(&self) -> usize {
        self.offset
    }

    pub fn show(&self) -> String {
        format!(
            "TextureIndexKind @ {:04X}: {}, {:?}",
            self.offset, self.unk0, self.kind
        )
    }
}

#[derive(Debug)]
pub struct VertexBuf {
    offset: usize,
    target_offset: usize,
    pub verts: Vec<[i16; 3]>,
}

impl VertexBuf {
    pub const MAGIC: u8 = 0x82;

    pub fn from_bytes_after(offset: usize, data: &[u8]) -> Result<Self> {
        assert_eq!(data[0], Self::MAGIC);
        assert_eq!(data[1], 0);
        let head: &[u16] = unsafe { mem::transmute(&data[2..6]) };
        let words: &[i16] = unsafe { mem::transmute(&data[6..]) };
        let nverts = head[0] as usize;
        let target_offset = head[1] as usize;
        ensure!(
            target_offset % 8 == 0,
            "expected the vert buffer target offset to be a multiple of 8"
        );
        let mut buf = VertexBuf {
            offset,
            target_offset: target_offset / 8,
            verts: Vec::with_capacity(nverts),
        };
        for i in 0..nverts {
            let x = words[i * 3];
            let y = words[i * 3 + 1];
            let z = words[i * 3 + 2];
            buf.verts.push([x, y, z]);
        }
        Ok(buf)
    }

    pub fn size(&self) -> usize {
        6 + self.verts.len() * 6
    }

    pub fn magic(&self) -> &'static str {
        "82"
    }

    pub fn at_offset(&self) -> usize {
        self.offset
    }

    // In number of vertices in the pool (given 8 byte alignment).
    pub fn buffer_target_offset(&self) -> usize {
        self.target_offset
    }

    pub fn vertices(&self) -> Iter<'_, [i16; 3]> {
        self.verts.iter()
    }

    pub fn show(&self) -> String {
        let s = self
            .verts
            .iter()
            .map(|v| format!("({},{},{})", v[0], v[1], v[2]))
            .collect::<Vec<String>>()
            .join(", ");
        format!(
            "@{:04X} {}VxBuf: 82 00{}| {}{:04X}{} => {}verts -> {}{}{}",
            self.offset,
            ansi().magenta().bold(),
            ansi(),
            ansi().magenta(),
            self.target_offset,
            ansi(),
            self.verts.len(),
            ansi().magenta().dimmed(),
            s,
            ansi(),
        )
    }
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

    _indices_count_pointer: *const u8,
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
    // FC 0b0000_0000_0010_0100  00   FC                                       00
    // FC 0b0000_0001_0000_0000  9F   00                                       04   16 16 22 22
    // FC 0b0000_0001_0000_0010  9F   00                                       04   19 19 2A 2A
    // FC 0b0000_0100_0000_0011  00   00                                       03   26 29 26                B8   B4    C3   CA    B9   B3
    // FC 0b0100_0001_0000_0000  6D   00 11 08 91 7F 93 06 1D 00 FB FF 30 01   03   02 01 0B
    // FC 0b0100_0001_0000_0010  5B   00 00 00 00 00 FD 7F 02 FE F3            04   13 24 23 05
    // FC 0b0100_0001_0000_0110  9F   00 00 00 BE 0E 23 7F 00 0A 1E            04   4D00 4000 0701 0801
    // FC 0b0100_0001_0000_1010  A0   00 D1 7F A8 06 00 00 E3 FB 00            03   03 0A 09
    // FC 0b0100_0100_0000_0000  00   00 9F B9 E7 6A 00 00 5E FF F4 FF C5 FF   04   0E 0B 0A 0F           0000 2101  0000 4A01  8E00 4A01  8E00 2101
    // FC 0b0100_0100_0000_0000  00   00 B4 93 F1 C4 21 22 CB FF 16 00 E9 00   03   00 01 02              4C00 6C02  4C00 8202  0000 6C02
    // FC 0b0100_0100_0000_0001  00   00 00 00 00 00 03 80 00 00 DA FF 66 FF   04   00 01 02 03             04   07    2E   2D    54   2D    7E   07
    // FC 0b0100_0100_0000_0010  00   00 00 00 00 00 03 80 01 FA D9            04   06 07 08 09           0C00 E500  0C00 2A01  5B00 2A01  5A00 E500
    // FC 0b0100_0100_0000_0011  00   00 00 00 00 00 03 80 00 02 FA            04   04 06 07 05             C0   39    F6   39    F6   22    C0   22
    // FC 0b0100_0100_0000_0100  00   00 03 80 00 00 00 00 AF 00 C7 FF 2D 01   04   FE00 FF00 0001 0101   0400 2C01  4C00 2C01  4C00 BB00  0400 BB00
    // FC 0b0100_0100_0000_0101  00   00 00 00 FD 7F 00 00 DB 00 DC FF 2D 01   04   0701 0601 0301 0201     61   80    61   38    04   38    04   80
    // FC 0b0100_0100_0000_0111  00   00 00 00 3F 77 7E 2E FD 07 30            04   0101 0201 FA00 FF00    46 82 46 67 00 66 00 84
    // FC 0b0100_0100_0000_1001  00   00 00 00 FD 7F 00 00 CB FE 00 00 6A 02   04   6E 20 6D 6F             00   D2    76   D2    76   5C    00   5C
    // FC 0b0100_0100_0000_1011  00   00 00 00 62 73 9E C8 00 07 F7            04   03 06 07 00             35   1C    35   02    00   02    00   1C
    // FC 0b0100_1001_0000_0000  5D   00 00 00 00 00 FD 7F 91 00 D4 FF A4 00   04   00 04 07 05
    // FC 0b0100_1001_0000_0010  52   00 0A 60 F8 AF 70 1B 01 FF 1A            04   04 06 02 01
    // FC 0b0100_1100_0000_0000  00   00 00 00 00 00 FD 7F D9 FE BF FF 67 00   04   1B 1A 19 1C           0000 0C04  8C00 0C04  8C00 9B03  0000 9B03
    // FC 0b0100_1100_0000_0001  00   00 00 00 00 00 03 80 7F 00 7C FF 97 FF   04   7A 72 79 82             A6   7B    F0   7C    F0   BC    A6   BC
    // FC 0b0100_1100_0000_0010  00   00 00 00 00 00 03 80 01 E2 AA            04   04 02 01 05           0400 A601  8000 A601  8000 4901  0400 4901
    // FC 0b0100_1100_0000_0011  00   00 00 00 00 00 03 80 00 01 01            04   3B 3A 3D 3C             5B   05    34   05    34   23    5B   23
    // FC 0b0100_1100_0000_0110  6B   00 3E 70 00 00 81 3D 09 F7 32            04   0301 EB00 EA00 0201   2900 7F01 2900 8801 2100 8801 2100 7F01
    // FC 0b0100_1100_0000_0111  00   00 00 00 FD 7F 00 00 00 13 26            04   7200 0001 0101 7300   C2 E5 C2 FF FC FF FC E6
    // FC 0b0100_1100_0000_1011  00   00 00 00 7A 72 3D 39 00 09 09            04   03 02 01 00           00 24 00 3C 35 3C 35 24
    // FC 0b0101_0001_0000_0000  90   00 00 00 FD 7F 00 00 85 02 FF FF 7D FD   04   DF E0 E1 E2
    // FC 0b0101_0001_0000_0010  90   00 00 00 FD 7F 00 00 77 00 6A            04   99 9A 9B 9C
    // FC 0b0101_0001_0000_0100  AF   00 00 00 FD 7F 00 00 55 00 00 00 D7 01   04   FD00 FE00 FF00 0001
    // FC 0b0101_0001_0000_1000  44   00 D0 9D B6 3B 5A 38 4D 00 1A 00 61 FF   04   03 05 06 00
    // FC 0b0101_0001_0000_1010  91   00 22 E2 74 7C 00 00 FF 18 00            04   04 05 01 00
    // FC 0b0101_0001_0000_1100  92   00 F9 7F 0D 02 00 00 CD FF 3E 00 71 FF   04   1601 1701 1801 1101
    // FC 0b0101_0100_0000_0000  00   00 00 00 FD 7F 00 00 19 02 00 00 5C 00   04   43 47 48 44           9500 0202  9500 6F01  0300 7001  0300 0202
    // FC 0b0101_0100_0000_0001  00   00 00 00 FD 7F 00 00 30 02 00 00 CD FE   04   6C 6D 6B 6A             FA   75    97   75    97   D6    FA   D7
    // FC 0b0101_0100_0000_0010  00   00 00 00 FD 7F 00 00 C3 FF 09            04   0F 10 11 12           FC00 5A01 9900 5B01 9900 BC01 FC00 BC01
    // FC 0b0101_0100_0000_0011  00   00 00 00 FD 7F 00 00 A1 00 4E            04   32 33 31 30           3A 30 00 32 00 AA 3A AA
    // FC 0b0101_0100_0000_0111  00   00 FD 7F 00 00 00 00 30 67 DF            04   1D01 1E01 1F01 2001   2F 29 1D 2A 1D 1E 2F 1E
    // FC 0b0101_0100_0000_1000  00   00 00 00 FD 7F 00 00 39 00 18 00 77 FF   05   0A 5C 59 68 3F        3E00 5B01  0C00 7601  0000 9701  0D00 9701  5E00 9701
    // FC 0b0101_0100_0000_1001  00   00 00 00 00 00 FD 7F C9 FF B7 FF 4E 01   04   00 01 02 03             DE   03    DE   1B    F4   1B    F4   03
    // FC 0b0101_0100_0000_1010  00   00 31 80 3D F9 00 00 B4 84 81            04   3D 3C 38 37           0000 5101  0000 8501  7800 8501  8A00 5101
    // FC 0b0101_0100_0000_1011  00   00 00 00 FD 7F 00 00 00 EE 8F            04   0C 0D 02 01             CB   4F    99   50    99   97    CB   97
    // FC 0b0101_0100_0000_1100  00   00 59 80 FF F6 37 02 B6 FF 84 FF 0C 01   04   3A00 2A01 2B01 3B00   7E00 1C01 0000 1C01 0000 5001 7E00 5001
    // FC 0b0101_0100_0000_1101  00   00 1C EE 45 81 00 00 AB FF 38 00 71 FF   04   0301 0201 0401 0501   7D 78 88 78 88 54 7D 54
    // FC 0b0101_1001_0000_0000  A0   00 00 00 66 82 9B 18 00 00 F0 FF 21 FF   04   05 09 0D 0E
    // FC 0b0101_1001_0000_0010  A0   00 00 00 33 80 F2 06 00 F8 87            04   26 2D 38 33
    // FC 0b0101_1001_0000_0100  90   00 00 00 FD 7F 00 00 21 FF FF FF 64 03   04   0501 0601 0701 0801
    // FC 0b0101_1001_0000_0110  9E   00 00 00 03 80 00 00 1C 7F D6            04   0901 0A01 0B01 0C01
    // FC 0b0101_1001_0000_1000  5D   00 00 00 00 00 FD 7F 00 00 E4 FF AC 00   04   03 06 09 04
    // FC 0b0101_1001_0000_1010  59   00 69 02 F7 7F B7 00 21 E4 56            05   09 0A 0B 0C 0D        1E FC 59 0A 59 00 88 FD F7 7F
    // FC 0b0101_1100_0000_0000  00   00 00 00 FD 7F 00 00 41 03 00 00 80 FD   04   5A 5B 57 56           6300 0A01  0000 0A01  0000 7701  6300 7701
    // FC 0b0101_1100_0000_0001  00   00 00 00 9E 7D 7B E7 00 00 F5 FF 30 FF   04   00 01 02 03             01   17    2B   19    2B   02    01   03
    // FC 0b0101_1100_0000_0010  00   00 00 00 50 6B C1 45 00 F4 EC            04   39 38 3B 3A           3D00 1101  0000 1101  0000 2C01  3D00 2C01
    // FC 0b0101_1100_0000_0011  00   00 FD 7F 00 00 00 00 0B F7 CE            04   17 00 0D 1F             93   64    93   93    FB   93    FB   64
    // FC 0b0101_1100_0000_0100  00   00 00 00 00 00 FD 7F 14 FD 07 00 91 03   04   0201 0301 0401 0501   FE00 EC00 EE00 EC00 EE00 0C01 FE00 0C01
    // FC 0b0101_1100_0000_0101  00   00 00 00 00 00 FD 7F 72 FF 0B 00 1D 00   04   1601 1501 1801 1701   F0 12 E0 12 E0 32 F0 32
    // FC 0b0101_1100_0000_0110  00   00 00 00 00 00 03 80 1F 78 DC            04   1601 1501 1401 1301   F600 2402 F600 6202 DB00 6202 DB00 2402
    // FC 0b0101_1100_0000_0111  00   00 00 00 FD 7F 00 00 1F 6B CE            04   1901 1C01 1B01 1A01   01 2B 01 39 26 39 26 2B
    // FC 0b0101_1100_0000_1000  00   00 00 00 00 00 FD 7F 00 00 B8 FF BD 00   04   00 01 02 03           9600 2A01 F500 2A01 F500 B000 9600 B000
    // FC 0b0101_1100_0000_1001  00   00 00 00 00 00 03 80 00 00 B8 FF 46 FF   04   04 05 06 07             96   AF    F5   AF    F5   35    96   35
    // FC 0b0101_1100_0000_1010  00   00 03 80 00 00 00 00 AB E4 AA            04   07 06 0A 0B           0000 FA02 8900 FA02 8900 8002 0000 8002
    // FC 0b0101_1100_0000_1011  00   00 00 00 FD 7F 00 00 00 2E A6            04   0A 08 01 00           00 02 00 69 6E 69 6E 02
    // FC 0b0101_1100_0000_1100  00   00 A5 55 00 00 1C 5F A0 FF 6F FF 6D 00   04   0201 0301 0401 0501   C100 6801 F600 A401 ED00 A801 BE00 7301
    // FC 0b0101_1100_0000_1101  00   00 5B AA 00 00 1C 5F 60 00 6F FF 6D 00   04   FE00 FF00 0001 0101   AD 96 AA A1 D9 D6 E2 D2
    // FC 0b0101_1100_0000_1111  00   00 00 00 00 00 03 80 02 F3 2D            04   0001 FF00 FE00 FD00   95 31 95 38 60 38 60 31
    // FC 0b0110_0001_0000_0000  96   00 DC 01 DD FD F5 7F F8 FF 0E 00 77 FF   04   18 19 1A 1B
    // FC 0b0110_0001_0000_0010  44   00 75 54 8A 5F 09 F5 06 FD F5            03   09 0A 08
    // FC 0b0110_0001_0000_0110  6E   00 00 00 FD 7F 00 00 F7 FD 01            04   0501 0401 0301 0201
    // FC 0b0110_0011_0000_0000  6D   00 00 00 0D 80 C9 FC 50 FF F0 FF 18 00   04   0A 09 06 07
    // FC 0b0110_0011_0000_0010  78   00 00 00 2D 71 3C C4 FC 12 08            04   19 10 0F 14
    // FC 0b0110_0011_0000_0110  96   00 FD 7F 00 00 00 00 00 FF FD            04   2601 0800 0700 0600
    // FC 0b0110_0100_0000_0010  00   00 25 80 F0 04 D8 FC FE 01 E9            04   00 01 02 03           B100 0801 C100 1101 D500 0901 B700 F800
    // FC 0b0110_0100_0000_0011  00   00 00 00 03 80 00 00 0D 0A D9            04   0B 29 28 27           CB 34 9A 33 97 25 C9 1C
    // FC 0b0110_1100_0000_0001  00   00 00 00 00 00 FD 7F 02 00 04 00 5D FF   04   2D 2C 2B 2A           33 17 33 2C 00 2C 00 17
    // FC 0b0110_1100_0000_0010  00   00 03 80 00 00 00 00 02 09 C7            04   1D 1C 20 1E           D500 FA00 D500 0A01 5600 0B01 5600 FA00    # 12 00 03 00 38 32 00
    // FC 0b0110_1100_0000_0011  00   00 00 00 03 80 00 00 09 01 02            04   0b 0a 09 08           52 55 52 35 9a 36 9a 55
    // FC 0b1100_1101_0000_0010  6B   00 FD 7F CD FF D9 FF 00 0A A5            04   A8 A9 8E AA           DC00 7101 C200 7601 9B00 3401 BB00 2801
    // FC 0b1100_1101_0000_0011  6E   00 00 00 FD 7F 00 00 00 07 F8            04   00 01 02 03           EB A7 D9 A7 D9 E8 EB EA
    // FC 0b1100_1101_0000_0111  6C   00 00 00 03 80 00 00 00 F5 32            04   0201 FC00 F900 0301   AF D8 AF 89 A9 7F A8 E4
    // FC 0b1101_1101_0000_1000  9E   00 00 00 00 00 03 80 A1 00 5A FF 36 FF   04   12 13 14 15           0000 0101 3B00 0101 3B00 5C01 0000 5C01
    // FC 0b1101_1101_0000_1001  98   00 D6 75 BC 0E BD 2F 0E 00 59 00 0A FF   04   A1 A0 A2 A3           10 4D 10 69 0C 69 08 4D
    // FC 0b1101_1101_0000_1010  9B   00 03 80 00 00 00 00 F8 10 FB            05   29 1B 17 16 2A        8600 1501 8600 1E01 8600 2701 AA00 2701 AD00 1501
    // FC 0b1101_1101_0000_1011  96   00 74 00 FD 7F FE FF 11 EE D1            05   0D 0E 0F 07 06        00 62  07 62  07 35  00 1C  00 49
    // FC 0b1101_1101_0000_1100  9F   00 03 80 00 00 00 00 B9 FF 5A FF 56 00   04   F900 F800 0001 0101   E200 0C01 E200 6701 BE00 6701 BE00 0C01
    // FC 0b1101_1101_0000_1101  9F   00 03 80 00 00 00 00 28 00 5A FF 56 00   04   FB00 FA00 0201 0301   24 4E 24 A9 00 A9 00 4E
    // FC 0b1110_1101_0000_0010  5A   00 7F 00 F1 7F 93 FC 03 01 F8            04   03 04 05 00           EC00 DB00 DC00 D900 DC00 FD00 ED00 0001
    // FC 0b1110_1101_0000_0011  44   00 00 00 0F 80 97 FC 10 FB 18            03   03 02 04              CB 08 CB 70 B9 08
    // FC 0b1110_1110_0000_0010  94   00 32 A9 0A 5E A3 01 FB 06 F1            03   0B 69 68              4700 6D01 D700 7101 D700 6401
    // FC 0b1110_1110_0000_0011  68   00 80 A5 80 A5 00 00 25 FF 01            04   7D 02 06 7E           5E 0A 61 0A 61 24 5E 24
    // FC 0b1110_1110_0000_0110  98   00 03 E1 78 7B BD F2 FC 09 FB            03   0300 B800 1201        AA00 C101 C100 8D01 A900 4501
    // FC 0b1110_1110_0000_0111  98   00 47 2B 1C 88 69 F4 0D F8 C5            03   FD00 4200 0301        7A B3 7A A8 37 AC 1E
     */
    pub fn from_bytes_after(offset: usize, data: &[u8]) -> Result<Self> {
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
        let indices: Vec<u16> = if flags.contains(FacetFlags::USE_SHORT_INDICES) {
            let index_u16: &[u16] = unsafe { mem::transmute(&data[off..]) };
            off += index_count * 2;
            index_u16[0..index_count].to_vec()
        } else {
            let index_u8 = &data[off..off + index_count];
            off += index_count;
            index_u8.iter().map(|&v| u16::from(v)).collect()
        };
        let indices_size = off - indices_offset;

        // Tex Coords
        let tc_offset = off;
        let mut tex_coords = Vec::with_capacity(index_count);
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

            flags_pointer: data[flags_offset..].as_ptr(),
            flags,

            color_pointer: data[color_offset..].as_ptr(),
            color,

            material_pointer: data[material_offset..].as_ptr(),
            material_size,
            raw_material,

            _indices_count_pointer: data[index_count_offset..].as_ptr(),
            indices_pointer: data[indices_offset..].as_ptr(),
            indices_size,
            indices,

            tc_pointer: data[tc_offset..].as_ptr(),
            tc_size,
            tex_coords,
        })
    }

    pub fn size(&self) -> usize {
        self.length
    }

    pub fn magic(&self) -> &'static str {
        "Facet(FC)"
    }

    pub fn at_offset(&self) -> usize {
        self.offset
    }

    pub fn show(&self) -> String {
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
            ansi().cyan().bold(),
            ansi(),

            // Flags
            ansi().cyan(),
            p2s(self.flags_pointer, 0, 2),
            ansi().white(), // (

            ansi().red(),
            flags[0],
            ansi().cyan(),
            flags[1],
            flags[2],
            flags[3],

            ansi().red(),
            flags[4],
            ansi().cyan(),
            flags[5],
            ansi().red(),
            flags[6],
            flags[7],

            flags[8],
            ansi().cyan(),
            flags[9],
            flags[10],
            ansi().magenta(),
            flags[11],
            ansi().white(), // )

            // Color
            ansi().cyan(),
            p2s(self.color_pointer, 0, 1).trim(),
            ansi(),

            // Material
            ansi().red().dimmed(),
            p2s(self.material_pointer, 0, self.material_size),
            ansi(),

            // Index count
            ansi().cyan().bold(),
            self.indices.len(),
            ansi(),

            // Indices
            ansi().cyan().dimmed(),
            p2s(self.indices_pointer, 0, self.indices_size).trim(),
            ansi().white(),
            ansi().cyan().bold(),
            ind,
            ansi(),

            // Texture Coordinates
            ansi().cyan().dimmed(),
            p2s(self.tc_pointer, 0, self.tc_size),
            ansi(),

            ansi().cyan().bold(),
            tcs,
            ansi(),
        )
    }
}

#[derive(Debug)]
pub struct VertexNormal {
    pub offset: usize,
    pub data: *const u8,
    pub index: usize,
    pub color: u8,
    pub norm: [i8; 3],
}

#[packed_struct]
pub struct VertexNormalOverlay {
    magic: u8,
    index: u16,
    color: u8,
    norm: [i8; 3],
}

impl VertexNormal {
    pub const MAGIC: u8 = 0xF6;
    pub const SIZE: usize = 7;

    pub fn from_bytes_after(offset: usize, data: &[u8]) -> Result<Self> {
        let overlay = VertexNormalOverlay::overlay(&data[..Self::SIZE])?;
        assert_eq!(overlay.magic(), Self::MAGIC);
        Ok(Self {
            offset,
            data: data.as_ptr(),
            index: overlay.index() as usize,
            norm: overlay.norm(),
            color: overlay.color(),
        })
    }

    pub fn size(&self) -> usize {
        Self::SIZE
    }

    pub fn magic(&self) -> &'static str {
        "F6"
    }

    pub fn at_offset(&self) -> usize {
        self.offset
    }

    pub fn show(&self) -> String {
        format!(
            "@{:04X} {}VxNrm{}: {}{}{}   | {}{}{}",
            self.offset,
            ansi().blue().bold(),
            ansi(),
            ansi().blue().bold(),
            p2s(self.data, 0, 1).trim(),
            ansi(),
            ansi().blue(),
            p2s(self.data, 1, Self::SIZE),
            ansi(),
        )
    }
}
