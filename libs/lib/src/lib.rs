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

extern crate codepage_437;
#[macro_use]
extern crate failure;
extern crate glob;
extern crate lzss;
extern crate memmap;
#[macro_use]
extern crate packed_struct;
extern crate pkware;
extern crate regex;

use codepage_437::{CP437_CONTROL, FromCp437};
use failure::Fallible;
use glob::{MatchOptions, Pattern};
use memmap::{Mmap, MmapOptions};
use regex::Regex;
use std::{
    borrow::Cow, collections::HashMap, ffi::OsStr, fs, mem, path::{Path, PathBuf}, str,
};

#[derive(Clone, Debug)]
pub enum CompressionType {
    None = 0,
    LZSS = 1,   // Compressed with LZSS
    PXPK = 3,   // No compression, but includes 4 byte inline header 'PXPK'
    PKWare = 4, // Compressed with PKWare zip algorithm
}

impl CompressionType {
    fn from_byte(b: u8) -> Fallible<Self> {
        ensure!(
            b <= 4,
            "invalid compression type byte '{}'; expected 0-4",
            b
        );
        return Ok(match b {
            0 => CompressionType::None,
            1 => CompressionType::LZSS,
            3 => CompressionType::PXPK,
            4 => CompressionType::PKWare,
            _ => unreachable!(),
        });
    }
}

#[derive(Debug)]
struct Priority {
    priority: usize,
    version: usize,
}

impl Priority {
    fn from_path(path: &Path) -> Fallible<Self> {
        let stem = path.file_stem();
        ensure!(stem.is_some(), "lib name: must no start with a '.'");
        let name = stem.unwrap().to_str();
        ensure!(name.is_some(), "lib name: must be utf8");
        let filename = name.unwrap();
        let re = Regex::new(r"(\d+)([a-zA-Z]?)")?;
        let maybe_caps = re.captures(&filename);
        ensure!(maybe_caps.is_some(), "lib name: must contain a number");
        let caps = maybe_caps.unwrap();
        let priority = caps.get(1).unwrap().as_str().parse::<usize>()?;
        let version = Self::version_from_char(caps.get(2));
        return Ok(Self { priority, version });
    }

    fn version_from_char(opt: Option<regex::Match>) -> usize {
        if opt.is_none() {
            return 0;
        }
        let c = opt.unwrap().as_str().to_uppercase().chars().next();
        if c.is_none() {
            return 0;
        }
        return (1u8 + c.unwrap() as u8 - 'A' as u8) as usize;
    }
}

pub struct StatInfo {
    pub name: String,
    pub compression: CompressionType,
    pub packed_size: usize,
    pub unpacked_size: usize,
}

#[derive(Clone, Debug)]
pub struct FileInfo {
    libkey: usize,
    start_offset: usize,
    end_offset: usize,
    compression: CompressionType,
}

impl FileInfo {
    pub fn new(
        libkey: usize,
        start_offset: usize,
        end_offset: usize,
        compression: u8,
    ) -> Fallible<Self> {
        return Ok(Self {
            libkey,
            start_offset,
            end_offset,
            compression: CompressionType::from_byte(compression)?,
        });
    }
}

pub struct LibFile {
    // The location of the lib file.
    _path: PathBuf,

    // The priority of a lib file increases with the number in the name.
    // Given two libs with the same name, the larger suffix letter wins.
    priority: Priority,

    // Index of the files in this library.
    local_index: HashMap<String, FileInfo>,

