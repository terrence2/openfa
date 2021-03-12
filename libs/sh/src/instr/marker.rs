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
use anyhow::Result;
use reverse::p2s;
use std::mem;

#[derive(Debug)]
pub struct PtrToObjEnd {
    pub offset: usize,
    data: *const u8,
    pub delta_to_end: usize,
}

impl PtrToObjEnd {
    pub const MAGIC: u8 = 0xF2;
    pub const SIZE: usize = 4;

    pub fn from_bytes_after(offset: usize, data: &[u8]) -> Result<Self> {
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

    pub fn size(&self) -> usize {
        Self::SIZE
    }

    pub fn magic(&self) -> &'static str {
        "F2"
    }

    pub fn at_offset(&self) -> usize {
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
            ansi().blue().bright().bold(),
            ansi(),
            ansi().blue().bright().bold(),
            p2s(self.data, 0, 2).trim(),
            ansi(),
            ansi().blue().bright(),
            p2s(self.data, 2, Self::SIZE),
            ansi(),
            self.delta_to_end,
            self.end_byte_offset()
        )
    }
}
