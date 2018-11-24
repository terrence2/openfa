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

#![cfg_attr(feature = "cargo-clippy", allow(transmute_ptr_to_ptr))]

extern crate codepage_437;
extern crate failure;
extern crate glob;
extern crate lazy_static;
extern crate lzss;
extern crate memmap;
extern crate packed_struct;
extern crate pkware;
extern crate regex;

use codepage_437::{BorrowFromCp437, FromCp437, CP437_CONTROL};
use failure::{bail, ensure, err_msg, Fallible};
use glob::{MatchOptions, Pattern};
use lazy_static::lazy_static;
use memmap::{Mmap, MmapOptions};
use packed_struct::packed_struct;
use regex::Regex;
use std::{
    borrow::Cow,
    collections::HashMap,
    ffi::OsStr,
    fs,
    io::Read,
    mem,
    path::{Path, PathBuf},
    str,
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
        Ok(match b {
            0 => CompressionType::None,
            1 => CompressionType::LZSS,
            3 => CompressionType::PXPK,
            4 => CompressionType::PKWare,
            _ => unreachable!(),
        })
    }
}

#[derive(Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq)]
pub struct Priority {
    priority: usize,
    version: usize,
}

impl Priority {
    fn from_path(path: &Path) -> Fallible<Self> {
        lazy_static! {
            static ref PRIO_RE: Regex =
                Regex::new(r"(\d+)([a-zA-Z]?)").expect("failed to create regex");
        }
        let filename = path
            .file_stem()
            .ok_or_else(|| err_msg("priority: name must not start with a '.'"))?
            .to_str()
            .ok_or_else(|| err_msg("priority: name not utf8"))?
            .to_owned();
        let caps = PRIO_RE
            .captures(&filename)
            .ok_or_else(|| err_msg("priority: name must contain a number"))?;
        let priority = caps
            .get(1)
            .ok_or_else(|| err_msg("priority: expected number match"))?
            .as_str()
            .parse::<usize>()?;
        let version = Self::version_from_char(caps.get(2));
        Ok(Self { priority, version })
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
    ) -> Fallible<Self> {
        Ok(Self {
            libkey,
            start_offset,
            end_offset,
            compression: CompressionType::from_byte(compression)?,
        })
    }
}

pub struct LibFile {
    // Index of the files in this library.
    local_index: HashMap<String, PackedFileInfo>,

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
    pub fn from_path(key: usize, path: &Path) -> Fallible<Self> {
        let fp = fs::File::open(path)?;
        let map = unsafe { MmapOptions::new().map(&fp)? };

        // Header
        ensure!(map.len() > mem::size_of::<LibHeader>(), "lib too short");
        let hdr_ptr: *const LibHeader = map.as_ptr() as *const _;
        let hdr: &LibHeader = unsafe { &*hdr_ptr };
        let magic = String::from_utf8(hdr.magic().to_vec())?;
        ensure!(magic == "EALIB", "lib missing magic");

        // Entries
        let mut local_index: HashMap<String, PackedFileInfo> = HashMap::new();
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
            // This occurs at least once ATF Gold's 2.LIB.
            if local_index.contains_key(&name) {
                let new_name = format!("__rename{}__{}", i, name);
                let fileinfo = local_index[&name].clone();
                local_index.insert(new_name, fileinfo);
            }
            local_index.insert(
                name,
                PackedFileInfo::new(key, entry.offset() as usize, end_offset, entry.flags())?,
            );
        }

