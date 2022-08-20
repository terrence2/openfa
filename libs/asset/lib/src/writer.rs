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
use crate::{LibEntry, LibHeader};
use anyhow::{ensure, Result};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    mem,
    path::Path,
};
use zerocopy::AsBytes;

pub struct LibWriter {
    fp: File,
    total_count: usize,
    offset: usize,
}

impl LibWriter {
    pub fn new(path: &Path, file_count: u16) -> Result<Self> {
        let mut fp = File::create(path)?;
        fp.write_all(LibHeader::new(file_count).as_bytes())?;
        Ok(Self {
            fp,
            total_count: file_count as usize,
            offset: 0,
        })
    }

    pub fn add_file(&mut self, input: &Path) -> Result<()> {
        ensure!(self.offset < self.total_count);

        let mut f = File::open(input)?;
        let mut content = Vec::new();
        f.read_to_end(&mut content)?;

        let filename = input.file_name();
        ensure!(filename.is_some(), "directory passed to add_file");
        let mut filename = filename.unwrap().to_owned();
        ensure!(filename.is_ascii());
        filename.make_ascii_uppercase();
        let filename = filename.into_string().unwrap();

        self.fp.seek(SeekFrom::End(0))?;
        let offset = self.fp.stream_position()?;
        ensure!(offset < std::u32::MAX as u64);
        let entry = LibEntry::new(filename.as_bytes(), offset as u32)?;
        self.fp.write_all(&content)?;

        self.fp.seek(SeekFrom::Start(
            (mem::size_of::<LibHeader>() + mem::size_of::<LibEntry>() * self.offset) as u64,
        ))?;
        self.fp.write_all(entry.as_bytes())?;

        self.offset += 1;
        Ok(())
    }

    pub fn finish(mut self) -> Result<()> {
        self.fp.flush()?;
        Ok(())
    }
}
