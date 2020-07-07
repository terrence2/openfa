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
use crate::srtm::tile::Tile;
use failure::Fallible;
use geodesy::{GeoCenter, Graticule};
use std::{
    collections::HashMap,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

pub struct Index {
    tiles: Vec<Tile>,
    // by latitude, then longitude
    by_graticule: HashMap<i16, HashMap<i16, usize>>,
}

impl Index {
    pub fn from_directory(directory: &Path) -> Fallible<Self> {
        let mut index_filename = PathBuf::from(directory);
        index_filename.push("srtm30m_bounding_boxes.json");

        let mut index_file = File::open(index_filename.as_path())?;
        let mut index_content = String::new();
        index_file.read_to_string(&mut index_content)?;

        let index_json = json::parse(&index_content)?;
        assert_eq!(index_json["type"], "FeatureCollection");
        let features = &index_json["features"];
        let mut tiles = Vec::new();
        for feature in features.members() {
            let tile = Tile::from_feature(&feature, directory)?;
            tiles.push(tile);
        }

        let mut by_graticule = HashMap::new();
        for (i, tile) in tiles.iter().enumerate() {
            let lon = tile.longitude();
            by_graticule
                .entry(tile.latitude())
                .or_insert_with(HashMap::new)
                .insert(lon, i);
        }

        println!("loaded: {} tiles", tiles.len());
        Ok(Self {
            tiles,
            by_graticule,
        })
    }

    #[allow(unused)]
    pub fn sample_linear(&self, grat: &Graticule<GeoCenter>) -> f32 {
        let lat = Tile::index(grat.lat());
        let lon = Tile::index(grat.lon());
        if let Some(row) = self.by_graticule.get(&lat) {
            if let Some(&tile_id) = row.get(&lon) {
                return self.tiles[tile_id].sample_linear(grat);
            }
        }
        0f32
    }

    #[allow(unused)]
    pub fn sample_nearest(&self, grat: &Graticule<GeoCenter>) -> i16 {
        let lat = Tile::index(grat.lat());
        let lon = Tile::index(grat.lon());
        if let Some(row) = self.by_graticule.get(&lat) {
            if let Some(&tile_id) = row.get(&lon) {
                // use absolute_unit::Degrees;
                // println!(
                //     "ISN: {}x{} => {}x{} => {}x{} => {} => {}",
                //     grat.lat::<Degrees>(),
                //     grat.lon::<Degrees>(),
                //     lat,
                //     lon,
                //     self.tiles[tile_id].latitude(),
                //     self.tiles[tile_id].longitude(),
                //     tile_id,
                //     self.tiles[tile_id].sample_nearest(grat)
                // );
                return self.tiles[tile_id].sample_nearest(grat);
            }
        }
        0
    }
}
