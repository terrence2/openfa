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
use crate::{
    mip::tile::{ChildIndex, Tile, TILE_EXTENT},
    AS_PER_HEMI_LAT, AS_PER_HEMI_LON,
};
use failure::Fallible;
use json::JsonValue;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};
use terrain_geo::tile::{DataSetCoordinates, DataSetDataKind};

// The top level directory contains sub-folders for each dataset we have filtered for rendering.
// Each data set may be in spherical coordinates or cartesian polar coordinates.
// Each data set may be either a color data set or a height data set.
// Each data set has a collection of directories, one per mip level, named by scale.
pub struct Index {
    path: PathBuf,
    data_sets: HashMap<String, Arc<RwLock<IndexDataSet>>>,
}

impl Index {
    // Note: we do not try to discover data sets since they may be incomplete at this point.
    // Discovery of existing resources is left up to the builders.
    pub fn empty(path: &Path) -> Self {
        Self {
            path: path.to_owned(),
            data_sets: HashMap::new(),
        }
    }

    pub fn add_data_set(
        &mut self,
        name: &str,
        kind: DataSetDataKind,
        coordinates: DataSetCoordinates,
    ) -> Fallible<Arc<RwLock<IndexDataSet>>> {
        let mut path = self.path.clone();
        path.push(name);

        let ds = Arc::new(RwLock::new(IndexDataSet::new(
            name,
            &path,
            kind,
            coordinates,
        )?));
        self.data_sets.insert(name.to_owned(), ds.clone());
        Ok(ds)
    }
}

pub struct IndexDataSet {
    name: String,
    path: PathBuf,
    kind: DataSetDataKind,
    coordinates: DataSetCoordinates,
    tiles: HashMap<i32, HashMap<(i32, i32), Arc<RwLock<Tile>>>>,
    root: Option<Arc<RwLock<Tile>>>,
}

impl IndexDataSet {
    fn new(
        name: &str,
        path: &Path,
        kind: DataSetDataKind,
        coordinates: DataSetCoordinates,
    ) -> Fallible<Self> {
        if !path.exists() {
            fs::create_dir(path)?;
        }
        Ok(Self {
            name: name.to_owned(),
            path: path.to_owned(),
            kind,
            coordinates,
            tiles: HashMap::new(),
            root: None,
        })
    }

    pub fn add_tile(&mut self, scale: i32, base: (i32, i32)) -> Arc<RwLock<Tile>> {
        let tile = Arc::new(RwLock::new(Tile::new(&self.path, scale, base)));

        // Find the parent tile in the higher scale. Note the Tile docs for meaning of the index.
        let parent_scale = scale * 2;
        assert!(self.tiles.contains_key(&parent_scale) || self.tiles.is_empty());
        if !self.tiles.is_empty() {
            let align_lat = (base.0 + AS_PER_HEMI_LAT) % parent_scale == 0;
            let align_lon = (base.1 + AS_PER_HEMI_LON) % parent_scale == 0;
            let extent = scale * TILE_EXTENT;
            let (child_index, parent_corner) = match (align_lat, align_lon) {
                (true, true) => (ChildIndex::SouthWest, base),
                (true, false) => (ChildIndex::SouthCenter, (base.0, base.1 - extent)),
                (false, true) => (ChildIndex::CenterWest, (base.0 - extent, base.1)),
                (false, false) => (ChildIndex::Center, (base.0 - extent, base.1 - extent)),
            };
            let parent = self.tiles[&parent_scale][&parent_corner].clone();
            parent.write().unwrap().set_child(child_index, tile.clone());
        } else {
            self.root = Some(tile.clone());
        }

        // Keep a global record of all tiles we've created so we can make an index later.
        self.tiles
            .entry(scale)
            .or_insert_with(HashMap::new)
            .insert(base, tile.clone());

        tile
    }

    pub fn as_json(&self) -> Fallible<JsonValue> {
        let root = self
            .root
            .as_ref()
            .expect("a tree")
            .read()
            .unwrap()
            .as_json()?;

        let mut obj = JsonValue::new_object();
        obj.insert::<&str>("name", &self.name)?;
        obj.insert("root", root)?;
        obj.insert("kind", self.kind.name())?;
        obj.insert("coordinates", self.coordinates.name())?;

        Ok(obj)
    }

    pub fn write(&self) -> Fallible<()> {
        let mut filename = self.path.clone();
        filename.push("index.json");

        fs::write(filename, self.as_json()?.to_string())?;

        Ok(())
    }
}
