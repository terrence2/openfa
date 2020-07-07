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
use image::{ImageBuffer, Luma};
use json::JsonValue;
use std::{
    fs,
    fs::File,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};
use zerocopy::AsBytes;

// The physical number of pixels in the tile.
pub const TILE_PHYSICAL_SIZE: usize = 512;

// Number of samples that the tile is wide and tall. This leaves a one pixel strip at each side
// for linear filtering to pull from when we use this source as a texture.
pub const TILE_SAMPLES: i32 = 510;

// Width and height of the tile coverage. Multiply with the tile scale to get width or height
// in arcseconds.
pub const TILE_EXTENT: i32 = TILE_SAMPLES - 1;

pub enum ChildIndex {
    SouthWest,
    SouthCenter,
    CenterWest,
    Center,
}

impl ChildIndex {
    pub fn to_index(&self) -> usize {
        match self {
            Self::SouthWest => 0,
            Self::SouthCenter => 1,
            Self::CenterWest => 2,
            Self::Center => 3,
        }
    }

    pub fn from_index(index: usize) -> Self {
        match index {
            0 => Self::SouthWest,
            1 => Self::SouthCenter,
            2 => Self::CenterWest,
            3 => Self::Center,
            _ => panic!("not a valid index"),
        }
    }

    pub fn key_name(&self) -> String {
        match self {
            Self::SouthWest => "south_west",
            Self::SouthCenter => "south_center",
            Self::CenterWest => "center_west",
            Self::Center => "center",
        }
        .to_owned()
    }
}

pub struct Tile {
    // The location of the tile.
    path: PathBuf,

    // Number of arcseconds in a sample.
    scale: i32,

    // The tile's bottom left corner.
    base: (i32, i32),

    // Samples. Low indices are more south. This is opposite from SRTM ordering.
    data: [[i16; TILE_PHYSICAL_SIZE]; TILE_PHYSICAL_SIZE],

    // Keep a quad-tree of children. Indices as per ChildIndex.
    children: [Option<Arc<RwLock<Tile>>>; 4],
}

impl Tile {
    pub fn new(dataset_path: &Path, scale: i32, base: (i32, i32)) -> Self {
        let mut path = dataset_path.to_owned();
        path.push(&format!("scale-{}", scale));
        path.push(&Self::filename_base(base));

        Self {
            path,
            scale,
            base,
            data: [[0i16; TILE_PHYSICAL_SIZE]; TILE_PHYSICAL_SIZE],
            children: [None, None, None, None],
        }
    }

    pub fn set_child(&mut self, index: ChildIndex, child: Arc<RwLock<Tile>>) {
        self.children[index.to_index()] = Some(child);
    }

    fn arcsecond_to_dms(mut arcsecs: i32) -> (i32, i32, i32) {
        let degrees = arcsecs / 3_600;
        arcsecs -= degrees * 3_600;
        let minutes = arcsecs / 60;
        arcsecs -= minutes * 60;
        (degrees, minutes, arcsecs)
    }

    pub fn filename_base(base: (i32, i32)) -> String {
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
            "{}{:03}d{:02}m{:02}s-{}{:03}d{:02}m{:02}s",
            lat_hemi, lat_d, lat_m, lat_s, lon_hemi, lon_d, lon_m, lon_s
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

    pub fn save_equalized_png(&self, directory: &Path) -> Fallible<()> {
        let mut path = directory.to_owned();
        path.push(format!("R{}-", self.scale) + &Self::filename_base(self.base));

        let (_, high) = self.find_sampled_extremes();

        let mut pic: ImageBuffer<Luma<u8>, Vec<u8>> = ImageBuffer::new(512, 512);
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
        pic.save(path.with_extension("png"))?;
        Ok(())
    }

    #[allow(unused)]
    pub fn file_exists(&self) -> bool {
        self.path.exists()
    }

    pub fn write(&self) -> Fallible<()> {
        if !self.path.parent().expect("subdir").exists() {
            fs::create_dir(self.path.parent().expect("subdir"))?;
        }
        let mut fp = File::create(&self.path.with_extension("bin"))?;
        fp.write_all(self.data.as_bytes())?;
        Ok(())
    }

    pub fn as_json(&self) -> Fallible<JsonValue> {
        let mut children = JsonValue::new_object();
        for (i, maybe_child) in self.children.iter().enumerate() {
            if let Some(child) = maybe_child {
                let key = ChildIndex::from_index(i).key_name();
                children.insert(&key, child.read().unwrap().as_json()?)?;
            }
        }

        let mut base = JsonValue::new_object();
        base.insert("latitude_arcseconds", self.base.0)?;
        base.insert("longitude_arcseconds", self.base.1)?;

        let mut obj = JsonValue::new_object();
        obj.insert("children", children)?;
        obj.insert("base", base)?;
        obj.insert("scale", self.scale)?;
        let rel = self
            .path
            .strip_prefix(self.path.parent().unwrap().parent().unwrap())?
            .with_extension("bin")
            .to_string_lossy()
            .to_string();
        obj.insert::<&str>("path", &rel)?;

        Ok(obj)
    }
}
