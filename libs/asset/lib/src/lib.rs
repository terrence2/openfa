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

// Load LIB files; find files in them; hand out immutable pointers on request.
#![allow(clippy::transmute_ptr_to_ptr, clippy::new_ret_no_self)]

mod catalog_manager;
mod game_info;

pub use crate::{
    catalog_manager::{CatalogManager, CatalogOpts},
    game_info::{GameInfo, GAME_INFO},
};

use anyhow::{anyhow, ensure, Result};
use async_trait::async_trait;
use byteorder::{ByteOrder, LittleEndian};
use catalog::{DrawerFileId, DrawerFileMetadata, DrawerInterface};
use codepage_437::{BorrowFromCp437, FromCp437, CP437_CONTROL};
use lazy_static::lazy_static;
use log::trace;
use memmap::{Mmap, MmapOptions};
use packed_struct::packed_struct;
use regex::Regex;
use std::{
    borrow::Cow,
    collections::HashMap,
    fs, mem,
    ops::Range,
    path::{Path, PathBuf},
    str,
};

#[derive(Clone, Debug)]
pub enum CompressionType {
    None = 0,
    Lzss = 1,   // Compressed with LZSS
    PxPk = 3,   // No compression, but includes 4 byte inline header 'PXPK'
    PkWare = 4, // Compressed with PKWare zip algorithm
}

impl CompressionType {
    fn from_byte(b: u8) -> Result<Self> {
        ensure!(
            b <= 4,
            "invalid compression type byte '{}'; expected 0-4",
            b
        );
        Ok(match b {
            0 => CompressionType::None,
            1 => CompressionType::Lzss,
            3 => CompressionType::PxPk,
            4 => CompressionType::PkWare,
            _ => unreachable!(),
        })
    }

    fn name(&self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Lzss => Some("lzss"),
            Self::PxPk => Some("pxpk"),
            Self::PkWare => Some("pkware"),
        }
    }
}

#[derive(Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq)]
pub struct Priority {
    priority: usize,
    version: usize,
    adjust: i64,
}

impl Priority {
    fn from_path(path: &Path, adjust: i64) -> Result<Self> {
        lazy_static! {
            static ref PRIO_RE: Regex =
                Regex::new(r"(\d+)([a-zA-Z]?)").expect("failed to create regex");
        }
        let filename = path
            .file_stem()
            .ok_or_else(|| anyhow!("priority: name must not start with a '.'"))?
            .to_str()
            .ok_or_else(|| anyhow!("priority: name not utf8"))?
            .to_owned();
        if let Some(caps) = PRIO_RE.captures(&filename) {
            let priority = caps
                .get(1)
                .ok_or_else(|| anyhow!("priority: expected number match"))?
                .as_str()
                .parse::<usize>()?;
            let version = Self::version_from_char(caps.get(2));
            Ok(Self {
                priority,
                version,
                adjust,
            })
        } else {
            Ok(Self {
                priority: 0,
                version: 0,
                adjust,
            })
        }
    }

    fn version_from_char(opt: Option<regex::Match>) -> usize {
        if opt.is_none() {
            return 0;
        }
        let c = opt.unwrap().as_str().to_uppercase().chars().next();
        if c.is_none() {
            return 0;
        }
        (1u8 + c.unwrap() as u8 - b'A') as usize
    }

    pub fn as_drawer_priority(&self) -> i64 {
        let offset = (self.priority * 26 + self.version) as i64;
        -offset - self.adjust
    }
}

