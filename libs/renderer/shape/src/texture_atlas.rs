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
use image::DynamicImage;
use log::trace;
use pal::Palette;
use pic::Pic;
use std::{borrow::Cow, collections::HashMap};

pub struct TexCoord {
    pub s: f32,
    pub t: f32,
}

pub struct Frame {
    pub coord0: TexCoord,
    pub coord1: TexCoord,
    pub width: f32,
    pub height: f32,
}

impl Frame {
    pub fn tex_coord_at(&self, raw: [u16; 2]) -> [f32; 2] {
        // The raw coords are in terms of bitmap pixels, so normalize first.
        let n = TexCoord {
            s: f32::from(raw[0]) / self.width,
            t: 1f32 - f32::from(raw[1]) / self.height,
        };

        // Project the normalized numbers above into the frame.
        [
            self.coord0.s + (n.s * (self.coord1.s - self.coord0.s)),
            self.coord0.t + (n.t * (self.coord1.t - self.coord0.t)),
        ]
    }
}

pub struct TextureAtlas {
    pub img: DynamicImage,
    pub frames: HashMap<String, Frame>,
}

impl TextureAtlas {
    pub fn from_raw_data(
        palette: &Palette,
        mut sources: Vec<(String, Pic, Cow<'_, [u8]>)>,
    ) -> Fallible<Self> {
        // Note that sources may be empty if the model is untextured.
        if sources.is_empty() {
            return Ok(Self {
                img: DynamicImage::new_rgba8(1, 1),
                frames: HashMap::new(),
            });
        }

        sources.sort_by_key(|(_, img, _)| img.height);
        sources.reverse();

        // Pre-pass to get a width and height.
        let mut atlas_width = 0;
        let mut atlas_height = 0;
        for (_name, pic, _data) in &sources {
            atlas_width += pic.width + 1;
            atlas_height = atlas_height.max(pic.height);
        }
        trace!("sh atlas size: {}x{}", atlas_width, atlas_height);

        // Copy images to destination.
        let mut img = DynamicImage::new_rgba8(atlas_width, atlas_height);
        let mut frames = HashMap::with_capacity(sources.len());
        let mut offset = 0;
        for (name, pic, data) in sources.drain(..) {
            let coord0 = TexCoord {
                s: offset as f32 / atlas_width as f32,
                t: 0.0f32,
            };
            let coord1 = TexCoord {
                s: (offset + pic.width) as f32 / atlas_width as f32,
                t: pic.height as f32 / atlas_height as f32,
            };

            frames.insert(
                name,
                Frame {
                    coord0,
                    coord1,
                    width: pic.width as f32,
                    height: pic.height as f32,
                },
            );

            Pic::decode_into(palette, &mut img, offset, 0, &pic, &data)?;

            offset += pic.width;
        }

        Ok(Self { img, frames })
    }
}