    // mmapped buffer
    data: Mmap,
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

impl LibFile {
    pub fn new(key: usize, path: &Path) -> Fallible<Self> {
        let fp = fs::File::open(path)?;
        let map = unsafe { MmapOptions::new().map(&fp)? };

        // Take priorty from the name.
        let priority = Priority::from_path(path)?;
        println!("{:?}: {:?}", path, priority);

        // Header
        ensure!(map.len() > mem::size_of::<LibHeader>(), "lib too short");
        let hdr_ptr: *const LibHeader = map.as_ptr() as *const _;
        let hdr: &LibHeader = unsafe { &*hdr_ptr };
        let magic = String::from_utf8(hdr.magic().to_vec())?;
        ensure!(magic == "EALIB", "lib missing magic");

        // Entries
        let mut local_index = HashMap::new();
        let entries_start = mem::size_of::<LibHeader>();
        let entries_end = entries_start + hdr.count() as usize * mem::size_of::<LibEntry>();
        ensure!(map.len() > entries_end, "lib too short for entries");
        let entries: &[LibEntry] = unsafe { mem::transmute(&map[entries_start..entries_end]) };
        for i in 0..hdr.count() as usize {
            let entry = &entries[i];
            let name = String::from_utf8(entry.name().to_vec())?
                .trim_matches('\0')
                .to_uppercase();
            let end_offset = if i + 1 < hdr.count() as usize {
                entries[i + 1].offset() as usize
            } else {
                map.len()
            };
            ensure!(
                !local_index.contains_key(&name),
                "duplicate filename in lib {:?}",
                path
            );
            local_index.insert(
                name,
                FileInfo::new(key, entry.offset() as usize, end_offset, entry.flags())?,
            );
        }

        return Ok(Self {
            _path: path.to_owned(),
            priority,
            local_index,
            data: map,
        });
    }
}

pub struct LibStack {
    // Map from lib file path to the local index.
    libs: HashMap<usize, LibFile>,

    // Keep a list of all hidden files.
    hidden: Vec<(String, LibFile, FileInfo)>,

    // Global index mapping file names to lib key.
    index: HashMap<String, FileInfo>,
}

impl LibStack {
    pub fn new_from_files(libfiles: &Vec<PathBuf>) -> Fallible<Self> {
        let mut libs = HashMap::new();
        let mut key = 0;
        for libfile in libfiles {
            let info = LibFile::new(key, &libfile)?;
            libs.insert(key, info);
            key += 1;
        }
        let mut index = HashMap::new();
        for lib in libs.values() {
            for (name, info) in lib.local_index.iter() {
                // TODO: proper masking
                // ensure!(
                //     !index.contains_key(name),
                //     "duplicate filename in lib stack: {}",
                //     name
                // );
                index.insert(name.to_owned(), info.clone());
            }
        }
        return Ok(Self {
            libs,
            hidden: Vec::new(),
            index,
        });
    }

    /// Find all lib files under search path and index them.
    pub fn new_from_search_under(search_path: &Path) -> Fallible<Self> {
        let libfiles = Self::find_all_libs_under(search_path)?;
        return Self::new_from_files(&libfiles);
    }

    pub fn new_for_test() -> Fallible<Self> {
        let libdirs = fs::read_dir("../../test_data/unpacked/FA/")?;
        //LibStack::new_from_directories(libdirs)?
        LibStack::new_from_search_under(Path::new("../../test_data/FA"))
    }

    pub fn file_exists(&self, filename: &str) -> bool {
        return self.index.get(filename).is_some();
    }

    pub fn load(&self, filename: &str) -> Fallible<Cow<[u8]>> {
        ensure!(filename.len() > 0, "cannot load empty file");
        if let Some(info) = self.index.get(filename) {
            if let Some(lib) = self.libs.get(&info.libkey) {
                match info.compression {
                    CompressionType::None => {
                        return Ok(Cow::from(&lib.data[info.start_offset..info.end_offset]));
                    }
                    CompressionType::PKWare => {
                        let dwords: &[u32] = unsafe {
                            mem::transmute(&lib.data[info.start_offset..info.start_offset + 4])
                        };
                        let expect_output_size = Some(dwords[0] as usize);
                        return Ok(Cow::from(pkware::explode(
                            &lib.data[info.start_offset + 4..info.end_offset],
                            expect_output_size,
                        )?));
                    }
                    CompressionType::LZSS => {
                        let dwords: &[u32] = unsafe {
                            mem::transmute(&lib.data[info.start_offset..info.start_offset + 4])
                        };
                        let expect_output_size = Some(dwords[0] as usize);
                        return Ok(Cow::from(lzss::explode(
                            &lib.data[info.start_offset + 4..info.end_offset],
                            expect_output_size,
                        )?));
                    }
                    CompressionType::PXPK => unimplemented!(),
                }
            }
            panic!("found file in index with invalid libkey: {:?}", info);
        }
        bail!("no such file {} in index", filename);
    }

