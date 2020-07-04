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

// Data Sets:
//   NASA's Shuttle Radar Topography Map (SRTM); height data
//
// Desired Data Sets:
//   NASA's Blue Marble Next Generation (BMNG); diffuse color information
//   JAXA's Advanced Land Observing Satellite "DAICHI" (ALOS); height data
//   Something cartesian polar north and south
//
// Each data set gets a directory.
// That directory has an index.json with all of the tile metadata in it.
//   Data sets may have spherical or cartesian-polar coordinates.
//   Data sets may contain height or color data.
// That directory contains one subdir for every available resolution.
// All units for spherical data-sets are in arcseconds, including resolution above.
// Tiles are 512x512 with a one pixel overlap with other tiles to enable linear filtering. Data is
//   stored row-major with low indexed rows to the south, going north and low index.
//
// Tile cache design:
//   Upload one (or more) mega-texture(s) for each dataset.
//   The index is a fixed, large texture:
//     * SRTM has 1' resolution, but tiles have at minimum 510' of content.
//     * We need a (360|180 * 60 * 60 / 510) pixels wide|high texture => 2541.17 x 1270.59
//     * 2560 * 1280 px index texture.
//     * Open Question: do we have data sets with higher resolution that we want to support? Will
//       those inherently load in larger blocks to support the above index scheme?
//     * Open Question: one index per dataset or shared globally and we assume the same resolution
//       choice for all datasets?
//   Tile Updates:
//     * The patch tree "votes" on what resolution it wants.
//     * We select a handful of the most needed that are not present to upload and create copy ops.
//     * We update the index texture with a compute shader that overwrites if the scale is smaller.

// First pass: hard code everything.
use failure::Fallible;
use gpu::GPU;
use std::{fs, fs::File, io::Read, path::PathBuf};

const TILE_SIZE: u32 = 512;
const INDEX_WIDTH: u32 = 2560;
const INDEX_HEIGHT: u32 = 1280;

pub struct TileManager;

impl TileManager {
    pub fn new(gpu: &GPU) -> Fallible<Self> {
        let index_extent = wgpu::Extent3d {
            width: INDEX_WIDTH as u32,
            height: INDEX_HEIGHT as u32,
            depth: 1,
        };
        /*
        let index_texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("terrain-geo-tile-index-texture"),
            size: extent,
            array_layer_count: self.images.len() as u32,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsage::all(),
        });
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor {
            format: wgpu::TextureFormat::Rgba8Unorm,
            dimension: wgpu::TextureViewDimension::D2Array,
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            array_layer_count: self.images.len() as u32,
        });
         */

        let base_path = PathBuf::from("/home/terrence/storage/srtm/output/srtm/R512");
        let tiles = [
            "R512-N000d00m00s-E000d00m00s.bin",
            "R512-N000d00m00s-W144d46m56s.bin",
            "R512-S072d23m28s-E144d46m56s.bin",
            "R512-N000d00m00s-E072d23m28s.bin",
            "R512-N000d00m00s-W217d10m24s.bin",
            "R512-S072d23m28s-W072d23m28s.bin",
            "R512-N000d00m00s-E144d46m56s.bin",
            "R512-S072d23m28s-E000d00m00s.bin",
            "R512-S072d23m28s-W144d46m56s.bin",
            "R512-N000d00m00s-W072d23m28s.bin",
            "R512-S072d23m28s-E072d23m28s.bin",
            "R512-S072d23m28s-W217d10m24s.bin",
        ];
        for tile in &tiles {
            let mut path = base_path.clone();
            path.push(tile);
            let mut fp = File::open(&path)?;
            let mut data = [0u8; 2 * 512 * 512];
            fp.read_exact(&mut data);

            // Upload as texture.
        }

        Ok(Self)
    }
}
