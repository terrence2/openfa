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
mod mip;
mod srtm;

use crate::{
    mip::{MipTile, TILE_EXTENT, TILE_PHYSICAL_SIZE, TILE_SAMPLES},
    srtm::SrtmIndex,
};
use absolute_unit::{arcseconds, degrees, meters};
use failure::Fallible;
use geodesy::{GeoCenter, Graticule};
use std::path::PathBuf;
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

fn main() -> Fallible<()> {
    let opt = Opt::from_args();

    let index = SrtmIndex::from_directory(&opt.srtm_directory)?;
    // let grat = Graticule::<GeoCenter>::new(degrees!(10.5), degrees!(0.5), meters!(0));
    // let height = index.sample(&grat);
    // let grat = Graticule::<GeoCenter>::new(degrees!(-0.5), degrees!(0.5), meters!(0));
    // let height = index.sample(&grat);

    // Sampling strategy:
    //   1) Samples must fall on integer arc-seconds so that we can use sample_nearest.
    //   2) Use gaussian kernel over sample area or re-import pre-sampled data somehow?
    // Variables:
    //   Tile size: 512 or 1024? I think 512 still since bandwidth is still an issue.
    // Resolutions:
    //  resolution  | ~tiles   | tiles/hemi
    //  ------------|----------|------------
    //   1"         | 700,000  | 648,000
    //   2"         | 180,000  | 324,000
    //   4"         |  44,800  | 162,000
    //   8"         |  11,200  |  81,000
    //  16"         |   2,800  |  40,500
    //  32"         |     700  |  20,250
    //  64" | 1'4"  |     175  |  10,125
    // 128" | 2'8"  |
    // 256" | 4'16" |
    let resolution = 64;
    let tile_extent_as = TILE_EXTENT * resolution;
    let as_per_hemi_lon = 180 * 60 * 60;
    let as_per_hemi_lat = 60 * 60 * 60;
    let tiles_per_hemi_lon = ((as_per_hemi_lon as f64) / (tile_extent_as as f64)).ceil() as i32;
    let tiles_per_hemi_lat = ((as_per_hemi_lat as f64) / (tile_extent_as as f64)).ceil() as i32;

    let lon_tile_indices = (-tiles_per_hemi_lon..=-1).chain(0..tiles_per_hemi_lon);
    let lat_tile_indices = (-tiles_per_hemi_lat..=-1).chain(0..tiles_per_hemi_lat);

    // Positive quadrant
    let mut lon_as = 0;
    for lon_tile_offset in lon_tile_indices {
        let lon_as = lon_tile_offset * tile_extent_as;
        for lat_tile_offset in lat_tile_indices.clone() {
            let lat_as = lat_tile_offset * tile_extent_as;
            let base =
                Graticule::<GeoCenter>::new(arcseconds!(lat_as), arcseconds!(lon_as), meters!(0));
            println!("at tile: {} {} => {}", lat_as, lon_as, base);

            // Fill in a tile.
            let mut tile = MipTile::new(resolution, (lat_as, lon_as));
            for lat_i in -1..TILE_SAMPLES + 1 {
                for lon_i in -1..TILE_SAMPLES + 1 {
                    let position = Graticule::<GeoCenter>::new(
                        arcseconds!(lat_as + lat_i * resolution),
                        arcseconds!(lon_as + lon_i * resolution),
                        meters!(0),
                    );
                    // FIXME: sample regions
                    let height = index.sample_nearest(&position);
                    tile.set_sample(lat_i + 1, lon_i + 1, height);
                }
            }
            tile.save_equalized_png(std::path::Path::new("scanout"));
        }
    }

    /*
    // Pos lat, neg lon.
    let mut tile_data: image::ImageBuffer<image::Luma<u16>, Vec<u16>> =
        image::ImageBuffer::new(512, 512);
    let mut lon_as = 0;
    for lon_tile_offset in 1..=tiles_per_hemi_lon {
        let lon_as = -lon_tile_offset * tile_extent_as;
        for lat_tile_offset in 0..tiles_per_hemi_lat {
            let lat_as = lat_tile_offset * tile_extent_as;
            let base =
                Graticule::<GeoCenter>::new(arcseconds!(lat_as), arcseconds!(lon_as), meters!(0));
            println!("at tile: {} {} => {}", lat_as, lon_as, base);

            // Fill in a tile.
            for lat_i in -1..TILE_SAMPLES + 1 {
                for lon_i in -1..TILE_SAMPLES + 1 {
                    let position = Graticule::<GeoCenter>::new(
                        arcseconds!(lat_as + lat_i * resolution),
                        arcseconds!(lon_as + lon_i * resolution),
                        meters!(0),
                    );
                    let height = index.sample_nearest(&position);
                    tile_data.put_pixel(
                        (lon_i + 1) as u32, // lon is left to right
                        512 - (lat_i + 1) as u32 - 1,
                        image::Luma([height.max(0) as u16]),
                    );
                }
            }

            break;
        }
        break;
    }
    tile_data.save("test1.png");
     */

    /*
    let as_per_hemi_lon = 180 * 60 * 60;
    let as_per_hemi_lat = 120 * 60 * 60;
    let tile_pixel_as = 64;
    let tile_as = 510 * tile_pixel_as;
    let tiles_per_hemi_lon = ((as_per_hemi_lon as f64) / (tile_as as f64)).ceil() as i32;
    let tiles_per_hemi_lat = ((as_per_hemi_lat as f64) / (tile_as as f64)).ceil() as i32;
    println!(
        "tiles per hemi: {} x {}",
        tiles_per_hemi_lat, tiles_per_hemi_lon
    );
    assert_eq!(tiles_per_hemi_lon * tile_as, as_per_hemi_lon);
    assert_eq!(tiles_per_hemi_lat * tile_as, as_per_hemi_lat);
    let i = -tiles_per_hemi_lon;
    //for i in -tiles_per_hemi_lon..tiles_per_hemi_lon {
    for j in -tiles_per_hemi_lat..tiles_per_hemi_lat {
        let base = Graticule::<GeoCenter>::new(
            arcseconds!(j * tile_as),
            arcseconds!(i * tile_as),
            meters!(0),
        );
        println!("{}x{} => {}", i, j, base);
    }
    //}
     */

    Ok(())
}