    pub fn stat(&self, filename: &str) -> Fallible<StatInfo> {
        ensure!(filename.len() > 0, "cannot load empty file");
        if let Some(info) = self.index.get(filename) {
            if let Some(lib) = self.libs.get(&info.libkey) {
                match info.compression {
                    CompressionType::None => {
                        return Ok(StatInfo {
                            name: filename.to_owned(),
                            compression: info.compression.clone(),
                            packed_size: info.end_offset - info.start_offset,
                            unpacked_size: info.end_offset - info.start_offset,
                        });
                    }
                    CompressionType::PKWare => {
                        let dwords: &[u32] = unsafe {
                            mem::transmute(&lib.data[info.start_offset..info.start_offset + 4])
                        };
                        return Ok(StatInfo {
                            name: filename.to_owned(),
                            compression: info.compression.clone(),
                            packed_size: info.end_offset - info.start_offset,
                            unpacked_size: dwords[0] as usize,
                        });
                    }
                    CompressionType::LZSS => {
                        let dwords: &[u32] = unsafe {
                            mem::transmute(&lib.data[info.start_offset..info.start_offset + 4])
                        };
                        return Ok(StatInfo {
                            name: filename.to_owned(),
                            compression: info.compression.clone(),
                            packed_size: info.end_offset - info.start_offset,
                            unpacked_size: dwords[0] as usize,
                        });
                    }
                    CompressionType::PXPK => unimplemented!(),
                }
            }
            panic!("found file in index with invalid libkey: {:?}", info);
        }
        bail!("no such file {} in index", filename);
    }

    pub fn load_text(&self, filename: &str) -> Fallible<String> {
        let contents = self.load(filename)?.to_vec();
        return Ok(String::from_cp437(contents, &CP437_CONTROL));
    }

    pub fn find_matching(&self, glob: &str) -> Fallible<Vec<String>> {
        let mut matching = Vec::new();
        let opts = MatchOptions {
            case_sensitive: false,
            require_literal_leading_dot: false,
            require_literal_separator: true,
        };
        let pattern = Pattern::new(glob)?;
        for key in self.index.keys() {
            if pattern.matches_with(key, &opts) {
                matching.push(key.to_owned());
            }
        }
        return Ok(matching);
    }

    fn find_all_libs_under(path: &Path) -> Fallible<Vec<PathBuf>> {
        let mut out = Vec::new();
        for maybe_child in fs::read_dir(path)? {
            let child = maybe_child?;
            if child.file_type()?.is_dir() {
                out.append(&mut Self::find_all_libs_under(&child.path())?);
            } else if child.path().extension() == Some(OsStr::new("lib"))
                || child.path().extension() == Some(OsStr::new("LIB"))
            {
                out.push(child.path().to_owned());
            }
        }
        return Ok(out);
    }
}

/// Hold multiple LibStacks at once: e.g. for visiting resources from multiple games at once.
struct OmniLib {
    stacks: HashMap<String, LibStack>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_load_and_match_all_files() -> Fallible<()> {
        let libs = LibStack::new_from_search_under(Path::new("../../test_data/packed/FA"))?;
        for name in libs.find_matching("*")?.iter() {
            println!("At: {}", name);
            let data = libs.load(name)?;
            assert!((data[0] as usize + data[data.len() - 1] as usize) < 512);
        }
        return Ok(());
    }
}
