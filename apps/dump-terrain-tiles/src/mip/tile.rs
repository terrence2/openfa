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
use std::path::Path;

// The physical number of pixels in the tile.
pub const TILE_PHYSICAL_SIZE: usize = 512;

// Number of samples that the tile is wide and tall. This leaves a one pixel strip at each side
// for linear filtering to pull from when we use this source as a texture.
pub const TILE_SAMPLES: i32 = 510;

// Width and height of the tile coverage. Multiply with the tile resolution to get width or height
// in arcseconds.
pub const TILE_EXTENT: i32 = TILE_SAMPLES - 1;

pub struct Tile {
    // Number of arcseconds in a sample.
    resolution: i32,

    // The tile's bottom left corner.
    base: (i32, i32),

    // Samples. Low indices are more south. This is opposite from SRTM ordering.
    data: [[i16; TILE_PHYSICAL_SIZE]; TILE_PHYSICAL_SIZE],
}

impl Tile {
    pub fn new(resolution: i32, base: (i32, i32)) -> Self {
        Self {
            resolution,
            base,
            data: [[0i16; TILE_PHYSICAL_SIZE]; TILE_PHYSICAL_SIZE],
        }
    }

    // Set a sample, offset in samples from the base corner.
    pub fn set_sample(&mut self, lat_offset: i32, lon_offset: i32, sample: i16) {
        self.data[lat_offset as usize][lon_offset as usize] = sample;
    }

    pub fn save_equalized_png(&self, directory: &Path) {
        let mut path = directory.to_owned();
        path.push(self.file_name());

        let mut high = i16::MIN;
        for row in self.data.iter() {
            for &v in row.iter() {
                if v > high {
                    high = v;
                }
            }
        }
        use image::{ImageBuffer, Luma};
        let mut pic: image::ImageBuffer<image::Luma<u8>, Vec<u8>> =
            image::ImageBuffer::new(512, 512);
        for (y, row) in self.data.iter().enumerate() {
            for (x, &v) in row.iter().enumerate() {
                // Scale 0..high into 0..255
                let p = v.max(0) as f32;
                let pf = p / (high as f32) * 255f32;
                pic.put_pixel(
                    x as u32,
                    (TILE_PHYSICAL_SIZE - y - 1) as u32,
                    Luma([pf as u8]),
                );
            }
        }
        pic.save(path.with_extension("png"));
    }

    pub fn file_name(&self) -> String {
        let (mut lat, mut lon) = self.base;
        let lat_hemi = if lat >= 0 {
            "N"
        } else {
            lat = -lat;
            "S"
        };
        let lon_hemi = if lon >= 0 {
            "E"
        } else {
            lon = -lon;
            "W"
        };
        format!("{}{:07}{}{:07}.mpt", lon_hemi, lon, lat_hemi, lat)
    }
}
