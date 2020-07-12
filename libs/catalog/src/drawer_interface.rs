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
use failure::Fallible;
use std::{borrow::Cow, collections::HashMap, path::PathBuf};

// Files are identified by an id internally.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct DrawerFileId(u32);

impl DrawerFileId {
    pub fn from_u32(i: u32) -> Self {
        DrawerFileId(i)
    }

    pub fn raw(&self) -> u32 {
        self.0
    }
}

pub struct DrawerFileMetadata {
    pub drawer_file_id: DrawerFileId,
    pub name: String,
    pub compression: Option<&'static str>,
    pub packed_size: u64,
    pub unpacked_size: u64,
    pub path: Option<PathBuf>,
}

// A drawer is one related section of a catalog. It is a uniform interface for a group of files.
// A game can implement this trait to expose their file grouping as part of a Catalog.
pub trait DrawerInterface {
    // Index on a drawer lets us build an index over the entire catalog. This must return
    // every name that can be loaded from the drawer, even if it is not yet loadable. After
    // this method is called, the catalog will never reference the returned names again, in
    // preference of the associated FileId returned here.
    fn index(&self) -> Fallible<HashMap<DrawerFileId, String>>;

    // Must return the priority of a drawer. Files from drawers with higher priority will be
    // loaded from by name before drawers with lower priority. Clients can still list every
    // DrawerFileId associated with a name, however, and load items without masking if needed.
    fn priority(&self) -> i64;

    // Must return the name of the drawer. If there is a duplicate priority, the name will
    // be used instead. If the names are also the same, then an error is thrown, blocking
    // further progress.
    fn name(&self) -> &str;

    // Stat must fill out the stat struct for the given file.
    fn stat_sync(&self, id: DrawerFileId) -> Fallible<DrawerFileMetadata>;

    // Provide the content of the given file.
    fn read_sync(&self, id: DrawerFileId) -> Fallible<Cow<[u8]>>;
}
