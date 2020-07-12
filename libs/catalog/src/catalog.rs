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
use crate::{DrawerFileId, DrawerInterface, FileMetadata};
use failure::{ensure, Fallible};
use glob::{MatchOptions, Pattern};
use std::{borrow::Cow, collections::HashMap};

type DrawerId = u32;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct FileId {
    drawer_id: DrawerId,
    drawer_file_id: DrawerFileId,
}

// A catalog is a uniform, indexed interface to a collection of Drawers. This
// allows a game engine to expose several sources of data through a single interface.
// Common uses are allowing loose files when developing, while shipping compacted
// asset packs in production and combining data from multiple packs, e.g. when assets
// are shipped on multiple disks, or where assets may get extended and overridden
// with mod content.
pub struct Catalog {
    last_drawer: DrawerId,
    drawer_index: HashMap<(i64, String), DrawerId>,
    drawers: HashMap<DrawerId, Box<dyn DrawerInterface>>,
    index: HashMap<String, FileId>,
}

impl Catalog {
    pub fn empty() -> Self {
        Self {
            last_drawer: 0,
            drawer_index: HashMap::new(),
            drawers: HashMap::new(),
            index: HashMap::new(),
        }
    }

    pub fn with_drawers(mut drawers: Vec<Box<dyn DrawerInterface>>) -> Fallible<Self> {
        let mut catalog = Self::empty();
        for drawer in drawers.drain(..) {
            catalog.add_drawer(drawer)?;
        }
        Ok(catalog)
    }

    pub fn add_drawer(&mut self, drawer: Box<dyn DrawerInterface>) -> Fallible<()> {
        let next_priority = drawer.priority();
        let index = drawer.index()?;
        let drawer_key = (drawer.priority(), drawer.name().to_owned());
        ensure!(
            !self.drawer_index.contains_key(&drawer_key),
            "duplicate drawer added"
        );
        let drawer_id = self.last_drawer;
        self.last_drawer = self.last_drawer + 1;
        self.drawer_index.insert(drawer_key.to_owned(), drawer_id);
        self.drawers.insert(drawer_id, drawer);
        for (&drawer_file_id, name) in index.iter() {
            if self.index.contains_key(name) {
                let prior_drawer = self.index[name].drawer_id;
                let prior_priority = self.drawers[&prior_drawer].priority();
                // If there is already a higher priority entry, skip indexing the new version.
                if next_priority < prior_priority {
                    continue;
                }
            }
            self.index.insert(
                name.to_owned(),
                FileId {
                    drawer_id,
                    drawer_file_id,
                },
            );
        }
        Ok(())
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
            if pattern.matches_with(key, opts) {
                matching.push(key.to_owned());
            }
        }
        Ok(matching)
    }

    pub fn stat_name_sync(&self, name: &str) -> Fallible<FileMetadata> {
        ensure!(self.index.contains_key(name), "file not found");
        let fid = &self.index[name];
        let drawer_meta = self.drawers[&fid.drawer_id].stat_sync(fid.drawer_file_id)?;
        Ok(FileMetadata::from_drawer(*fid, drawer_meta))
    }

    pub fn read_name_sync(&self, name: &str) -> Fallible<Cow<[u8]>> {
        ensure!(self.index.contains_key(name), "file not found");
        let fid = &self.index[name];
        Ok(self.drawers[&fid.drawer_id].read_sync(fid.drawer_file_id)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DirectoryDrawer;
    use std::path::PathBuf;

    #[test]
    fn basic_functionality() -> Fallible<()> {
        let mut catalog =
            Catalog::with_drawers(vec![DirectoryDrawer::new("a", 0, "./test_data/a")?])?;

        // Expect success
        let meta = catalog.stat_name_sync("a.txt")?;
        assert_eq!(meta.name, "a.txt");
        assert_eq!(meta.path, Some(PathBuf::from("./test_data/a/a.txt")));
        let data = catalog.read_name_sync("a.txt")?;
        assert_eq!(data, "hello".as_bytes());

        // Missing file
        assert!(catalog.stat_name_sync("a_long_and_silly_name").is_err());
        // Present, but a directory.
        assert!(catalog.stat_name_sync("nested").is_err());

        // Add a second drawer with lower priority.
        catalog.add_drawer(DirectoryDrawer::new("b", -1, "./test_data/b")?)?;
        let meta = catalog.stat_name_sync("a.txt")?;
        assert_eq!(meta.name, "a.txt");
        assert_eq!(meta.path, Some(PathBuf::from("./test_data/a/a.txt")));
        let data = catalog.read_name_sync("a.txt")?;
        assert_eq!(data, "hello".as_bytes());

        // Add a third drawer with higher priority.
        catalog.add_drawer(DirectoryDrawer::new("b", 1, "./test_data/b")?)?;
        let meta = catalog.stat_name_sync("a.txt")?;
        assert_eq!(meta.name, "a.txt");
        assert_eq!(meta.path, Some(PathBuf::from("./test_data/b/a.txt")));
        let data = catalog.read_name_sync("a.txt")?;
        assert_eq!(data, "world".as_bytes());

        Ok(())
    }
}
