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
use crate::{DrawerFileMetadata, FileId};
use std::path::PathBuf;

// Information about a "file" in the catalog.
pub struct FileMetadata {
    pub id: FileId,
    pub name: String,
    pub compression: Option<&'static str>,
    pub packed_size: u64,
    pub unpacked_size: u64,
    pub path: Option<PathBuf>,
}

impl FileMetadata {
    pub(crate) fn from_drawer(id: FileId, drawer_meta: DrawerFileMetadata) -> FileMetadata {
        Self {
            id,
            name: drawer_meta.name,
            compression: drawer_meta.compression,
            packed_size: drawer_meta.packed_size,
            unpacked_size: drawer_meta.unpacked_size,
            path: drawer_meta.path,
        }
    }
}
