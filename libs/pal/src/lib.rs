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
extern crate failure;
extern crate image;

use failure::{ensure, Fallible};
use image::{Pixel, Rgb, Rgba};

#[derive(Clone)]
pub struct Palette {
    pub color_count: usize,
    entries: Vec<Rgba<u8>>,
}

impl Palette {
    pub fn from_bytes(data: &[u8]) -> Fallible<Palette> {
        // The VGA palette contains 6 bit colors, so we need to scale by 4.
        Self::from_bytes_with_scale(data, 4)
    }

    pub fn from_bytes_prescaled(data: &[u8]) -> Fallible<Palette> {
        Self::from_bytes_with_scale(data, 1)
    }

    fn from_bytes_with_scale(data: &[u8], scale: u8) -> Fallible<Palette> {
        ensure!(data.len() % 3 == 0, "expected data to divide cleanly by 3");
        let mut entries = Vec::new();
        let color_count = data.len() / 3;
        for i in 0..color_count {
            entries.push(Rgba {
                data: [
                    data[i * 3] * scale,
                    data[i * 3 + 1] * scale,
                    data[i * 3 + 2] * scale,
                    255,
                ],
            });
        }
        Ok(Palette {
            color_count,
            entries,
        })
    }

    pub fn rgba(&self, index: usize) -> Fallible<Rgba<u8>> {
        ensure!(index < self.entries.len(), "index outside of palette");
        Ok(self.entries[index])
    }

    pub fn rgb(&self, index: usize) -> Fallible<Rgb<u8>> {
        ensure!(index < self.entries.len(), "index outside of palette");
        // if index >= self.entries.len() {
        //     return Ok(Rgb { data: [0, 0, 0] });
        // }
        Ok(self.entries[index].to_rgb())
    }

    pub fn overlay_at(&mut self, other: &Palette, offset: usize) -> Fallible<()> {
        let mut dst_i = offset;
        for src_i in 0..other.entries.len() {
            //println!("at 0x{:02X}: {:?}", dst_i, other.entries[src_i]);
            self.entries[dst_i] = other.entries[src_i];
            dst_i += 1;
        }
        Ok(())
    }

    // Slice from [start to end), half-open.
    pub fn slice(&self, start: usize, end: usize) -> Fallible<Palette> {
        let slice = self.entries[start..end].to_owned();
        Ok(Palette {
            color_count: slice.len(),
            entries: slice,
        })
    }

    pub fn dump_png(&self, name: &str) -> Fallible<()> {
        let size = 80;
        let mut buf = image::ImageBuffer::new(16u32 * size, 16u32 * size);
        for i in 0..16 {
            for j in 0..16 {
                let off = (j << 4 | i) as usize;
                for ip in 0..size {
                    for jp in 0..size {
                        buf.put_pixel(i * size + ip, j * size + jp, self.rgb(off)?);
                    }
                }
            }
        }
        let img = image::ImageRgb8(buf);
        img.save(name.to_owned() + ".png")?;
        Ok(())
    }

    /// Show the given data as if it were colors for a palette. Ignores short data, data that
    /// ends with a truncated color, and data that is too long.
    pub fn dump_partial(data: &[u8], scale: u8, name: &str) -> Fallible<()> {
        let size = 80;
        let mut buf = image::ImageBuffer::new(16u32 * size, 16u32 * size);

        // Dump a haze everywhere so we know what was unset.
        for i in 0..16 {
            for j in 0..16 {
                for ip in 0..size {
                    for jp in 0..size {
                        let c = (255 * ((ip + (jp % 2)) % 2)) as u8;
                        let pixel = Rgb([c, c, c]);
                        buf.put_pixel(i * size + ip, j * size + jp, pixel.to_rgb());
                    }
                }
            }
        }

        let mut off = 0;
        for i in 0..16 {
            for j in 0..16 {
                for ip in 0..size {
                    for jp in 0..size {
                        let pixel = if off + 2 < data.len() {
                            Rgb([data[off] * scale, data[off + 1] * scale, data[off + 2] * scale])
                        } else if off < data.len() {
                            let c = (255 * ((ip + (jp % 2)) % 2)) as u8;
                            Rgb([c, 0, c])
                        } else {
                            let c = (255 * ((ip + (jp % 2)) % 2)) as u8;
                            Rgb([c, c, c])
                        };
                        buf.put_pixel(j * size + ip, i * size + jp, pixel);
                    }
                }
                off += 3;
            }
        }

        let img = image::ImageRgb8(buf);
        img.save(name.to_owned() + ".png")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::prelude::*;

    #[test]
    fn it_works_with_normal_palette() -> Fallible<()> {
        let mut fp = fs::File::open("test_data/PALETTE.PAL")?;
        let mut data = Vec::new();
        fp.read_to_end(&mut data)?;
        let pal = Palette::from_bytes(&data)?;
        assert_eq!(
            pal.rgb(1)?,
            Rgb {
                data: [252, 0, 252]
            }
        );
        Ok(())
    }

    #[test]
    fn it_can_be_empty() -> Fallible<()> {
        let empty = Vec::new();
        let pal = Palette::from_bytes(&empty)?;
        assert_eq!(pal.color_count, 0);
        Ok(())
    }
}
