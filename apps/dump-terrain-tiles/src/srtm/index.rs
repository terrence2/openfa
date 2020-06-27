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
use std::{
    collections::HashMap,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

pub struct Index {
    tiles: Vec<Tile>,
    // from latitude to list of tiles at latitude, sorted by longitude.
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
                .or_insert_with(|| HashMap::new())
                .insert(lon, i);
        }

        println!("loaded: {} tiles", tiles.len());
        Ok(Self {
            tiles,
            by_graticule,
        })
    }
}