pub struct StatInfo {
    pub name: String,
    pub compression: CompressionType,
    pub packed_size: u64,
    pub unpacked_size: u64,
    pub path: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct PackedFileInfo {
    libkey: usize,
    start_offset: usize,
    end_offset: usize,
    compression: CompressionType,
}

impl PackedFileInfo {
    pub fn new(
        libkey: usize,
        start_offset: usize,
        end_offset: usize,
        compression: u8,
    ) -> Result<Self> {
        Ok(Self {
            libkey,
            start_offset,
            end_offset,
            compression: CompressionType::from_byte(compression)?,
        })
    }
}

packed_struct!(LibHeader {
    _0 => magic: [u8; 5], // EALIB
    _1 => count: u16
});

packed_struct!(LibEntry {
    _0 => name: [u8; 13],
    _1 => flags: u8,
    _2 => offset: u32
});

pub struct LibDrawer {
    drawer_index: HashMap<DrawerFileId, String>,
    index: HashMap<DrawerFileId, PackedFileInfo>,
    data: Mmap,
    priority: i64,
    name: String,
}

impl LibDrawer {
    pub fn from_path(priority: i64, path: &Path) -> Result<Box<dyn DrawerInterface>> {
        trace!("opening lib file {:?} with priority {}", path, priority);
        let fp = fs::File::open(path)?;
        let map = unsafe { MmapOptions::new().map(&fp)? };

        // Header
        ensure!(map.len() > mem::size_of::<LibHeader>(), "lib too short");
        let hdr_ptr: *const LibHeader = map.as_ptr() as *const _;
        let hdr: &LibHeader = unsafe { &*hdr_ptr };
        let magic = String::from_utf8(hdr.magic().to_vec())?;
        ensure!(magic == "EALIB", "lib missing magic");

        // Entries
        let mut drawer_index: HashMap<DrawerFileId, String> = HashMap::new();
        let mut index: HashMap<DrawerFileId, PackedFileInfo> = HashMap::new();
        let entries_start = mem::size_of::<LibHeader>();
        let entries_end = entries_start + hdr.count() as usize * mem::size_of::<LibEntry>();
        ensure!(map.len() > entries_end, "lib too short for entries");
        // FIXME: use LayoutVerified from zerocopy here
        let entries: &[LibEntry] = unsafe { mem::transmute(&map[entries_start..entries_end]) };
        for i in 0..hdr.count() as usize {
            let dfid = DrawerFileId::from_u32(i as u32);
            let entry = &entries[i];
            let name = String::from_utf8(entry.name().to_vec())?
                .trim_matches('\0')
                .to_uppercase();
            let end_offset = if i + 1 < hdr.count() as usize {
                entries[i + 1].offset() as usize
            } else {
                map.len()
            };
            // Note: there is at least one duplicate in ATF Gold's 2.LIB.
            let info = PackedFileInfo::new(0, entry.offset() as usize, end_offset, entry.flags())?;
            drawer_index.insert(dfid, name.clone());
            index.insert(dfid, info);
        }

        let name = path
            .file_name()
            .expect("a file")
            .to_string_lossy()
            .to_string();

        Ok(Box::new(Self {
            drawer_index,
            index,
            data: map,
            priority,
            name,
        }))
    }
}

pub fn from_dos_string(input: Cow<[u8]>) -> Cow<str> {
    match input {
        Cow::Borrowed(r) => Cow::borrow_from_cp437(r, &CP437_CONTROL),
        Cow::Owned(o) => Cow::from(String::from_cp437(o, &CP437_CONTROL)),
    }
}

#[async_trait]
impl DrawerInterface for LibDrawer {
    fn index(&self) -> Result<HashMap<DrawerFileId, String>> {
        Ok(self.drawer_index.clone())
    }

