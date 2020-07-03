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
    mip::{
        DataSetCoordinates, DataSetDataKind, MipIndex, MipTile, TILE_EXTENT, TILE_PHYSICAL_SIZE,
        TILE_SAMPLES,
    },
    srtm::SrtmIndex,
};
use absolute_unit::{arcseconds, degrees, meters};
use failure::Fallible;
use geodesy::{GeoCenter, Graticule};
use std::{
    borrow::BorrowMut,
    io::{stdout, Write},
    path::PathBuf,
};
use structopt::StructOpt;

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
    let as_per_hemi_lon = 180 * 60 * 60;
    let as_per_hemi_lat = 60 * 60 * 60;

    for &resolution in &[512] {
        let tile_extent_as = TILE_EXTENT * resolution;
        let tiles_per_hemi_lon = ((as_per_hemi_lon as f64) / (tile_extent_as as f64)).ceil() as i32;
        let tiles_per_hemi_lat = ((as_per_hemi_lat as f64) / (tile_extent_as as f64)).ceil() as i32;

        let lon_tile_indices = (-tiles_per_hemi_lon..=-1).chain(0..tiles_per_hemi_lon);
        let lat_tile_indices = (-tiles_per_hemi_lat..=-1).chain(0..tiles_per_hemi_lat);

        let mut lon_as = 0;
        for lat_tile_offset in lat_tile_indices {
            let lat_as = lat_tile_offset * tile_extent_as;

            for lon_tile_offset in lon_tile_indices.clone() {
                let lon_as = lon_tile_offset * tile_extent_as;

                // Note that the base of the tile might extend past the data area, so we need to
                // manually clamp and wrap each individual position back into a reasonable spot.
                let base = Graticule::<GeoCenter>::new(
                    arcseconds!(lat_as),
                    arcseconds!(lon_as),
                    meters!(0),
                );
                println!("at tile: {} {} => {}", lat_as, lon_as, base);

                // Fill in a tile.
                let mut tile = mip_srtm
                    .borrow_mut()
                    .write()
                    .unwrap()
                    .add_tile(resolution, (lat_as, lon_as));
                let mut td = tile.borrow_mut().write().unwrap();
                for lat_i in -1..TILE_SAMPLES + 1 {
                    if lat_i % 8 == 0 {
                        print!(".");
                        stdout().flush()?;
                    }

                    let lat_actual = lat_as + lat_i * resolution;
                    let lat_position = lat_actual.max(-60 * 60 * 60).min(60 * 60 * 60);

                    for lon_i in -1..TILE_SAMPLES + 1 {
                        let lon_actual = lon_as + lon_i * resolution;
                        let lon_position = if lon_actual < -as_per_hemi_lon {
                            as_per_hemi_lon - (-lon_actual - as_per_hemi_lon)
                        } else if lon_actual > as_per_hemi_lon {
                            -as_per_hemi_lon + (lon_actual - as_per_hemi_lon)
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
                td.save_equalized_png(std::path::Path::new("scanout"));
                td.write()?;
                println!();
            }
        }
    }

    Ok(())
}
