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
use crate::{DrawerFileId, DrawerFileMetadata, DrawerInterface};
use failure::{ensure, Fallible};
use std::{
    collections::HashMap,
    ffi::OsStr,
    fs,
    path::PathBuf,
    sync::{Arc, RwLock},
};

pub struct DirectoryDrawer {
    name: String,
    priority: i64,
    path: PathBuf,
    index: HashMap<DrawerFileId, String>,
}

impl DirectoryDrawer {
    pub fn new<S: AsRef<OsStr> + ?Sized>(
        name: &str,
        priority: i64,
        path_name: &S,
    ) -> Fallible<Arc<RwLock<dyn DrawerInterface>>> {
        let path = PathBuf::from(path_name);
        let mut dd = Self {
            name: name.to_owned(),
            priority,
            path: path.clone(),
            index: HashMap::new(),
        };
        for (i, entry) in fs::read_dir(&path)?.enumerate() {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            if let Some(name) = entry.path().file_name() {
                dd.index.insert(
                    DrawerFileId::from_u32(i as u32),
                    name.to_string_lossy().to_string(),
                );
            }
        }
        Ok(Arc::new(RwLock::new(dd)))
    }
}

impl DrawerInterface for DirectoryDrawer {
    fn index(&self) -> Fallible<HashMap<String, DrawerFileId>> {
        let mut out = HashMap::new();
        for (fid, name) in self.index.iter() {
            out.insert(name.clone(), *fid);
        }
        Ok(out)
    }

    fn priority(&self) -> i64 {
        self.priority
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn stat(&self, id: DrawerFileId) -> Fallible<DrawerFileMetadata> {
        ensure!(self.index.contains_key(&id), "file not found");
        let mut global_path = self.path.clone();
        global_path.push(&self.index[&id]);
        let meta = fs::metadata(&global_path)?;
        Ok(DrawerFileMetadata {
            drawer_file_id: id,
            name: self.index[&id].clone(),
            compression: None,
            packed_size: meta.len(),
            unpacked_size: meta.len(),
            path: Some(global_path),
        })
    }
}
