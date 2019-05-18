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
use ansi::ansi;
use failure::Fallible;
use reverse::p2s;
use std::mem;

// When points to a VertexBuf, it "unmasks" the facets that occur after that vertex buffer up
// to the next vertex buffer or Header. When it points elsewhere, :shrug:. It usually points
// to something that jumps, I think to do manual backface culling, maybe? Doesn't seem important.
#[derive(Debug)]
pub struct Unmask {
    pub offset: usize,
    data: *const u8,
    pub offset_to_next: usize,
}

impl Unmask {
    pub const MAGIC: u8 = 0x12;
    pub const SIZE: usize = 4;

    pub fn from_bytes_after(offset: usize, data: &[u8]) -> Fallible<Self> {
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

    pub fn size(&self) -> usize {
        Self::SIZE
    }

    pub fn magic(&self) -> &'static str {
        "Unmask"
    }

    pub fn at_offset(&self) -> usize {
        self.offset
    }

    pub fn target_byte_offset(&self) -> usize {
        self.offset + Self::SIZE + self.offset_to_next
    }

    pub fn show(&self) -> String {
        format!(
            "@{:04X} {}UMask{}: {}{}{}| {}{}{} (target:{:04X})",
            self.offset,
            ansi().red().bold(),
            ansi(),
            ansi().red().bold(),
            p2s(self.data, 0, 2).trim(),
            ansi(),
            ansi().red(),
            p2s(self.data, 2, Self::SIZE),
            ansi(),
            self.target_byte_offset()
        )
    }
}

// Seems identical to Unk12, but stores a 32bit offset instead of a 16bit offset.
#[derive(Debug)]
pub struct Unmask4 {
    pub offset: usize,
    data: *const u8,
    pub offset_to_next: usize,
}

impl Unmask4 {
    pub const MAGIC: u8 = 0x6E;
    pub const SIZE: usize = 6;

    pub fn from_bytes_after(offset: usize, data: &[u8]) -> Fallible<Self> {
        assert_eq!(data[0], Self::MAGIC);
        assert_eq!(data[1], 0x00);
        let dword_ref: &[u32] = unsafe { mem::transmute(&data[2..]) };
        let offset_to_next = dword_ref[0] as usize;
        Ok(Self {
            offset,
            data: data[0..Self::SIZE].as_ptr(),
            offset_to_next,
        })
    }

    pub fn size(&self) -> usize {
        Self::SIZE
    }

    pub fn magic(&self) -> &'static str {
        "Unmask"
    }

    pub fn at_offset(&self) -> usize {
        self.offset
    }

    pub fn target_byte_offset(&self) -> usize {
        self.offset + Self::SIZE + self.offset_to_next
    }

    pub fn show(&self) -> String {
        format!(
            "@{:04X} {}UMsk4{}: {}{}{}| {}{}{} (target:{:06X})",
            self.offset,
            ansi().red().bold(),
            ansi(),
            ansi().red().bold(),
            p2s(self.data, 0, 2).trim(),
            ansi(),
            ansi().red(),
            p2s(self.data, 2, Self::SIZE),
            ansi(),
            self.target_byte_offset()
        )
    }
}

// Like Unmask(12), but also includes a transform to be applied to the
// facets. Typically this instruction is the target of some x86 script
// that mutates the embedded transform each frame to, e.g. animate the
// landing gear lowering.
#[derive(Debug)]
pub struct XformUnmask {
    pub offset: usize,
    data: *const u8,

    pub t0: i16,
    pub t1: i16,
    pub t2: i16,
    pub a0: i16,
    pub a1: i16,
    pub a2: i16,
    pub xform_base: [u8; 12],
    pub offset_to_next: usize,
}

impl XformUnmask {
    pub const MAGIC: u8 = 0xC4;
    pub const SIZE: usize = 16;

    pub fn from_bytes_after(offset: usize, data: &[u8]) -> Fallible<Self> {
        assert_eq!(data[0], Self::MAGIC);
        assert_eq!(data[1], 0x00);
        let word_ref: &[i16] = unsafe { mem::transmute(&data[2..]) };
        let t0 = word_ref[0];
        let t1 = word_ref[1];
        let t2 = word_ref[2];
        let a0 = word_ref[3];
        let a1 = word_ref[4];
        let a2 = word_ref[5];
        let mut xform_base: [u8; 12] = Default::default();
        xform_base.copy_from_slice(&data[2..14]);
        let uword_ref: &[u16] = unsafe { mem::transmute(&data[14..]) };
        let offset_to_next = uword_ref[0] as usize;
        Ok(Self {
            offset,
            data: data[0..Self::SIZE].as_ptr(),
            t0,
            t1,
            t2,
            a0,
            a1,
            a2,
            xform_base,
            offset_to_next,
        })
    }

