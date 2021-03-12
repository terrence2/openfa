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
use anyhow::{ensure, Result};
use reverse::p2s;
use std::mem;

#[derive(Debug)]
pub struct Jump {
    offset: usize,
    data: *const u8,
    offset_to_target: isize,
}

impl Jump {
    pub const MAGIC: u8 = 0x48;
    pub const SIZE: usize = 4;

    pub fn from_bytes_after(offset: usize, data: &[u8]) -> Result<Self> {
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

    pub fn size(&self) -> usize {
        Self::SIZE
    }

    pub fn magic(&self) -> &'static str {
        "48"
    }

    pub fn at_offset(&self) -> usize {
        self.offset
    }

    pub fn target_byte_offset(&self) -> usize {
        (self.offset + Self::SIZE).wrapping_add(self.offset_to_target as usize)
    }

    pub fn show(&self) -> String {
        format!(
            "@{:04X} {}Jump!{}: {}{}{}| {}{}{} (tgt:{:04X})",
            self.offset,
            ansi().blue().bold(),
            ansi(),
            ansi().blue().bold(),
            p2s(self.data, 0, 2).trim(),
            ansi(),
            ansi().blue(),
            p2s(self.data, 2, Self::SIZE),
            ansi(),
            self.target_byte_offset()
        )
    }
}

#[derive(Debug)]
pub struct JumpToDamage {
    offset: usize,
    data: *const u8,
    delta_to_damage: isize,
}

impl JumpToDamage {
    pub const MAGIC: u8 = 0xAC;
    pub const SIZE: usize = 4;

    pub fn from_bytes_after(offset: usize, data: &[u8]) -> Result<Self> {
        assert_eq!(data[0], Self::MAGIC);
        assert_eq!(data[1], 0x00);
        let word_ref: &[i16] = unsafe { mem::transmute(&data[2..]) };
        let delta_to_damage = word_ref[0] as isize;
        Ok(Self {
            offset,
            data: data.as_ptr(),
            delta_to_damage,
        })
    }

    pub fn size(&self) -> usize {
        Self::SIZE
    }

    pub fn magic(&self) -> &'static str {
        "AC"
    }

    pub fn at_offset(&self) -> usize {
        self.offset
    }

    pub fn damage_byte_offset(&self) -> usize {
        (self.offset + Self::SIZE).wrapping_add(self.delta_to_damage as usize)
    }

    pub fn show(&self) -> String {
        format!(
            "@{:04X} {}ToDam{}: {}{}{}| {}{}{} (delta:{:04X}, target:{:04X})",
            self.offset,
            ansi().blue().bright().bold(),
            ansi(),
            ansi().blue().bright().bold(),
            p2s(self.data, 0, 2).trim(),
            ansi(),
            ansi().blue().bright(),
            p2s(self.data, 2, Self::SIZE),
            ansi(),
            self.delta_to_damage,
            self.damage_byte_offset()
        )
    }
}

#[derive(Debug)]
pub struct JumpToDetail {
    offset: usize,
    data: *const u8,

    offset_to_target: isize,

    // This is in the range 1-3, so is probably the game detail level control, rather
    // than a Level-of-Detail control.
    pub level: u16,
}

impl JumpToDetail {
    pub const MAGIC: u8 = 0xA6;
    const SIZE: usize = 6;

    pub fn from_bytes_after(offset: usize, data: &[u8]) -> Result<Self> {
        assert_eq!(data[0], Self::MAGIC);
        assert_eq!(data[1], 0x00);
        let word_ref: &[i16] = unsafe { mem::transmute(&data[2..]) };
        let level = word_ref[1] as u16;
        assert!((1..=3).contains(&level));
        let offset_to_target = word_ref[0] as isize;
        Ok(Self {
            offset,
            level,
            offset_to_target,
            data: data[0..Self::SIZE].as_ptr(),
        })
    }

    pub fn size(&self) -> usize {
        Self::SIZE
    }