        Ok(Self {
            local_index,
            data: map,
        })
    }

    pub fn load(&self, info: &PackedFileInfo) -> Fallible<Cow<[u8]>> {
        Ok(match info.compression {
            CompressionType::None => Cow::from(&self.data[info.start_offset..info.end_offset]),
            CompressionType::PKWare => {
                let dwords: &[u32] =
                    unsafe { mem::transmute(&self.data[info.start_offset..info.start_offset + 4]) };
                let expect_output_size = Some(dwords[0] as usize);
                Cow::from(pkware::explode(
                    &self.data[info.start_offset + 4..info.end_offset],
                    expect_output_size,
                )?)
            }
            CompressionType::LZSS => {
                let dwords: &[u32] =
                    unsafe { mem::transmute(&self.data[info.start_offset..info.start_offset + 4]) };
                let expect_output_size = Some(dwords[0] as usize);
                Cow::from(lzss::explode(
                    &self.data[info.start_offset + 4..info.end_offset],
                    expect_output_size,
                )?)
            }
            CompressionType::PXPK => unimplemented!(),
        })
    }

    pub fn stat(&self, filename: &str, info: &PackedFileInfo) -> Fallible<StatInfo> {
        Ok(match info.compression {
            CompressionType::None => StatInfo {
                name: filename.to_owned(),
                compression: info.compression.clone(),
                packed_size: (info.end_offset - info.start_offset) as u64,
                unpacked_size: (info.end_offset - info.start_offset) as u64,
                path: None,
            },
            CompressionType::PKWare => {
                let dwords: &[u32] =
                    unsafe { mem::transmute(&self.data[info.start_offset..info.start_offset + 4]) };
                StatInfo {
                    name: filename.to_owned(),
                    compression: info.compression.clone(),
                    packed_size: (info.end_offset - info.start_offset) as u64,
                    unpacked_size: u64::from(dwords[0]),
                    path: None,
                }
            }
            CompressionType::LZSS => {
                let dwords: &[u32] =
                    unsafe { mem::transmute(&self.data[info.start_offset..info.start_offset + 4]) };
                StatInfo {
                    name: filename.to_owned(),
                    compression: info.compression.clone(),
                    packed_size: (info.end_offset - info.start_offset) as u64,
                    unpacked_size: u64::from(dwords[0]),
                    path: None,
                }
            }
            CompressionType::PXPK => unimplemented!(),
        })
    }
}

#[derive(Clone, Debug)]
pub struct UnpackedFileInfo {
    libkey: usize,
    path: PathBuf,
}

pub struct LibDir {
    local_index: HashMap<String, UnpackedFileInfo>,
}

impl LibDir {
    pub fn from_path(libkey: usize, path: &Path) -> Fallible<Self> {
        let mut local_index = HashMap::new();
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            if !entry.path().is_file() {
                continue;
            }
            let filename = entry
                .path()
                .file_name()
                .ok_or_else(|| err_msg("libdir: no filename in file"))?
                .to_owned();
            let name = filename
                .to_str()
                .ok_or_else(|| {
                    err_msg(format!(
                        "libdir: non-utf8 characters in file: {:?}",
                        filename,
                    ))
                })?.to_owned();
            let rv = local_index.insert(
                name,
                UnpackedFileInfo {
                    libkey,
                    path: entry.path(),
                },
            );
            assert!(rv.is_none());
        }
        Ok(Self { local_index })
    }

    pub fn load(&self, info: &UnpackedFileInfo) -> Fallible<Cow<[u8]>> {
        let mut fp = fs::File::open(&info.path)?;
        let mut content = Vec::new();
        fp.read_to_end(&mut content)?;
        Ok(Cow::from(content))
    }

    pub fn stat(&self, filename: &str, info: &UnpackedFileInfo) -> Fallible<StatInfo> {
        let stat = fs::metadata(&info.path)?;
        Ok(StatInfo {
            name: filename.to_owned(),
            compression: CompressionType::None,
            packed_size: stat.len(),
            unpacked_size: stat.len(),
            path: Some(info.path.clone()),
        })
    }
}

pub enum LibraryData {
    File(LibFile),
    Dir(LibDir),
}

pub struct Library {
    // The assigned key. The key is used to avoid a circular reference
    // from the FileInfo structures back to the owning index.
    _libkey: usize,

    // The location of the lib file.
    path: PathBuf,

    // The priority of a lib file increases with the number in the name.
    // Given two libs with the same name, the larger suffix letter wins.
    _priority: Priority,

    // The index + data to load actual content.
    data: LibraryData,
}

impl Library {
    pub fn from_path(priority: &Priority, libkey: usize, path: &Path) -> Fallible<Self> {
        let data = if path.is_file() {
            LibraryData::File(LibFile::from_path(libkey, path)?)
        } else if path.is_dir() {
            LibraryData::Dir(LibDir::from_path(libkey, path)?)
        } else {
            bail!("library: tried to open non-file");
        };
        Ok(Library {
            _libkey: libkey,
            path: path.to_owned(),
            _priority: priority.to_owned(),
            data,
        })
    }

