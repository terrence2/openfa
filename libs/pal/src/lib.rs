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
#[macro_use] extern crate failure;
extern crate image;

use failure::Error;
use image::{Pixel, Rgb, Rgba};

pub struct Palette {
    pub color_count: usize,
    entries: Vec<Rgba<u8>>,
}

impl Palette {
    pub fn from_bytes(data: &[u8]) -> Result<Palette, Error> {
        ensure!(data.len() % 3 == 0, "expected data to divide cleanly by 3");
        let mut entries = Vec::new();
        let color_count = data.len() / 3;
        for i in 0..color_count {
            entries.push(Rgba { data: [
                data[i * 3 + 0] * 3,
                data[i * 3 + 1] * 3,
                data[i * 3 + 2] * 3,
                255
            ] });
        }
        return Ok(Palette {
            color_count,
            entries
        });
    }

    pub fn rgba(&self, index: usize) -> Result<Rgba<u8>, Error> {
        ensure!(index < self.entries.len(), "index outside of palette");
        return Ok(self.entries[index]);
    }

    pub fn rgb(&self, index: usize) -> Result<Rgb<u8>, Error> {
        ensure!(index < self.entries.len(), "index outside of palette");
        return Ok(self.entries[index].to_rgb());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::prelude::*;

    #[test]
    fn it_works_with_normal_palette() {
        let mut fp = fs::File::open("test_data/PALETTE.PAL").unwrap();
        let mut data = Vec::new();
        fp.read_to_end(&mut data).unwrap();
        let pal = Palette::from_bytes(&data).unwrap();
        assert_eq!(pal.rgb(1).unwrap(), Rgb { data: [189, 0, 189] });
    }

    #[test]
    fn it_can_be_empty() {
        let empty = Vec::new();
        let pal = Palette::from_bytes(&empty).unwrap();
        assert_eq!(pal.color_count, 0);
    }
}
