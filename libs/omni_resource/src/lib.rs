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
extern crate failure;
extern crate lib;
extern crate resource_manager;

use failure::{err_msg, Fallible};
use lib::LibStack;
use resource_manager::ResourceManager;
use std::{borrow::Cow, collections::HashMap, fs, path::Path};

/// Hold multiple LibStacks at once: e.g. for visiting resources from multiple games at once.
pub struct OmniResource {
    stacks: HashMap<String, ResourceManager>,
}

impl OmniResource {
    pub fn new_for_test() -> Fallible<Self> {
        Self::from_subdirs(Path::new("../../test_data/unpacked"))
    }

    pub fn new_for_test_in_games(dirs: Vec<&str>) -> Fallible<Self> {
        let mut stacks = HashMap::new();
        for dir in dirs {
            let path = Path::new("../../test_data/unpacked/").join(dir);
            let libs = LibStack::from_dir_search(&path)?;
            let rm = ResourceManager::new_headless(libs)?;
            stacks.insert(dir.to_owned(), rm);
        }
        return Ok(Self { stacks });
    }

    // LibStack from_dir_search in every subdir in the given path.
    pub fn from_subdirs(path: &Path) -> Fallible<Self> {
        let mut stacks = HashMap::new();
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
            let libs = LibStack::from_dir_search(&entry.path())?;
            let rm = ResourceManager::new_headless(libs)?;
            stacks.insert(name, rm);
        }
        return Ok(Self { stacks });
    }

    pub fn find_matching(&self, glob: &str) -> Fallible<Vec<(String, String)>> {
        let mut out = Vec::new();
        for (libname, res) in self.stacks.iter() {
            let names = res.library().find_matching(glob)?;
            for name in names {
                out.push((libname.to_owned(), name));
            }
        }
        out.sort();
        return Ok(out);
    }

    pub fn resource_manager(&self, libname: &str) -> &ResourceManager {
        return &self.stacks[libname];
    }

    pub fn path(&self, libname: &str, name: &str) -> Fallible<String> {
        return Ok(self.resource_manager(libname)
            .library()
            .stat(name)?
            .path
            .ok_or_else(|| err_msg("no path for name"))?
            .to_str()
            .ok_or_else(|| err_msg("path with invalid characters"))?
            .to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_omnilib_from_dir() -> Fallible<()> {
        let omni = OmniLib::from_subdirs(Path::new("../../test_data/unpacked"))?;
        return Ok(());
    }
}
