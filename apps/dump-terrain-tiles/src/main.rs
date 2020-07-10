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
    mip::{MipIndex, TILE_EXTENT, TILE_SAMPLES},
    srtm::SrtmIndex,
};
use absolute_unit::{arcseconds, meters};
use failure::Fallible;
use geodesy::{GeoCenter, Graticule};
use std::{
    borrow::BorrowMut,
    io::{stdout, Write},
    path::PathBuf,
};
use structopt::StructOpt;
use terrain_geo::tile::{DataSetCoordinates, DataSetDataKind};

#[derive(Debug, StructOpt)]
#[structopt(
    name = "dump-terrain-tiles",
    about = "Slice various data sets into the formats we need."
)]
struct Opt {
    /// Slice srtm into tiles
    #[structopt(short, long)]
    srtm_directory: PathBuf,

    /// The directory to save work to.
    #[structopt(short, long)]
    output_directory: PathBuf,
}

pub const AS_PER_SPHERE_LON: i32 = 360 * 60 * 60;
pub const AS_PER_SPHERE_LAT: i32 = 120 * 60 * 60;
pub const AS_PER_HEMI_LON: i32 = AS_PER_SPHERE_LON / 2;
pub const AS_PER_HEMI_LAT: i32 = AS_PER_SPHERE_LAT / 2;

fn main() -> Fallible<()> {
    let opt = Opt::from_args();

    let index = SrtmIndex::from_directory(&opt.srtm_directory)?;

    let mut mip_index = MipIndex::empty(&opt.output_directory);

    let mut mip_srtm = mip_index.add_data_set(
        "srtm",
        DataSetDataKind::Height,
        DataSetCoordinates::Spherical,
    )?;

    // Sampling strategy:
    //   1) Samples must fall on integer arc-seconds so that we can use sample_nearest.
    //   2) Use gaussian kernel over sample area or re-import pre-sampled data somehow?
    // Variables:
    //   Tile size: 512 or 1024? I think 512 still since bandwidth is still an issue.
    // Resolutions:
    //  scale       | ~tiles   | tiles/hemi
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

    for &scale in &[4096, 2048, 1024, 512] {
        let tile_extent_as = TILE_EXTENT * scale;
        let tiles_per_sphere_lon =
            ((AS_PER_SPHERE_LON as f64) / (tile_extent_as as f64)).ceil() as i32;
        let tiles_per_sphere_lat =
            ((AS_PER_SPHERE_LAT as f64) / (tile_extent_as as f64)).ceil() as i32;

        for lat_tile_offset in 0..tiles_per_sphere_lat {
            let lat_as = (lat_tile_offset * tile_extent_as) - AS_PER_HEMI_LAT;

            for lon_tile_offset in 0..tiles_per_sphere_lon {
                let lon_as = (lon_tile_offset * tile_extent_as) - AS_PER_HEMI_LON;

                // Note that the base of the tile might extend past the data area, so we need to
                // manually clamp and wrap each individual position back into a reasonable spot.
                let base = Graticule::<GeoCenter>::new(
                    arcseconds!(lat_as),
                    arcseconds!(lon_as),
                    meters!(0),
                );
                println!(
                    "building: scale {} [{} of {}] @ {}",
                    scale,
                    lat_tile_offset * tiles_per_sphere_lon + lon_tile_offset + 1,
                    tiles_per_sphere_lat * tiles_per_sphere_lon,
                    base
                );

                // Fill in a tile.
                let mut tile = mip_srtm
                    .borrow_mut()
                    .write()
                    .unwrap()
                    .add_tile(scale, (lat_as, lon_as));
                let mut td = tile.borrow_mut().write().unwrap();
                for lat_i in -1..TILE_SAMPLES + 1 {
                    if lat_i % 8 == 0 {
                        print!(".");
                        stdout().flush()?;
                    }

                    let lat_actual = lat_as + lat_i * scale;
                    let lat_position = lat_actual.max(-60 * 60 * 60).min(60 * 60 * 60);

                    for lon_i in -1..TILE_SAMPLES + 1 {
                        let lon_actual = lon_as + lon_i * scale;
                        let lon_position = if lon_actual < -AS_PER_HEMI_LON {
                            AS_PER_HEMI_LON - (-lon_actual - AS_PER_HEMI_LON)
                        } else if lon_actual > AS_PER_HEMI_LON {
                            -AS_PER_HEMI_LON + (lon_actual - AS_PER_HEMI_LON)
                        } else {
                            lon_actual
                        };

                        let position = Graticule::<GeoCenter>::new(
                            arcseconds!(lat_position),
                            arcseconds!(lon_position),
                            meters!(0),
                        );
                        // FIXME: sample regions
                        let height = index.sample_nearest(&position);
                        td.set_sample(lat_i + 1, lon_i + 1, height);
                    }
                }
                td.save_equalized_png(std::path::Path::new("scanout"))?;
                td.write()?;
                println!();
            }
        }
    }
    mip_srtm.write().unwrap().write()?;
    Ok(())
}
