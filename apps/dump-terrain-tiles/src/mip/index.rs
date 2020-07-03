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
use crate::mip::tile::Tile;
use failure::Fallible;
use json::JsonValue;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

// The top level directory contains sub-folders for each dataset we have filtered for rendering.
// Each data set may be in spherical coordinates or cartesian polar coordinates.
// Each data set may be either a color data set or a height data set.
// Each data set has a collection of directories, one per mip level, named by resolution.
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

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum DataSetCoordinates {
    Spherical,
    CartesianPolar,
}

impl DataSetCoordinates {
    fn name(&self) -> String {
        match self {
            Self::Spherical => "spherical",
            Self::CartesianPolar => "cartesian_polar",
        }
        .to_owned()
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum DataSetDataKind {
    Color,
    Height,
}

impl DataSetDataKind {
    fn name(&self) -> String {
        match self {
            Self::Color => "color",
            Self::Height => "height",
        }
        .to_owned()
    }
}

pub struct IndexDataSet {
    name: String,
    path: PathBuf,
    kind: DataSetDataKind,
    coordinates: DataSetCoordinates,
    tiles: HashMap<i32, HashMap<(i32, i32), Arc<RwLock<Tile>>>>,
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
        })
    }

    pub fn add_tile(&mut self, resolution: i32, base: (i32, i32)) -> Arc<RwLock<Tile>> {
        let tile = Arc::new(RwLock::new(Tile::new(&self.path, resolution, base)));

        self.tiles
            .entry(resolution)
            .or_insert_with(|| HashMap::new())
            .insert(base, tile.clone());

        tile
    }

    pub fn as_json(&self) {
        let mut obj = JsonValue::new_object();

        for (resolution, tiles_in_level) in self.tiles.iter() {
            let mut tile_arr = JsonValue::new_array();
            for tile in tiles_in_level.values() {
                let mut tile_info = JsonValue::new_object();
            }

            obj.insert(&format!("{}", resolution), tile_arr);
        }

        ()
    }

    pub fn write(&self) -> Fallible<()> {
        Ok(())
    }
}
