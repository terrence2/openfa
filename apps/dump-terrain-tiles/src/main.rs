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
    io::Read,
    path::{Path, PathBuf},
};
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "dump-terrain-tiles",
    about = "Slice various data sets into the formats we need."
)]
struct Opt {
    /// Slice srtm into tiles
    #[structopt(short = "s", long)]
    srtm_directory: PathBuf,
}

struct SrtmTile {
    data: Mmap,
    corners: [(f64, f64); 4],
}

impl SrtmTile {
    fn from_feature(feature: &JsonValue, base_path: &Path) -> Fallible<Self> {
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
        let mut corners = [(0f64, 0f64); 4];
        corners.copy_from_slice(&all_corners[0..4]);

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

        println!("path: {:?}", path);
        let file = File::open(&path)?;
        let data = unsafe { MmapOptions::new().map(&file)? };

        Ok(Self { data, corners })
    }
}

fn main() -> Fallible<()> {
    let opt = Opt::from_args();

    let mut index_filename = PathBuf::from(&opt.srtm_directory);
    index_filename.push("srtm30m_bounding_boxes.json");
    let mut index_file = File::open(index_filename.as_path())?;
    let mut index_content = String::new();
    index_file.read_to_string(&mut index_content)?;
    let index_json = json::parse(&index_content)?;
    assert_eq!(index_json["type"], "FeatureCollection");
    let features = &index_json["features"];
    let mut tiles = Vec::new();
    for feature in features.members() {
        let tile = SrtmTile::from_feature(&feature, &opt.srtm_directory)?;
        tiles.push(tile);
    }

    Ok(())
}
