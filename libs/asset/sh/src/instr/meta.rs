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
use anyhow::Result;
use reverse::{bs2s, p2s, p_2_i16};

#[derive(Debug)]
pub struct SourceRef {
    pub offset: usize,
    pub source: String,
}

impl SourceRef {
    pub const MAGIC: u8 = 0x42;

    pub fn from_bytes_after(offset: usize, data: &[u8]) -> Result<Self> {
        assert_eq!(data[0], Self::MAGIC);
        assert_eq!(data[1], 0x00);
        let source = read_name(&data[2..])?;
        Ok(SourceRef { offset, source })
    }

    pub fn size(&self) -> usize {
        2 + self.source.len() + 1
    }

    pub fn magic(&self) -> &'static str {
        "42"
    }

    pub fn at_offset(&self) -> usize {
        self.offset
    }

    pub fn show(&self) -> String {
        format!(
            "@{:04X} {}SrcRf{}: {}{}{}",
            self.offset,
            ansi().yellow().bold(),
            ansi(),
            ansi().yellow(),
            self.source,
            ansi(),
        )
    }
}

#[derive(Debug)]
pub struct EndOfShape {
    pub offset: usize,
    pub data: Vec<u8>,
}

// 1 2 3 2 1 0*
impl EndOfShape {
    pub fn from_bytes_after(offset: usize, data: &[u8]) -> Result<Self> {
        Ok(Self {
            offset,
            data: data.to_owned(),
        })
    }

    pub fn size(&self) -> usize {
        self.data.len()
    }

    pub fn magic(&self) -> &'static str {
        "EndOfShape"
    }

    pub fn at_offset(&self) -> usize {
        self.offset
    }

    pub fn show(&self) -> String {
        format!(
            "@{:04X} {}EndSh{}: {}{}{}| {}{}{}",
            self.offset,
            ansi().green().bold(),
            ansi(),
            ansi().green().bold(),
            bs2s(&self.data[0..2]).trim(),
            ansi(),
            ansi().green(),
            bs2s(&self.data[2..]),
            ansi()
        )
    }
}

// 00 XX (and maybe more? :shrug:)
#[derive(Debug)]
pub struct EndOfObject {
    pub offset: usize,
    pub data: *const u8,
}

impl EndOfObject {
    pub const SIZE: usize = 18;

    pub fn from_bytes_after(offset: usize, data: &[u8]) -> Result<Self> {
        Ok(Self {
            offset,
            data: data.as_ptr(),
        })
    }

    pub fn size(&self) -> usize {
        Self::SIZE
    }

    pub fn magic(&self) -> &'static str {
        "EndOfObject"
    }

    pub fn at_offset(&self) -> usize {
        self.offset
    }

    pub fn show(&self) -> String {
        format!(
            "@{:04X} {}EdObj{}: {}{}{}| {}{}{}",
            self.offset,
            ansi().green().bold(),
            ansi(),
            ansi().green().bold(),
            p2s(self.data, 0, 2).trim(),
            ansi(),
            ansi().green(),
            p_2_i16(self.data, 2, Self::SIZE),
            ansi()
        )
    }
}

#[derive(Debug)]
pub struct Pad1E {
    offset: usize,
    length: usize,
}

impl Pad1E {
    pub const MAGIC: u8 = 0x1E;

    pub fn from_bytes_after(offset: usize, data: &[u8]) -> Result<Self> {
        let mut cnt = 0;
        while cnt < data.len() && data[cnt] == 0x1E {
            cnt += 1;
        }
        assert!(cnt > 0);
        Ok(Pad1E {
            offset,
            length: cnt,
        })
    }

    pub fn size(&self) -> usize {
        self.length
    }

    pub fn magic(&self) -> &'static str {
        "1E"
    }

    pub fn at_offset(&self) -> usize {
        self.offset
    }

    pub fn show(&self) -> String {
        if self.length == 1 {
            format!(
                "@{:04X} {}Pad1E: 1E{}   |",
                self.offset,
                ansi().dimmed(),
                ansi()
            )
        } else if self.length == 2 {
            format!(
                "@{:04X} {}Pad1E: 1E 1E{}|",
                self.offset,
                ansi().dimmed(),
                ansi()
            )
        } else {
            let mut data = String::new();
            for _ in 0..self.length - 2 {
                data += "1E ";
            }
            let data = data.trim_end();
            format!(
                "@{:04X} {}Pad1E: 1E 1E{}| {}{}{}",
                self.offset,
                ansi().dimmed(),
                ansi(),
                ansi().dimmed(),
                data,
                ansi()
            )
        }
    }
}
