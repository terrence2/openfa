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
use std::{
    fs,
    fs::File,
    io::Write,
    path::{Path, PathBuf},
};
use zerocopy::AsBytes;

// The physical number of pixels in the tile.
pub const TILE_PHYSICAL_SIZE: usize = 512;

// Number of samples that the tile is wide and tall. This leaves a one pixel strip at each side
// for linear filtering to pull from when we use this source as a texture.
pub const TILE_SAMPLES: i32 = 510;

// Width and height of the tile coverage. Multiply with the tile resolution to get width or height
// in arcseconds.
pub const TILE_EXTENT: i32 = TILE_SAMPLES - 1;

pub struct Tile {
    // The location of the tile.
    path: PathBuf,

    // Number of arcseconds in a sample.
    resolution: i32,

    // The tile's bottom left corner.
    base: (i32, i32),

    // Samples. Low indices are more south. This is opposite from SRTM ordering.
    data: [[i16; TILE_PHYSICAL_SIZE]; TILE_PHYSICAL_SIZE],
}

impl Tile {
    pub fn new(dataset_path: &Path, resolution: i32, base: (i32, i32)) -> Self {
        let mut path = dataset_path.to_owned();
        path.push(&format!("R{}", resolution));
        path.push(&Self::filename_base(resolution, base));

        Self {
            path: path.to_owned(),
            resolution,
            base,
            data: [[0i16; TILE_PHYSICAL_SIZE]; TILE_PHYSICAL_SIZE],
        }
    }

    fn arcsecond_to_dms(mut arcsecs: i32) -> (i32, i32, i32) {
        let degrees = arcsecs / 3_600;
        arcsecs -= degrees * 3_600;
        let minutes = arcsecs / 60;
        arcsecs -= minutes * 60;
        (degrees, minutes, arcsecs)
    }

    pub fn filename_base(resolution: i32, base: (i32, i32)) -> String {
        let (mut lat, mut lon) = base;
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
        let (lat_d, lat_m, lat_s) = Self::arcsecond_to_dms(lat);
        let (lon_d, lon_m, lon_s) = Self::arcsecond_to_dms(lon);
        format!(
            "R{}-{}{:03}d{:02}m{:02}s-{}{:03}d{:02}m{:02}s",
            resolution, lat_hemi, lat_d, lat_m, lat_s, lon_hemi, lon_d, lon_m, lon_s
        )
    }

    pub fn find_sampled_extremes(&self) -> (i16, i16) {
        let mut lo = i16::MAX;
        let mut hi = i16::MIN;
        for row in self.data.iter() {
            for &v in row.iter() {
                if v > hi {
                    hi = v;
                }
                if v < lo {
                    lo = v;
                }
            }
        }
        (lo, hi)
    }

    // Set a sample, offset in samples from the base corner.
    pub fn set_sample(&mut self, lat_offset: i32, lon_offset: i32, sample: i16) {
        self.data[lat_offset as usize][lon_offset as usize] = sample;
    }

    pub fn save_equalized_png(&self, directory: &Path) {
        let mut path = directory.to_owned();
        path.push(Self::filename_base(self.resolution, self.base));

        let (_, high) = self.find_sampled_extremes();

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

    pub fn file_exists(&self) -> bool {
        self.path.exists()
    }

    pub fn write(&self) -> Fallible<()> {
        if !self.path.parent().expect("subdir").exists() {
            fs::create_dir(self.path.parent().expect("subdir"));
        }
        let mut fp = File::create(&self.path.with_extension("bin"))?;
        fp.write(self.data.as_bytes());
        Ok(())
    }
}
