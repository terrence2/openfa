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
use failure::{ensure, Fallible};
use image::{Pixel, Rgb, Rgba};
use std::{borrow::Cow, fs::File, io::Write};

#[derive(Clone)]
pub struct Palette {
    pub color_count: usize,
    entries: Vec<Rgb<u8>>,
}

impl Palette {
    pub fn empty() -> Fallible<Self> {
        Self::from_bytes(&[])
    }

    pub fn grayscale() -> Fallible<Self> {
        let mut arr = [0u8; 256 * 3];
        for i in 0..256 {
            arr[i * 3] = i as u8;
            arr[i * 3 + 1] = i as u8;
            arr[i * 3 + 2] = i as u8;
        }
        Self::from_bytes_prescaled(&arr)
    }

    pub fn from_bytes(data: &[u8]) -> Fallible<Self> {
        // The VGA palette contains 6 bit colors, so we need to scale by 4 and add the bottom 2 bits.
        ensure!(data.len() % 3 == 0, "expected data to divide cleanly by 3");
        let mut entries = Vec::new();
        let color_count = data.len() / 3;
        for i in 0..color_count {
            entries.push(Rgb {
                data: [
                    (data[i * 3] << 2) | (data[i * 3] >> 6),
                    (data[i * 3 + 1] << 2) | (data[i * 3 + 1] >> 6),
                    (data[i * 3 + 2] << 2) | (data[i * 3 + 2] >> 6),
                ],
            });
        }
        Ok(Self {
            color_count,
            entries,
        })
    }

    pub fn from_bytes_prescaled(data: &[u8]) -> Fallible<Self> {
        ensure!(data.len() % 3 == 0, "expected data to divide cleanly by 3");
        let mut entries = Vec::new();
        let color_count = data.len() / 3;
        for i in 0..color_count {
            entries.push(Rgb {
                data: [data[i * 3], data[i * 3 + 1], data[i * 3 + 2]],
            });
        }
        Ok(Self {
            color_count,
            entries,
        })
    }

    pub fn iter(&self) -> std::slice::Iter<Rgb<u8>> {
        self.entries.iter()
    }

    pub fn rgba(&self, index: usize) -> Fallible<Rgba<u8>> {
        ensure!(index < self.entries.len(), "index outside of palette");
        Ok(Rgba {
            data: [
                self.entries[index][0],
                self.entries[index][1],
                self.entries[index][2],
                255,
            ],
        })
    }

    pub fn rgba_f32(&self, index: usize) -> Fallible<[f32; 4]> {
        let c = self.rgb(index)?;
        Ok([
            f32::from(c.data[0]) / 256f32,
            f32::from(c.data[1]) / 256f32,
            f32::from(c.data[2]) / 256f32,
            1f32,
        ])
    }

    pub fn rgb(&self, index: usize) -> Fallible<Rgb<u8>> {
        ensure!(index < self.entries.len(), "index outside of palette");
        Ok(self.entries[index])
    }

    pub fn overlay_at(&mut self, other: &Palette, offset: usize) -> Fallible<()> {
        let mut dst_i = offset;
        for src_i in 0..other.entries.len() {
            if dst_i >= self.entries.len() {
                break;
            }
            self.entries[dst_i] = other.entries[src_i];
            dst_i += 1;
        }
        Ok(())
    }

    pub fn override_one(&mut self, offset: usize, color: [u8; 3]) {
        self.entries[offset][0] = color[0];
        self.entries[offset][1] = color[1];
        self.entries[offset][2] = color[2];
    }

    // Slice from [start to end), half-open.
    pub fn slice(&self, start: usize, end: usize) -> Fallible<Palette> {
        let slice = self.entries[start..end].to_owned();
        Ok(Palette {
            color_count: slice.len(),
            entries: slice,
        })
    }

    /// Dump this pal to `path` in PNG format, in a 16x16 grid, expanding each
    /// entry to an 80x80 pixel square in order to increase visbility.
    pub fn dump_png(&self, path: &str) -> Fallible<()> {
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
        img.save(path.to_owned() + ".png")?;
        Ok(())
    }

    /// Serialize to raw palette format.
    pub fn as_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.entries.len() * 3);
        for entry in &self.entries {
            out.push(entry[0] >> 2);
            out.push(entry[1] >> 2);
            out.push(entry[2] >> 2);
        }
        out
    }

    /// Save this pal to `path` in PAL format (e.g. raw VGA palette data).
    pub fn dump_pal(&self, path: &str) -> Fallible<()> {
        let mut fp = File::create(path)?;
        fp.write_all(&self.as_bytes())?;
        Ok(())
    }

    /// Show the given data as if it were colors for a palette. Ignores short
    /// data, data that ends with a truncated color, and data that is too long.
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
                            Rgb([
                                data[off] * scale,
                                data[off + 1] * scale,
                                data[off + 2] * scale,
                            ])
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

impl<'a> From<&'a Palette> for Cow<'a, Palette> {
    #[inline]
    fn from(pal: &'a Palette) -> Cow<'a, Palette> {
        Cow::Borrowed(pal)
    }
}

impl<'a> From<Palette> for Cow<'a, Palette> {
    #[inline]
    fn from(pal: Palette) -> Cow<'a, Palette> {
        Cow::Owned(pal)
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
