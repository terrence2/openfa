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
use failure::{err_msg, Fallible};
use lib::Library;
use log::trace;
use std::{collections::HashMap, path::Path, sync::Arc};

/// Hold multiple LibStacks at once: e.g. for visiting resources from multiple games at once.
pub struct OmniLib {
    libraries: HashMap<String, Arc<Box<Library>>>,
}

// Tests run dramatically slower when using the libs because we cannot force sub-libraries to
// be built with optimizations if we are building the using library without them.
const USE_LIB: bool = false;

impl OmniLib {
    pub fn new_for_test() -> Fallible<Self> {
        Self::new_for_test_in_games(&["FA", "USNF97", "ATFGOLD", "ATFNATO", "ATF", "MF", "USNF"])
    }

    pub fn new_for_test_in_games(dirs: &[&str]) -> Fallible<Self> {
        let mut libraries = HashMap::new();
        for &dir in dirs {
            trace!("adding libraries for {}", dir);

            let libs = if USE_LIB {
                let path = Path::new("../../test_data/packed/").join(dir);
                Library::from_file_search(&path)?
            } else {
                let path = Path::new("../../test_data/unpacked/").join(dir);
                Library::from_dir_search(&path)?
            };
            libraries.insert(dir.to_owned(), Arc::new(Box::new(libs)));
        }
        Ok(Self { libraries })
    }

    // Library from_dir_search in every subdir in the given path.
    // Note: we don't need this for testing and for non-testing we only care about the version
    // that looks for lib files, so not much point. Keeping it here for now in case I'm wrong.
    /*
    pub fn from_subdirs(path: &Path) -> Fallible<Self> {
        let mut libraries = HashMap::new();
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            if !entry.path().is_dir() {
                continue;
            }
            let name = entry
                .path()
                .file_name()
                .ok_or_else(|| err_msg("omnilib: no file name"))?
                .to_str()
                .ok_or_else(|| err_msg("omnilib: file name not utf8"))?
                .to_owned();
            let libs = Library::from_dir_search(&entry.path())?;
            libraries.insert(name, Arc::new(Box::new(libs)));
        }
        Ok(Self { libraries })
    }
    */

    pub fn find_matching(&self, glob: &str) -> Fallible<Vec<(String, String)>> {
        let mut out = Vec::new();
        for (libname, libs) in self.libraries.iter() {
            let names = libs.find_matching(glob)?;
            for name in names {
                out.push((libname.to_owned(), name));
            }
        }
        out.sort();
        Ok(out)
    }

    pub fn libraries(&self) -> Vec<Arc<Box<Library>>> {
        self.libraries.values().cloned().collect()
    }

    pub fn library(&self, libname: &str) -> Arc<Box<Library>> {
        self.libraries[libname].clone()
    }

    pub fn path(&self, libname: &str, name: &str) -> Fallible<String> {
        Ok(self
            .library(libname)
            .stat(name)?
            .path
            .ok_or_else(|| err_msg("no path for name"))?
            .to_str()
            .ok_or_else(|| err_msg("path with invalid characters"))?
            .to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_omnilib_from_dir() -> Fallible<()> {
        let omni = OmniLib::new_for_test()?;
        let palettes = omni.find_matching("PALETTE.PAL")?;
        assert!(!palettes.is_empty());
        Ok(())
    }
}
