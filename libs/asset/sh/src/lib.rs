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

mod instr;

pub use crate::instr::{
    read_name, EndOfObject, EndOfShape, Facet, FacetFlags, Jump, JumpToDamage, JumpToDetail,
    JumpToFrame, JumpToLOD, Pad1E, PtrToObjEnd, SourceRef, TextureIndex, TextureRef, Unmask,
    Unmask4, VertexBuf, VertexNormal, X86Code, X86Message, X86Trampoline, XformUnmask,
    XformUnmask4,
};
use ansi::{ansi, Color};
use anyhow::{anyhow, bail, ensure, Result};
use byteorder::{ByteOrder, LittleEndian};
use lazy_static::lazy_static;
use log::trace;
use reverse::{bs2s, bs_2_i16, p2s};
use std::{
    cmp,
    collections::{HashMap, HashSet},
    fmt, mem, str,
};

// Sandwiched instructions
// Unmask
//   12  -- 16 bit
//   6E  -- 32 bit
// Unmask and Xform
//   C4  -- 16 bit
//   C6  -- 32 bit
// Jump
//   48  --
// Pile of code
//   F0
//   X86Message
// Unknown
//   E4  -- Only in wave1/2
//   Data

pub const SHAPE_LOAD_BASE: u32 = 0xAA00_0000;

