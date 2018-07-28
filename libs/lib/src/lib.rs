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
extern crate memmap;
#[macro_use]
extern crate packed_struct;
extern crate pkware;

use codepage_437::{CP437_CONTROL, FromCp437};
use failure::Error;
use glob::{MatchOptions, Pattern};
use memmap::{Mmap, MmapOptions};
use std::{
    borrow::Cow, collections::HashMap, ffi::OsStr, fs, mem, path::{Path, PathBuf}, str,
};

#[derive(Clone, Debug)]
pub struct FileInfo {
    libkey: usize,
    start_offset: usize,
    end_offset: usize,
    flags: u8,
}

impl FileInfo {
    pub fn new(libkey: usize, start_offset: usize, end_offset: usize, flags: u8) -> Self {
        Self {
            libkey,
            start_offset,
            end_offset,
            flags,
        }
    }
}

pub struct StatInfo {
    pub name: String,
    pub packed_size: usize,
    pub unpacked_size: usize,
}

pub struct LibInfo {
    // The location of the lib file.
    _path: PathBuf,

    // The priority of a lib file increases with the number in the name.
    // Given two libs with the same name, the larger suffix letter wins.
    // We stuff the number into the top octet and the letter into the
    // bottom octet (as offset from 'a') in order to get the right sort
    // order.
    _priority: u16,

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

impl LibInfo {
    pub fn new(key: usize, path: &Path) -> Result<Self, Error> {
        let fp = fs::File::open(path)?;
        let map = unsafe { MmapOptions::new().map(&fp)? };

        // Header
        ensure!(map.len() > mem::size_of::<LibHeader>(), "lib too short");
        let hdr_ptr: *const LibHeader = map.as_ptr() as *const _;
        let hdr: &LibHeader = unsafe { &*hdr_ptr };
        let magic = String::from_utf8(hdr.magic().to_vec())?;
        ensure!(magic == "EALIB", "lib missing magic");

        // Entries
        let mut index = HashMap::new();
        let entries_start = mem::size_of::<LibHeader>();
        let entries_end = entries_start + hdr.count() as usize * mem::size_of::<LibEntry>();
        ensure!(map.len() > entries_end, "lib too short for entries");
        let entries: &[LibEntry] = unsafe { mem::transmute(&map[entries_start..entries_end]) };
        for i in 0..hdr.count() as usize {
            let entry = &entries[i];
            let name = String::from_utf8(entry.name().to_vec())?
                .trim_matches('\0')
                .to_uppercase();
            ensure!(
                entry.flags() == 0 || entry.flags() == 4,
                "unknown flag {:02X} at {} in {:?}",
                entry.flags(),
                name,
                path
            );
            let end_offset = if i + 1 < hdr.count() as usize {
                entries[i + 1].offset() as usize
            } else {
                map.len()
            };
            index.insert(
                name,
                FileInfo::new(key, entry.offset() as usize, end_offset, entry.flags()),
            );
        }

        return Ok(Self {
            _path: path.to_owned(),
            _priority: 0,
            local_index: index,
            data: map,
        });
    }
}

pub struct Library {
    // Map from lib file path to the local index.
    libs: HashMap<usize, LibInfo>,

    // Global index mapping file names to lib key.
    index: HashMap<String, FileInfo>,
}

impl Library {
    pub fn new_from_files(libfiles: &Vec<PathBuf>) -> Result<Self, Error> {
        let mut libs = HashMap::new();
        let mut key = 0;
        for libfile in libfiles {
            let info = LibInfo::new(key, &libfile)?;
            libs.insert(key, info);
            key += 1;
        }
        let mut index = HashMap::new();
        for lib in libs.values() {
            for (name, info) in lib.local_index.iter() {
                index.insert(name.to_owned(), info.clone());
            }
        }
        return Ok(Self { libs, index });
    }

    /// Find all lib files under search path and index them.
    pub fn new_from_search_under(search_path: &Path) -> Result<Self, Error> {
        let libfiles = Self::find_all_libs_under(search_path)?;
        return Self::new_from_files(&libfiles);
    }

    /// Load a useful set of libraries for testing.
    //#[cfg(test)]
    pub fn new_for_test() -> Result<Self, Error> {
        // TODO: maybe do something smarter here?
        Library::new_from_search_under(Path::new("../../test_data/FA"))
    }

    pub fn file_exists(&self, filename: &str) -> bool {
        return self.index.get(filename).is_some();
    }

    pub fn load(&self, filename: &str) -> Result<Cow<[u8]>, Error> {
        ensure!(filename.len() > 0, "cannot load empty file");
        if let Some(info) = self.index.get(filename) {
            if let Some(lib) = self.libs.get(&info.libkey) {
                if info.flags == 0 {
                    return Ok(Cow::from(&lib.data[info.start_offset..info.end_offset]));
                } else {
                    let dwords: &[u32] = unsafe {
                        mem::transmute(&lib.data[info.start_offset..info.start_offset + 4])
                    };
                    let expect_output_size = Some(dwords[0] as usize);
                    return Ok(Cow::from(pkware::explode(
                        &lib.data[info.start_offset + 4..info.end_offset],
                        expect_output_size,
                    )?));
                    // use std::fs::File;
                    // use std::io::Write;
                    // let mut fp = File::create(format!("test_data/{}.pkware.zip", filename))?;
                    // fp.write(&lib.data[info.start_offset + 4..info.end_offset]);
                    // return Ok(Cow::from(&lib.data[info.start_offset + 4..info.end_offset]));
                }
            }
            panic!("found file in index with invalid libkey: {:?}", info);
        }
        bail!("no such file {} in index", filename);
    }

    pub fn stat(&self, filename: &str) -> Result<StatInfo, Error> {
        ensure!(filename.len() > 0, "cannot load empty file");
        if let Some(info) = self.index.get(filename) {
            if let Some(lib) = self.libs.get(&info.libkey) {
                if info.flags == 0 {
                    return Ok(StatInfo {
                        name: filename.to_owned(),
                        packed_size: info.end_offset - info.start_offset,
                        unpacked_size: info.end_offset - info.start_offset,
                    });
                } else {
                    let dwords: &[u32] = unsafe {
                        mem::transmute(&lib.data[info.start_offset..info.start_offset + 4])
                    };
                    return Ok(StatInfo {
                        name: filename.to_owned(),
                        packed_size: info.end_offset - info.start_offset,
                        unpacked_size: dwords[0] as usize,
                    });
                }
            }
            panic!("found file in index with invalid libkey: {:?}", info);
        }
        bail!("no such file {} in index", filename);
    }

    pub fn load_text(&self, filename: &str) -> Result<String, Error> {
        let contents = self.load(filename)?.to_vec();
        return Ok(String::from_cp437(contents, &CP437_CONTROL));
    }

    pub fn find_matching(&self, glob: &str) -> Result<Vec<String>, Error> {
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

    fn find_all_libs_under(path: &Path) -> Result<Vec<PathBuf>, Error> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_load_and_match_all_files() {
        let library = Library::new_from_search_under(Path::new("../../test_data/FA")).unwrap();
        for name in library.find_matching("*").unwrap().iter() {
            println!("At: {}", name);
            let data = library.load(name).unwrap();
            assert!((data[0] as usize + data[data.len() - 1] as usize) < 512);
        }
    }
}