    pub fn size(&self) -> usize {
        Self::SIZE
    }

    pub fn magic(&self) -> &'static str {
        "XformUnmask"
    }

    pub fn at_offset(&self) -> usize {
        self.offset
    }

    pub fn target_byte_offset(&self) -> usize {
        self.offset + Self::SIZE + self.offset_to_next
    }

    pub fn show(&self) -> String {
        format!(
            "@{:04X} {}Xform{}: {}{}{}| {}{}{} t:({}{},{},{}{}) a:({}{},{},{}{}) {}{}{} (target:{:04X})",
            self.offset,
            ansi().red().bold(),
            ansi(),
            ansi().red().bold(),
            p2s(self.data, 0, 2).trim(),
            ansi(),
            ansi().blue(),
            p2s(self.data, 2, Self::SIZE).trim(),
            ansi(),
            ansi().magenta(),
            self.t0, self.t1, self.t2,
            ansi(),
            ansi().magenta(),
            self.a0, self.a1, self.a2,
            ansi(),
            ansi().cyan(),
            p2s(self.data, Self::SIZE - 2, Self::SIZE).trim(),
            ansi(),
            self.target_byte_offset()
        )
    }
}

// Like XformUnmask(C4), but the offset is a 32bit number instead of 16bit.
// In FA:F8.SH:
//At: 1241 => Unknown @ 59E0:     18b =>
//  @00|59E0: C6 00 00 00 FE FF 28 00 00 00 00 00 00 00 7D 0D
//  @10|59F0: 00 00
#[derive(Debug)]
pub struct XformUnmask4 {
    pub offset: usize,
    data: *const u8,

    pub t0: i16,
    pub t1: i16,
    pub t2: i16,
    pub a0: i16,
    pub a1: i16,
    pub a2: i16,
    pub xform_base: [u8; 12],
    pub offset_to_next: usize,
}

impl XformUnmask4 {
    pub const MAGIC: u8 = 0xC6;
    pub const SIZE: usize = 18;

    pub fn from_bytes_after(offset: usize, data: &[u8]) -> Fallible<Self> {
        assert_eq!(data[0], Self::MAGIC);
        assert_eq!(data[1], 0x00);
        let word_ref: &[i16] = unsafe { mem::transmute(&data[2..]) };
        let t0 = word_ref[0];
        let t1 = word_ref[1];
        let t2 = word_ref[2];
        let a0 = word_ref[3];
        let a1 = word_ref[4];
        let a2 = word_ref[5];
        let mut xform_base: [u8; 12] = Default::default();
        xform_base.copy_from_slice(&data[2..14]);
        let dword_ref: &[u32] = unsafe { mem::transmute(&data[14..]) };
        let offset_to_next = dword_ref[0] as usize;
        Ok(Self {
            offset,
            data: data[0..Self::SIZE].as_ptr(),
            t0,
            t1,
            t2,
            a0,
            a1,
            a2,
            xform_base,
            offset_to_next,
        })
    }

    pub fn size(&self) -> usize {
        Self::SIZE
    }

    pub fn magic(&self) -> &'static str {
        "XformUnmask"
    }

    pub fn at_offset(&self) -> usize {
        self.offset
    }

    pub fn target_byte_offset(&self) -> usize {
        self.offset + Self::SIZE + self.offset_to_next
    }

    pub fn show(&self) -> String {
        format!(
            "@{:04X} {}Xfrm4{}: {}{}{}| {}{}{} t:({}{},{},{}{}) a:({}{},{},{}{}) {}{}{} (target:{:04X})",
            self.offset,
            ansi().red().bold(),
            ansi(),
            ansi().red().bold(),
            p2s(self.data, 0, 2).trim(),
            ansi(),
            ansi().blue(),
            p2s(self.data, 2, Self::SIZE).trim(),
            ansi(),
            ansi().magenta(),
            self.t0, self.t1, self.t2,
            ansi(),
            ansi().magenta(),
            self.a0, self.a1, self.a2,
            ansi(),
            ansi().cyan(),
            p2s(self.data, Self::SIZE - 4, Self::SIZE).trim(),
            ansi(),
            self.target_byte_offset()
        )
    }
}