lazy_static! {
    // Virtual instructions that have a one-byte header instead of
    static ref ONE_BYTE_MAGIC: HashSet<u8> =
        [0x1E, 0x66, 0xFC, 0xFF].iter().cloned().collect();
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

    fn from_bytes(offset: usize, code: &[u8]) -> Result<Self> {
        let data = &code[offset..];
        assert_eq!(data[0], Self::MAGIC);
        ensure!(data[1] == 0, "not a word code instruction");
        let count = LittleEndian::read_u16(&data[14..16]) as usize;
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
            ansi().red().bold(),
            stringify!(Unk06),
            ansi(),
            ansi().red().bold(),
            p2s(self.data, 0, 2).trim(),
            ansi(),
            ansi().red(),
            p2s(self.data, 2, 14).trim(),
            ansi(),
            ansi().cyan(),
            self.count,
            ansi(),
            ansi().red(),
            p2s(self.data, 16, self.length),
            ansi()
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

    fn from_bytes(offset: usize, code: &[u8]) -> Result<Self> {
        let data = &code[offset..];
        assert_eq!(data[0], Self::MAGIC);
        ensure!(data[1] == 0, "not a word code instruction");
        let count = LittleEndian::read_u16(&data[10..12]) as usize;
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
            ansi().red().bold(),
            stringify!(Unk0C),
            ansi(),
            ansi().red().bold(),
            p2s(self.data, 0, 2).trim(),
            ansi(),
            ansi().red(),
            p2s(self.data, 2, 10).trim(),
            ansi(),
            ansi().cyan(),
            self.count,
            ansi(),
            ansi().red(),
            p2s(self.data, 12, self.length),
            ansi()
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

    fn from_bytes_after(offset: usize, data: &[u8]) -> Result<Self> {
        assert_eq!(data[0], Self::MAGIC);
        ensure!(data[1] == 0, "not a word code instruction");
        let count = LittleEndian::read_u16(&data[10..12]) as usize;
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
            ansi().red().bold(),
            stringify!(Unk0E),
            ansi(),
            ansi().red().bold(),
            p2s(self.data, 0, 2).trim(),
            ansi(),
            ansi().red(),
            p2s(self.data, 2, 10).trim(),
            ansi(),
            ansi().cyan(),
            self.count,
            ansi(),
            ansi().red(),
            p2s(self.data, 12, self.length),
            ansi()
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

    fn from_bytes_after(offset: usize, data: &[u8]) -> Result<Self> {
        assert_eq!(data[0], Self::MAGIC);
        ensure!(data[1] == 0, "not a word code instruction");
        let count = LittleEndian::read_u16(&data[10..12]) as usize;
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
            ansi().red().bold(),
            stringify!(Unk10),
            ansi(),
            ansi().red().bold(),
            p2s(self.data, 0, 2).trim(),
            ansi(),
            ansi().red(),
            p2s(self.data, 2, 10).trim(),
            ansi(),
            ansi().cyan(),
            self.count,
            ansi(),
            ansi().red(),
            p2s(self.data, 12, self.length),
            ansi()
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

    fn from_bytes_after(offset: usize, data: &[u8]) -> Result<Self> {
        assert_eq!(data[0], Self::MAGIC);
        ensure!(data[1] == 0, "not a word code instruction");
        let flag = data[10];
        let length = match flag {
            0x38 => 13, // Normal
            0x48 => 14, // F18 -- one of our errata?
            0x50 => 16, // F8
            _ => bail!("unexpected flag byte in 6C instruction: {:02X}", flag),
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
            ansi().red().bold(),
            stringify!(Unk6C),
            ansi(),
            ansi().red().bold(),
            p2s(self.data, 0, 2).trim(),
            ansi(),
            ansi().red(),
            p2s(self.data, 2, 10).trim(),
            ansi(),
            ansi().cyan(),
            self.flag,
            ansi(),
            ansi().red(),
            p2s(self.data, 11, self.length),
            ansi()
        )
    }
}

#[allow(clippy::upper_case_acronyms)]
pub struct UnkCE {
    pub offset: usize,
    pub data: [u8; 40 - 2],
}

impl UnkCE {
    pub const MAGIC: u8 = 0xCE;
    pub const SIZE: usize = 40;

    #[allow(clippy::unnecessary_wraps)]
    fn from_bytes(offset: usize, code: &[u8]) -> Result<Self> {
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

#[allow(clippy::upper_case_acronyms)]
#[derive(Debug)]
pub struct UnkBC {
    pub offset: usize,
    pub unk_header: u8,
    data: *const u8,
}

impl UnkBC {
    pub const MAGIC: u8 = 0xBC;

    #[allow(clippy::unnecessary_wraps)]
    fn from_bytes(offset: usize, code: &[u8]) -> Result<Self> {
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
            ansi().red().bold(),
            ansi(),
            ansi().red().bold(),
            p2s(self.data, 0, 2).trim(),
            ansi(),
            self.unk_header,
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

    #[allow(clippy::unnecessary_wraps)]
    fn from_bytes(offset: usize, code: &[u8]) -> Result<Self> {
        let data = &code[offset..];
        assert_eq!(data[0], Self::MAGIC);
        let unk0 = LittleEndian::read_u16(&data[1..3]) as usize;
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
            ansi().red().bold(),
            ansi(),
            ansi().red().bold(),
            p2s(self.data, 0, 1).trim(),
            ansi(),
            ansi().red(),
            p2s(self.data, 1, Self::SIZE),
            ansi(),
            self.unk0,
            self.offset + Self::SIZE + self.unk0
        )
    }
}

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

    #[allow(clippy::unnecessary_wraps)]
    fn from_bytes_after(offset: usize, data: &[u8]) -> Result<Self> {
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
            ansi().red().bold(),
            ansi(),
            self.data.len(),
            ansi().red().bold(),
            //bs2s(&self.data),
            bs_2_i16(&self.data),
            ansi(),
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
                ansi().red(),
                ansi(),
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

macro_rules! opaque_instr {
    ($name:ident, $magic_str: expr, $magic:expr, $size:expr) => {
        #[allow(clippy::upper_case_acronyms)]
        pub struct $name {
            pub offset: usize,
            pub data: *const u8,
        }

        impl $name {
            pub const MAGIC: u8 = $magic;
            pub const SIZE: usize = $size;

            fn from_bytes_after(offset: usize, data: &[u8]) -> Result<Self> {
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
                if stringify!($name) == "Header" {
                    let mut s = format!(
                        "@{:04X} {}{}{}: {}{}{}| ",
                        self.offset,
                        ansi().fg(Color::Green).bold(),
                        stringify!($name),
                        ansi(),
                        ansi().fg(Color::Green).bold(),
                        p2s(self.data, 0, 2).trim(),
                        ansi(),
                    );
                    let b: &[u8] = &unsafe { std::slice::from_raw_parts(self.data, 14) }[2..];
                    let d: &[i16] = unsafe { mem::transmute(b) };
                    for i in 0..6 {
                        s += &format!(
                            "{}{:02X}{:02X}({}){} ",
                            ansi().fg(Color::Green),
                            b[i * 2],
                            b[i * 2 + 1],
                            d[i],
                            ansi(),
                        );
                    }
                    return s;
                }

                let clr = if stringify!($name) == "Header" {
                    Color::Green
                } else {
                    Color::Red
                };
                format!(
                    "@{:04X} {}{}{}: {}{}{}| {}{}{}",
                    self.offset,
                    ansi().fg(clr).bold(),
                    stringify!($name),
                    ansi(),
                    ansi().fg(clr).bold(),
                    p2s(self.data, 0, 2).trim(),
                    ansi(),
                    ansi().fg(clr),
                    p2s(self.data, 2, Self::SIZE),
                    ansi()
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
opaque_instr!(Unk76, "76", 0x76, 10); // ATF:BULLET

// 6E 00| A5 30 00 00
// 6E 00| 06 00 00 00 50 00 73 00 00 00
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
#[allow(non_camel_case_types, clippy::upper_case_acronyms)]
pub enum Instr {
    Header(Header),
    PtrToObjEnd(PtrToObjEnd),
    SourceRef(SourceRef),

    Jump(Jump),
    JumpToDamage(JumpToDamage),
    JumpToDetail(JumpToDetail),
    JumpToFrame(JumpToFrame),
    JumpToLOD(JumpToLOD),

    // geometry
    TextureRef(TextureRef),
    TextureIndex(TextureIndex),
    VertexBuf(VertexBuf),
    Facet(Facet), // 0x__FC
    VertexNormal(VertexNormal),

    Unmask(Unmask),
    Unmask4(Unmask4),
    XformUnmask(XformUnmask),
    XformUnmask4(XformUnmask4),

    // Fixed size, with wasted 0 byte.
    Unk06(Unk06),
    Unk08(Unk08),
    Unk0C(Unk0C),
    Unk0E(Unk0E),
    Unk10(Unk10),
    Unk2E(Unk2E),
    Unk3A(Unk3A),
    Unk44(Unk44),
    Unk46(Unk46),
    Unk4E(Unk4E),
    Unk66(Unk66),
    Unk68(Unk68),
    Unk6C(Unk6C),
    Unk50(Unk50),
    Unk72(Unk72),
    Unk74(Unk74),
    Unk76(Unk76),
    Unk78(Unk78),
    Unk7A(Unk7A),
    Unk96(Unk96),
    UnkB2(UnkB2),
    UnkB8(UnkB8),
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

    // Fixed size, without wasted 0 byte after header.
    Pad1E(Pad1E),
    Unk38(Unk38),

    // Variable size.
    UnkBC(UnkBC),
    TrailerUnknown(TrailerUnknown),

    // Raw i386 bitcode used as a scripting language.
    X86Code(X86Code),
    X86Trampoline(X86Trampoline),
    X86Message(X86Message),
    UnknownUnknown(UnknownUnknown),
    UnknownData(UnknownData),

    EndOfObject(EndOfObject),
    EndOfShape(EndOfShape),
}

macro_rules! impl_for_all_instr {
    ($self:ident, $f:ident) => {
        match $self {
            Instr::Header(ref i) => i.$f(),
            Instr::PtrToObjEnd(ref i) => i.$f(),
            Instr::SourceRef(ref i) => i.$f(),
            Instr::EndOfObject(ref i) => i.$f(),
            Instr::EndOfShape(ref i) => i.$f(),
            Instr::Jump(ref i) => i.$f(),
            Instr::JumpToDamage(ref i) => i.$f(),
            Instr::JumpToDetail(ref i) => i.$f(),
            Instr::JumpToFrame(ref i) => i.$f(),
            Instr::JumpToLOD(ref i) => i.$f(),
            Instr::Unmask(ref i) => i.$f(),
            Instr::Unmask4(ref i) => i.$f(),
            Instr::XformUnmask(ref i) => i.$f(),
            Instr::XformUnmask4(ref i) => i.$f(),
            Instr::TextureIndex(ref i) => i.$f(),
            Instr::TextureRef(ref i) => i.$f(),
            Instr::VertexBuf(ref i) => i.$f(),
            Instr::VertexNormal(ref i) => i.$f(),
            Instr::Facet(ref i) => i.$f(),
            Instr::X86Code(ref i) => i.$f(),
            Instr::X86Trampoline(ref i) => i.$f(),
            Instr::X86Message(ref i) => i.$f(),
            Instr::Unk06(ref i) => i.$f(),
            Instr::Unk08(ref i) => i.$f(),
            Instr::Unk0C(ref i) => i.$f(),
            Instr::Unk0E(ref i) => i.$f(),
            Instr::Unk10(ref i) => i.$f(),
            Instr::Pad1E(ref i) => i.$f(),
            Instr::Unk2E(ref i) => i.$f(),
            Instr::Unk3A(ref i) => i.$f(),
            Instr::Unk44(ref i) => i.$f(),
            Instr::Unk46(ref i) => i.$f(),
            Instr::Unk4E(ref i) => i.$f(),
            Instr::Unk66(ref i) => i.$f(),
            Instr::Unk68(ref i) => i.$f(),
            Instr::Unk6C(ref i) => i.$f(),
            Instr::Unk50(ref i) => i.$f(),
            Instr::Unk72(ref i) => i.$f(),
            Instr::Unk74(ref i) => i.$f(),
            Instr::Unk76(ref i) => i.$f(),
            Instr::Unk78(ref i) => i.$f(),
            Instr::Unk7A(ref i) => i.$f(),
            Instr::Unk96(ref i) => i.$f(),
            Instr::UnkB2(ref i) => i.$f(),
            Instr::UnkB8(ref i) => i.$f(),
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
            Instr::Unk38(ref i) => i.$f(),
            Instr::UnkBC(ref i) => i.$f(),
            Instr::UnknownUnknown(ref i) => i.$f(),
            Instr::UnknownData(ref i) => i.$f(),
            Instr::TrailerUnknown(ref i) => i.$f(),
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

    pub fn unwrap_unmask_target(&self) -> Result<usize> {
        Ok(match self {
            Instr::Unmask(ref unmask) => unmask.target_byte_offset(),
            Instr::Unmask4(ref unmask) => unmask.target_byte_offset(),
            Instr::XformUnmask(ref unmask) => unmask.target_byte_offset(),
            Instr::XformUnmask4(ref unmask) => unmask.target_byte_offset(),
            _ => bail!("not an unwrap instruction"),
        })
    }

    pub fn unwrap_x86(&self) -> Result<&X86Code> {
        Ok(match self {
            Instr::X86Code(ref x86) => x86,
            _ => bail!("not an x86 code instruction"),
        })
    }

    pub fn unwrap_facet(&self) -> Result<&Facet> {
        Ok(match self {
            Instr::Facet(ref facet) => facet,
            _ => bail!("not a facet instruction"),
        })
    }
}

macro_rules! consume_instr {
    ($name:ident, $pe:ident, $offset:ident, $end_offset:ident, $instrs:ident) => {{
        let instr = $name::from_bytes(*$offset, &$pe.code[..$end_offset])?;
        *$offset += instr.size();
        $instrs.push(Instr::$name(instr));
    }};
}

macro_rules! consume_instr2 {
    ($name:ident, $pe:ident, $offset:ident, $end_offset:ident, $instrs:ident) => {{
        let instr = $name::from_bytes_after(*$offset, &$pe.code[*$offset..$end_offset])?;
        *$offset += instr.size();
        $instrs.push(Instr::$name(instr));
    }};
}

pub struct RawShape {
    pub instrs: Vec<Instr>,
    pub trampolines: Vec<X86Trampoline>,
    offset_map: HashMap<usize, usize>,
    pub pe: peff::PortableExecutable,
}

impl RawShape {
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        let mut pe = peff::PortableExecutable::from_bytes(data)?;

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
        let offset_map: HashMap<usize, usize> = instrs
            .iter()
            .enumerate()
            .map(|(i, instr)| (instr.at_offset(), i))
            .collect();

        Ok(RawShape {
            instrs,
            trampolines,
            offset_map,
            pe,
        })
    }

    pub fn bytes_to_index(&self, absolute_byte_offset: usize) -> Result<usize> {
        // FIXME: we need to handle ERRATA here?
        Ok(*self.offset_map.get(&absolute_byte_offset).ok_or_else(|| {
            anyhow!(format!(
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

    fn find_trampolines(pe: &peff::PortableExecutable) -> Result<Vec<X86Trampoline>> {
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

    fn find_end_of_shape(
        pe: &peff::PortableExecutable,
        trampolines: &[X86Trampoline],
    ) -> Result<EndOfShape> {
        let end_offset = pe.code.len() - trampolines.len() * X86Trampoline::SIZE;
        let mut offset = end_offset - 1;
        while pe.code[offset] == 0 {
            offset -= 1;
        }
        fn is_end(p: &[u8]) -> bool {
            p[0] == 1 && p[1] == 2 && p[2] == 3 && p[3] == 2 && p[4] == 1
        }
        offset -= 4;
        ensure!(
            is_end(&pe.code[offset..]),
            "expected 12321 sequence right before trampolines"
        );
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

    fn read_sections(
        pe: &peff::PortableExecutable,
        trampolines: &[X86Trampoline],
        trailer: &[Instr],
    ) -> Result<Vec<Instr>> {
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

        Ok(instrs)
    }

    fn read_instr(
        offset: &mut usize,
        pe: &peff::PortableExecutable,
        trampolines: &[X86Trampoline],
        trailer: &[Instr],
        instrs: &mut Vec<Instr>,
    ) -> Result<()> {
        let end_offset = pe.code.len() - Self::end_size(trailer);
        match pe.code[*offset] {
            Header::MAGIC => consume_instr2!(Header, pe, offset, end_offset, instrs),
            Pad1E::MAGIC => consume_instr2!(Pad1E, pe, offset, end_offset, instrs),
            SourceRef::MAGIC => consume_instr2!(SourceRef, pe, offset, end_offset, instrs),
            PtrToObjEnd::MAGIC => consume_instr2!(PtrToObjEnd, pe, offset, end_offset, instrs),

            TextureRef::MAGIC => consume_instr2!(TextureRef, pe, offset, end_offset, instrs),
            TextureIndex::MAGIC => consume_instr2!(TextureIndex, pe, offset, end_offset, instrs),
            VertexBuf::MAGIC => consume_instr2!(VertexBuf, pe, offset, end_offset, instrs),
            Facet::MAGIC => consume_instr2!(Facet, pe, offset, end_offset, instrs),
            VertexNormal::MAGIC => consume_instr2!(VertexNormal, pe, offset, end_offset, instrs),

            Jump::MAGIC => consume_instr2!(Jump, pe, offset, end_offset, instrs),
            JumpToDamage::MAGIC => consume_instr2!(JumpToDamage, pe, offset, end_offset, instrs),
            JumpToDetail::MAGIC => consume_instr2!(JumpToDetail, pe, offset, end_offset, instrs),
            JumpToFrame::MAGIC => consume_instr2!(JumpToFrame, pe, offset, end_offset, instrs),
            JumpToLOD::MAGIC => consume_instr2!(JumpToLOD, pe, offset, end_offset, instrs),

            Unmask::MAGIC => consume_instr2!(Unmask, pe, offset, end_offset, instrs),
            Unmask4::MAGIC => consume_instr2!(Unmask4, pe, offset, end_offset, instrs),
            XformUnmask::MAGIC => consume_instr2!(XformUnmask, pe, offset, end_offset, instrs),
            XformUnmask4::MAGIC => consume_instr2!(XformUnmask4, pe, offset, end_offset, instrs),

            Unk08::MAGIC => consume_instr2!(Unk08, pe, offset, end_offset, instrs),
            Unk0E::MAGIC => consume_instr2!(Unk0E, pe, offset, end_offset, instrs),
            Unk10::MAGIC => consume_instr2!(Unk10, pe, offset, end_offset, instrs),
            Unk2E::MAGIC => consume_instr2!(Unk2E, pe, offset, end_offset, instrs),
            Unk3A::MAGIC => consume_instr2!(Unk3A, pe, offset, end_offset, instrs),
            Unk44::MAGIC => consume_instr2!(Unk44, pe, offset, end_offset, instrs),
            Unk46::MAGIC => consume_instr2!(Unk46, pe, offset, end_offset, instrs),
            Unk4E::MAGIC => consume_instr2!(Unk4E, pe, offset, end_offset, instrs),
            Unk66::MAGIC => consume_instr2!(Unk66, pe, offset, end_offset, instrs),
            Unk68::MAGIC => consume_instr2!(Unk68, pe, offset, end_offset, instrs),
            Unk6C::MAGIC => consume_instr2!(Unk6C, pe, offset, end_offset, instrs),
            Unk50::MAGIC => consume_instr2!(Unk50, pe, offset, end_offset, instrs),
            Unk72::MAGIC => consume_instr2!(Unk72, pe, offset, end_offset, instrs),
            Unk74::MAGIC => consume_instr2!(Unk74, pe, offset, end_offset, instrs),
            Unk76::MAGIC => consume_instr2!(Unk76, pe, offset, end_offset, instrs),
            Unk78::MAGIC => consume_instr2!(Unk78, pe, offset, end_offset, instrs),
            Unk7A::MAGIC => consume_instr2!(Unk7A, pe, offset, end_offset, instrs),
            Unk96::MAGIC => consume_instr2!(Unk96, pe, offset, end_offset, instrs),
            UnkB2::MAGIC => consume_instr2!(UnkB2, pe, offset, end_offset, instrs),
            UnkB8::MAGIC => consume_instr2!(UnkB8, pe, offset, end_offset, instrs),
            UnkCA::MAGIC => consume_instr2!(UnkCA, pe, offset, end_offset, instrs),
            UnkD0::MAGIC => consume_instr2!(UnkD0, pe, offset, end_offset, instrs),
            UnkD2::MAGIC => consume_instr2!(UnkD2, pe, offset, end_offset, instrs),
            UnkDA::MAGIC => consume_instr2!(UnkDA, pe, offset, end_offset, instrs),
            UnkDC::MAGIC => consume_instr2!(UnkDC, pe, offset, end_offset, instrs),
            UnkE4::MAGIC => consume_instr2!(UnkE4, pe, offset, end_offset, instrs),
            UnkE6::MAGIC => consume_instr2!(UnkE6, pe, offset, end_offset, instrs),
            UnkE8::MAGIC => consume_instr2!(UnkE8, pe, offset, end_offset, instrs),
            UnkEA::MAGIC => consume_instr2!(UnkEA, pe, offset, end_offset, instrs),
            UnkEE::MAGIC => consume_instr2!(UnkEE, pe, offset, end_offset, instrs),

            Unk06::MAGIC => consume_instr!(Unk06, pe, offset, end_offset, instrs),
            Unk0C::MAGIC => consume_instr!(Unk0C, pe, offset, end_offset, instrs),
            UnkBC::MAGIC => consume_instr!(UnkBC, pe, offset, end_offset, instrs),
            UnkCE::MAGIC => consume_instr!(UnkCE, pe, offset, end_offset, instrs),

            Unk38::MAGIC => consume_instr!(Unk38, pe, offset, end_offset, instrs),

            X86Code::MAGIC => {
                let name =
                    if let Some(&Instr::SourceRef(ref source)) = find_first_instr(0x42, instrs) {
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
                        if let Some(&Instr::PtrToObjEnd(ref end_ptr)) =
                            find_first_instr(0xF2, instrs)
                        {
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
                        if let Some(&Instr::PtrToObjEnd(ref end_ptr)) =
                            find_first_instr(0xF2, instrs)
                        {
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
                    let obj_end =
                        EndOfObject::from_bytes_after(*offset, &remaining[..EndOfObject::SIZE])?;
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
                    data: pe.code[*offset..end_offset].to_owned(),
                };
                *offset = pe.code.len();
                instrs.push(Instr::UnknownUnknown(instr));

                // Someday we'll be able to turn on this bail.
                // bail!("unknown instruction 0x{:02X} at 0x{:04X}: {}", _vop, *offset, bs2s(&pe.code[*offset..]));
            }
        }
        Ok(())
    }

    // Map an offset in bytes from the beginning of the virtual instruction stream
    // to an offset into the virtual instructions.
    pub fn map_absolute_offset_to_instr_offset(&self, abs_offset: usize) -> Result<usize> {
        for (instr_offset, instr) in self.instrs.iter().enumerate() {
            if instr.at_offset() == abs_offset {
                return Ok(instr_offset);
            }
        }
        bail!("no instruction at absolute offset: {:08X}", abs_offset)
    }

    pub fn map_interpreter_offset_to_instr_offset(&self, x86_offset: u32) -> Result<usize> {
        let mut b_offset = 0u32;
        for (offset, instr) in self.instrs.iter().enumerate() {
            if SHAPE_LOAD_BASE + b_offset == x86_offset {
                return Ok(offset);
            }
            b_offset += instr.size() as u32;
        }
        bail!("no instruction at x86_offset: {:08X}", x86_offset)
    }

    pub fn lookup_trampoline_by_offset(&self, abs_offset: u32) -> Result<&X86Trampoline> {
        for tramp in &self.trampolines {
            if tramp.at_offset() == abs_offset as usize {
                return Ok(tramp);
            }
        }
        bail!("no trampoline at absolute offset: {:08X}", abs_offset);
    }

    pub fn lookup_trampoline_by_name(&self, name: &str) -> Result<&X86Trampoline> {
        for tramp in &self.trampolines {
            if tramp.name == name {
                return Ok(tramp);
            }
        }
        bail!("no trampoline with name: {}", name);
    }

    pub fn has_damage_section(&self) -> bool {
        for instr in &self.instrs {
            if let Instr::JumpToDamage(_) = instr {
                return true;
            }
        }
        false
    }

    pub fn byte_length(&self) -> usize {
        self.pe.code.len()
    }

    pub fn length(&self) -> usize {
        self.instrs.len()
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
mod tests {
    use super::*;
    use lib::Libs;
    use simplelog::{Config, LevelFilter, TermLogger};

    fn offset_of_trailer(shape: &RawShape) -> Option<usize> {
        let mut offset = None;
        for (_i, instr) in shape.instrs.iter().enumerate() {
            if let Instr::TrailerUnknown(trailer) = instr {
                assert_eq!(offset, None, "multiple trailers");
                offset = Some(trailer.offset);
            }
        }
        offset
    }

    fn find_f2_target(shape: &RawShape) -> Option<usize> {
        for instr in shape.instrs.iter().rev() {
            if let Instr::PtrToObjEnd(f2) = instr {
                return Some(f2.end_byte_offset());
            }
        }
        None
    }

    #[allow(dead_code)]
    fn compute_instr_freqs(shape: &RawShape, freq: &mut HashMap<&'static str, usize>) {
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
    fn it_works() -> Result<()> {
        TermLogger::init(LevelFilter::Info, Config::default())?;

        #[allow(unused_variables, unused_mut)]
        let mut freq: HashMap<&'static str, usize> = HashMap::new();

        let libs = Libs::for_testing()?;
        for (game, _palette, catalog) in libs.all() {
            for fid in catalog.find_with_extension("SH")? {
                let meta = catalog.stat(fid)?;
                println!("At: {}:{:13} @ {}", game.test_dir, meta.name(), meta.path());

                let data = catalog.read(fid)?;
                let shape = RawShape::from_bytes(data.as_ref())?;

                // Ensure that f2 points to the trailer if it exists.
                // And conversely that we found the trailer in the right place.
                if let Some(offset) = offset_of_trailer(&shape) {
                    if let Some(f2_target) = find_f2_target(&shape) {
                        assert_eq!(offset, f2_target);
                    }
                }

                // Ensure that all Unmask(12) and Jump(48) point to a valid instruction.
                for instr in &shape.instrs {
                    match instr {
                        Instr::Unmask(unk) => {
                            let index = shape.bytes_to_index(unk.target_byte_offset())?;
                            let _target_instr = &shape.instrs[index];
                        }
                        Instr::Jump(j) => {
                            let index = shape.bytes_to_index(j.target_byte_offset())?;
                            let _target_instr = &shape.instrs[index];
                        }
                        Instr::JumpToFrame(jf) => {
                            let mut all_final_targets = HashSet::new();
                            ensure!(
                                [2, 3, 4, 6].contains(&jf.num_frames()),
                                "only 2, 3, 4, & 6 frame count supported"
                            );
                            for frame_num in 0..jf.num_frames() {
                                // All frames must point to a single facet.
                                let index = shape.bytes_to_index(jf.target_for_frame(frame_num))?;
                                let target_instr = &shape.instrs[index];
                                assert_eq!(target_instr.magic(), "Facet(FC)");
                                // All frames must jump to the end or fall off the end.
                                if let Instr::Jump(ref j) = &shape.instrs[index + 1] {
                                    all_final_targets.insert(j.target_byte_offset());
                                } else {
                                    assert_eq!(frame_num, jf.num_frames() - 1);
                                    all_final_targets.insert(shape.instrs[index + 1].at_offset());
                                }
                                // All frames must end at the same instruction.
                                assert_eq!(all_final_targets.len(), 1);
                            }
                        }
                        _ => {}
                    }
                }

                // Ensure that all offsets and sizes line up.
                let mut expect_offset = 0;
                for instr in &shape.instrs {
                    assert_eq!(expect_offset, instr.at_offset());
                    expect_offset += instr.size();
                }
            }
        }

        //show_instr_freqs(&freq);

        Ok(())
    }
}