    pub fn build_index(
        &self,
        index: &mut HashMap<String, FileRef>,
        masked: &mut Vec<(String, FileRef)>,
    ) -> Fallible<()> {
        match self.data {
            LibraryData::File(ref libfile) => {
                for (name, info) in libfile.local_index.iter() {
                    let removed = index.insert(name.to_owned(), FileRef::Packed(info.to_owned()));
                    if let Some(overwritten) = removed {
                        masked.push((name.to_owned(), overwritten));
                    }
                }
            }
            LibraryData::Dir(ref libdir) => {
                for (name, info) in libdir.local_index.iter() {
                    let removed = index.insert(name.to_owned(), FileRef::Unpacked(info.to_owned()));
                    if let Some(overwritten) = removed {
                        masked.push((name.to_owned(), overwritten));
                    }
                }
            }
        }
        Ok(())
    }

    pub fn libfile(&self) -> Fallible<&LibFile> {
        match self.data {
            LibraryData::File(ref libfile) => Ok(libfile),
            LibraryData::Dir(_) => bail!("library: not a libfile"),
        }
    }

    pub fn libdir(&self) -> Fallible<&LibDir> {
        match self.data {
            LibraryData::Dir(ref libdir) => Ok(libdir),
            LibraryData::File(_) => bail!("library: not a libdir"),
        }
    }
}

pub enum FileRef {
    Packed(PackedFileInfo),
    Unpacked(UnpackedFileInfo),
}

impl FileRef {
    pub fn owning_library<'a>(&self, libs: &'a [Library]) -> &'a Library {
        match self {
            FileRef::Packed(ref fileinfo) => &libs[fileinfo.libkey],
            FileRef::Unpacked(ref fileinfo) => &libs[fileinfo.libkey],
        }
    }

    pub fn load<'a>(&self, libs: &'a [Library]) -> Fallible<Cow<'a, [u8]>> {
        match self {
            FileRef::Packed(ref fileinfo) => {
                let lib = &libs[fileinfo.libkey];
                Ok(lib.libfile()?.load(fileinfo)?)
            }
            FileRef::Unpacked(ref fileinfo) => {
                let lib = &libs[fileinfo.libkey];
                Ok(lib.libdir()?.load(fileinfo)?)
            }
        }
    }

    pub fn stat(&self, filename: &str, libs: &[Library]) -> Fallible<StatInfo> {
        match self {
            FileRef::Packed(ref fileinfo) => {
                let lib = &libs[fileinfo.libkey];
                Ok(lib.libfile()?.stat(filename, fileinfo)?)
            }
            FileRef::Unpacked(ref fileinfo) => {
                let lib = &libs[fileinfo.libkey];
                Ok(lib.libdir()?.stat(filename, fileinfo)?)
            }
        }
    }
}

pub struct LibStack {
    // Offset into this vec is the libkey. This should be sorted by priority.
    libs: Vec<Library>,

    // Global index mapping file names to FileInfo.
    index: HashMap<String, FileRef>,

    // Keep a list of all files that are hidden by a higher priority file.
    masked: Vec<(String, FileRef)>,
}

impl LibStack {
    pub fn from_paths(libpaths: &[PathBuf]) -> Fallible<Self> {
        // Ensure that all libs in the stack have a unique priority.
        let mut priorities = HashMap::new();
        for libpath in libpaths {
            let prio = Priority::from_path(&libpath)?;
            ensure!(
                !priorities.contains_key(&prio),
                "libstack: trying to load two libs with same priority: {:?} and {:?}",
                libpath,
                priorities[&prio]
            );
            priorities.insert(prio, libpath);
        }

        // Get all priorities in sorted order.
        let mut sorted_priorities = priorities.keys().collect::<Vec<_>>();
        sorted_priorities.sort();
        let sorted_priorities = sorted_priorities;

        // Load libraries in sorted order. This lets us use the index as a key to
        // avoid a second hash lookup in the load path.
        let mut libs = Vec::new();
        for (libkey, prio) in sorted_priorities.iter().enumerate() {
            libs.push(Library::from_path(&prio, libkey, priorities[prio])?);
        }

        // Build the global index from names to direct references.
        let mut index = HashMap::new();
        let mut masked = Vec::new();
        for lib in libs.iter() {
            lib.build_index(&mut index, &mut masked)?;
        }

        Ok(Self {
            libs,
            index,
            masked,
        })
    }

    /// Find all lib files under search path and index them.
    pub fn from_file_search(search_path: &Path) -> Fallible<Self> {
        let libfiles = Self::find_all_lib_files_under(search_path)?;
        Self::from_paths(&libfiles)
    }

    /// Find all lib directories under search path and index them.
    pub fn from_dir_search(search_path: &Path) -> Fallible<Self> {
        let libdirs = Self::find_all_lib_dirs_under(search_path)?;
        Self::from_paths(&libdirs)
    }