    fn priority(&self) -> i64 {
        self.priority
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn stat_sync(&self, id: DrawerFileId) -> Result<DrawerFileMetadata> {
        ensure!(self.index.contains_key(&id));
        let info = &self.index[&id];
        Ok(match info.compression {
            CompressionType::None => DrawerFileMetadata {
                drawer_file_id: id,
                name: self.drawer_index[&id].to_owned(),
                compression: info.compression.name(),
                packed_size: (info.end_offset - info.start_offset) as u64,
                unpacked_size: (info.end_offset - info.start_offset) as u64,
                path: None,
            },
            CompressionType::PkWare => {
                let dwords: &[u32] =
                    unsafe { mem::transmute(&self.data[info.start_offset..info.start_offset + 4]) };
                DrawerFileMetadata {
                    drawer_file_id: id,
                    name: self.drawer_index[&id].to_owned(),
                    compression: info.compression.name(),
                    packed_size: (info.end_offset - info.start_offset) as u64,
                    unpacked_size: u64::from(dwords[0]),
                    path: None,
                }
            }
            CompressionType::Lzss => {
                let dwords: &[u32] =
                    unsafe { mem::transmute(&self.data[info.start_offset..info.start_offset + 4]) };
                DrawerFileMetadata {
                    drawer_file_id: id,
                    name: self.drawer_index[&id].to_owned(),
                    compression: info.compression.name(),
                    packed_size: (info.end_offset - info.start_offset) as u64,
                    unpacked_size: u64::from(dwords[0]),
                    path: None,
                }
            }
            CompressionType::PxPk => unimplemented!(),
        })
    }

    fn read_sync(&self, id: DrawerFileId) -> Result<Cow<[u8]>> {
        ensure!(self.index.contains_key(&id));
        let info = &self.index[&id];
        Ok(match info.compression {
            CompressionType::None => Cow::from(&self.data[info.start_offset..info.end_offset]),
            CompressionType::PkWare => {
                assert!(info.start_offset + 4 <= info.end_offset);
                let expect_output_size = LittleEndian::read_u32(&self.data) as usize;
                Cow::from(pkware::explode(
                    &self.data[info.start_offset + 4..info.end_offset],
                    Some(expect_output_size),
                )?)
            }
            CompressionType::Lzss => {
                assert!(info.start_offset + 4 <= info.end_offset);
                let expect_output_size = LittleEndian::read_u32(&self.data) as usize;
                Cow::from(lzss::explode(
                    &self.data[info.start_offset + 4..info.end_offset],
                    Some(expect_output_size),
                )?)
            }
            CompressionType::PxPk => unimplemented!(),
        })
    }

    fn read_slice_sync(&self, id: DrawerFileId, extent: Range<usize>) -> Result<Cow<[u8]>> {
        ensure!(self.index.contains_key(&id));
        let info = &self.index[&id];
        Ok(match info.compression {
            CompressionType::None => {
                assert!(info.start_offset + extent.start <= info.end_offset);
                assert!(info.start_offset + extent.end <= info.end_offset);
                Cow::from(
                    &self.data[info.start_offset + extent.start..info.start_offset + extent.end],
                )
            }
            _ => unimplemented!("slice on compressed file"),
        })
    }

    async fn read(&self, id: DrawerFileId) -> Result<Vec<u8>> {
        ensure!(self.index.contains_key(&id));
        let info = &self.index[&id];
        // FIXME: make this more async friendly by spawn blocking off the io thread
        Ok(match info.compression {
            CompressionType::None => {
                let mut out = Vec::new();
                out.copy_from_slice(&self.data[info.start_offset..info.end_offset]);
                out
            }
            CompressionType::PkWare => {
                // FIXME: zerocopy
                let dwords: &[u32] =
                    unsafe { mem::transmute(&self.data[info.start_offset..info.start_offset + 4]) };
                let expect_output_size = Some(dwords[0] as usize);
                pkware::explode(
                    &self.data[info.start_offset + 4..info.end_offset],
                    expect_output_size,
                )?
            }
            CompressionType::Lzss => {
                // FIXME: zerocopy
                let dwords: &[u32] =
                    unsafe { mem::transmute(&self.data[info.start_offset..info.start_offset + 4]) };
                let expect_output_size = Some(dwords[0] as usize);
                lzss::explode(
                    &self.data[info.start_offset + 4..info.end_offset],
                    expect_output_size,
                )?
            }
            CompressionType::PxPk => unimplemented!(),
        })
    }

    async fn read_slice(&self, id: DrawerFileId, extent: Range<usize>) -> Result<Vec<u8>> {
        ensure!(self.index.contains_key(&id));
        let info = &self.index[&id];
        Ok(match info.compression {
            CompressionType::None => {
                assert!(info.start_offset + extent.start <= info.end_offset);
                assert!(info.start_offset + extent.end <= info.end_offset);
                let mut out = Vec::new();
                out.copy_from_slice(
                    &self.data[info.start_offset + extent.start..info.start_offset + extent.end],
                );
                out
            }
            _ => unimplemented!("slice on compressed file"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_catalog_builder() -> Result<()> {
        let _catalog = CatalogManager::for_testing()?;
        Ok(())
    }
}
