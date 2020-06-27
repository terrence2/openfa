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
use json::JsonValue;
use memmap::{Mmap, MmapOptions};
use std::{
    fs::File,
    path::{Path, PathBuf},
};

pub struct Tile {
    data: Mmap,
    real_corners: [(f64, f64); 4],
    latitude: i16,
    longitude: i16,
}

impl Tile {
    pub fn from_feature(feature: &JsonValue, base_path: &Path) -> Fallible<Self> {
        assert_eq!(feature["type"], "Feature");
        let geometry = &feature["geometry"];
        assert_eq!(geometry["type"], "Polygon");
        let mut all_corners = [(0f64, 0f64); 5];
        for coordinates in geometry["coordinates"].members() {
            for (i, corner) in coordinates.members().enumerate() {
                let mut m = corner.members();
                all_corners[i].0 = m.next().unwrap().as_f64().unwrap();
                all_corners[i].1 = m.next().unwrap().as_f64().unwrap();
            }
        }
        assert_eq!(all_corners[0], all_corners[4]);
        let mut real_corners = [(0f64, 0f64); 4];
        real_corners.copy_from_slice(&all_corners[0..4]);

        let latitude = real_corners[0].1.round() as i16;
        let longitude = real_corners[0].0.round() as i16;

        let tile_name = &feature["properties"]["dataFile"];
        let tile_zip_filename = PathBuf::from(tile_name.as_str().unwrap());
        let tile_filename = PathBuf::from(
            PathBuf::from(
                PathBuf::from(tile_zip_filename.file_stem().unwrap())
                    .file_stem()
                    .unwrap(),
            )
            .file_stem()
            .unwrap(),
        )
        .with_extension("hgt");
        let mut path = PathBuf::from(base_path);
        path.push("tiles_unpacked");
        path.push(&tile_filename);

        // println!("path: {:?}: {}x{}", path, latitude, longitude);
        let file = File::open(&path)?;
        let data = unsafe { MmapOptions::new().map(&file)? };

        Ok(Self {
            data,
            real_corners,
            latitude,
            longitude,
        })
    }

    pub fn latitude(&self) -> i16 {
        self.latitude
    }

    pub fn longitude(&self) -> i16 {
        self.longitude
    }
}