    pub fn file_exists(&self, filename: &str) -> bool {
        self.index.get(filename).is_some()
    }

    pub fn stat(&self, filename: &str) -> Fallible<StatInfo> {
        ensure!(!filename.is_empty(), "cannot load empty file");
        if let Some(info) = self.index.get(filename) {
            return Ok(info.stat(filename, &self.libs)?);
        }
        bail!("no such file {} in index", filename)
    }

    pub fn load(&self, filename: &str) -> Fallible<Cow<[u8]>> {
        ensure!(!filename.is_empty(), "cannot load empty file");
        if let Some(info) = self.index.get(filename) {
            return Ok(info.load(&self.libs)?);
        }
        bail!("no such file {} in index", filename)
    }

    pub fn load_text(&self, filename: &str) -> Fallible<Cow<str>> {
        Ok(match self.load(filename)? {
            Cow::Borrowed(r) => Cow::borrow_from_cp437(r, &CP437_CONTROL),
            Cow::Owned(o) => Cow::from(String::from_cp437(o, &CP437_CONTROL)),
        })
    }

    /// Load the masked filename from the given libpath.
    pub fn load_masked_text(&self, filename: &str, libpath: &Path) -> Fallible<String> {
        for (name, fileref) in self.masked.iter() {
            if name != filename {
                continue;
            }
            if libpath != fileref.owning_library(&self.libs).path.clone() {
                continue;
            }
            let contents = fileref.load(&self.libs)?.to_vec();
            return Ok(String::from_cp437(contents, &CP437_CONTROL));
        }
        bail!("libstack: no masked file {:?}/{}", libpath, filename)
    }

    /// Find all files with filename that have been masked.
    pub fn find_masked(&self, filename: &str) -> Fallible<Vec<PathBuf>> {
        let mut out = Vec::new();
        for (name, fileref) in self.masked.iter() {
            if name != filename {
                continue;
            }
            out.push(fileref.owning_library(&self.libs).path.clone());
        }
        Ok(out)
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
        Ok(matching)
    }

    fn find_all_lib_files_under(path: &Path) -> Fallible<Vec<PathBuf>> {
        let mut out = Vec::new();
        for maybe_child in fs::read_dir(path)? {
            let child = maybe_child?;
            if child.file_type()?.is_dir() {
                out.append(&mut Self::find_all_lib_files_under(&child.path())?);
            } else if child.path().extension() == Some(OsStr::new("lib"))
                || child.path().extension() == Some(OsStr::new("LIB"))
            {
                out.push(child.path().to_owned());
            }
        }
        Ok(out)
    }

    fn find_all_lib_dirs_under(path: &Path) -> Fallible<Vec<PathBuf>> {
        let mut out = Vec::new();
        for maybe_child in fs::read_dir(path)? {
            let child = maybe_child?;
            if !child.file_type()?.is_dir() {
                continue;
            }
            if child.path().extension() == Some(OsStr::new("lib"))
                || child.path().extension() == Some(OsStr::new("LIB"))
            {
                out.push(child.path());
            } else {
                out.append(&mut Self::find_all_lib_dirs_under(&child.path())?);
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_load_all_files_in_all_libfiles() -> Fallible<()> {
        let libs = LibStack::from_file_search(Path::new("../../test_data/packed/FA"))?;
        for name in libs.find_matching("*")?.iter() {
            println!("At: {}", name);
            let data = libs.load(name)?;
            assert!((data[0] as usize + data[data.len() - 1] as usize) < 512);
        }
        return Ok(());
    }

    #[test]
    fn can_load_all_files_in_all_libdirs() -> Fallible<()> {
        let libs = LibStack::from_dir_search(Path::new("../../test_data/unpacked/FA"))?;
        for name in libs.find_matching("*")?.iter() {
            println!("At: {}", name);
            let data = libs.load(name)?;
            assert!((data[0] as usize + data[data.len() - 1] as usize) < 512);
        }
        return Ok(());
    }

    #[test]
    fn mask_lower_priority_files() -> Fallible<()> {
        let libs = LibStack::from_dir_search(Path::new("./test_data/masking"))?;
        let txt = libs.load_text("FILE.TXT")?;
        assert_eq!(txt, "20b\n");
        let libpaths = libs.find_masked("FILE.TXT")?;
        for libpath in libpaths.iter() {
            let txt = libs.load_masked_text("FILE.TXT", libpath)?;
            assert!(txt == "10\n" || txt == "20\n" || txt == "20a\n");
        }
        return Ok(());
    }
}