    pub fn magic(&self) -> &'static str {
        "A6"
    }

    pub fn at_offset(&self) -> usize {
        self.offset
    }

    pub fn target_byte_offset(&self) -> usize {
        (self.offset + Self::SIZE).wrapping_add(self.offset_to_target as usize)
    }

    pub fn show(&self) -> String {
        format!(
            "@{:04X} {}ToDtl{}: {}{}{}| {}{}{} (level:{:04X}, target:{:04X})",
            self.offset,
            ansi().blue().bright().bold(),
            ansi(),
            ansi().blue().bright().bold(),
            p2s(self.data, 0, 2).trim(),
            ansi(),
            ansi().blue().bright(),
            p2s(self.data, 2, Self::SIZE),
            ansi(),
            self.level,
            self.target_byte_offset()
        )
    }
}

#[derive(Debug)]
pub struct JumpToFrame {
    pub offset: usize,
    length: usize,
    data: *const u8,

    count: usize,
    frame_offsets: Vec<u16>,
}

impl JumpToFrame {
    pub const MAGIC: u8 = 0x40;

    // 40 00   04 00   08 00, 25 00, 42 00, 5F 00
    pub fn from_bytes_after(offset: usize, data: &[u8]) -> Result<Self> {
        assert_eq!(data[0], Self::MAGIC);
        assert_eq!(data[1], 0x00);
        let words: &[u16] = unsafe { mem::transmute(&data[2..]) };
        let count = words[0] as usize;
        ensure!(
            count <= 6,
            "found jump-to-frame instruction with more than 6 frames of animation"
        );
        let length = 4 + count * 2;
        Ok(Self {
            offset,
            length,
            data: data.as_ptr(),

            count,
            frame_offsets: words[1..=count].to_owned(),
        })
    }

    pub fn size(&self) -> usize {
        self.length
    }

    pub fn magic(&self) -> &'static str {
        "40"
    }

    pub fn at_offset(&self) -> usize {
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

    pub fn show(&self) -> String {
        let targets = (0..self.count)
            .map(|i| format!("{:02X}", self.target_for_frame(i)))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "@{:04X} {}ToFrm{}: {}{}{}| {}{}{} (cnt:{}, data:({}))",
            self.offset,
            ansi().blue().bold(),
            ansi(),
            ansi().blue().bold(),
            p2s(self.data, 0, 2).trim(),
            ansi(),
            ansi().blue().dimmed(),
            p2s(self.data, 2, self.length),
            ansi(),
            self.count,
            targets
        )
    }
}

#[derive(Debug)]
pub struct JumpToLOD {
    pub offset: usize,
    data: *const u8,

    pub unk0: u16,
    pub unk1: u16,
    pub target_offset: isize,
}

impl JumpToLOD {
    pub const MAGIC: u8 = 0xC8;
    pub const SIZE: usize = 8;

    pub fn from_bytes_after(offset: usize, data: &[u8]) -> Result<Self> {
        assert_eq!(data[0], Self::MAGIC);
        assert_eq!(data[1], 0x00);
        let word_ref: &[u16] = unsafe { mem::transmute(&data[2..]) };
        let unk0 = word_ref[0];
        let unk1 = word_ref[1];
        let target_offset = word_ref[2] as i16 as isize;
        Ok(Self {
            offset,
            unk0,
            unk1,
            target_offset,
            data: data[0..Self::SIZE].as_ptr(),
        })
    }

    pub fn size(&self) -> usize {
        Self::SIZE
    }

    pub fn magic(&self) -> &'static str {
        "C8"
    }

    pub fn at_offset(&self) -> usize {
        self.offset
    }

    pub fn target_byte_offset(&self) -> usize {
        (self.offset + Self::SIZE).wrapping_add(self.target_offset as usize)
    }

    pub fn show(&self) -> String {
        format!(
            "@{:04X} {}ToLOD{}: {}{}{}| {}{}{} (unk0:{:04X}, unk1:{:04X} target:{:04X})",
            self.offset,
            ansi().blue().bright().bold(),
            ansi(),
            ansi().blue().bright().bold(),
            p2s(self.data, 0, 2).trim(),
            ansi(),
            ansi().blue().bright(),
            p2s(self.data, 2, Self::SIZE),
            ansi(),
            self.unk0,
            self.unk1,
            self.target_byte_offset()
        )
    }
}
