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
use image::{DynamicImage, GenericImage, GenericImageView};
use log::trace;
use std::collections::HashMap;

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
    pub fn tex_coord_at(&self, raw: &[u16; 2]) -> [f32; 2] {
        // The raw coords are in terms of bitmap pixels, so normalize first.
        let n = TexCoord {
            s: raw[0] as f32 / self.width,
            t: 1f32 - raw[1] as f32 / self.height,
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
    pub fn new(mut sources: Vec<(String, DynamicImage)>) -> Fallible<Self> {
        // Note that sources may be empty if the model is untextured.
        if sources.len() == 0 {
            return Ok(Self {
                img: DynamicImage::new_rgba8(1, 1),
                frames: HashMap::new(),
            });
        }

        sources.sort_by_key(|(_, img)| img.height());
        sources.reverse();

        // Pre-pass to get a width and height.
        let mut atlas_width = 0;
        let mut atlas_height = 0;
        for (_name, img) in &sources {
            assert_eq!(img.width(), 256);
            atlas_width += img.width() + 1;
            atlas_height = atlas_height.max(img.height());
        }
        trace!("sh atlas size: {}x{}", atlas_width, atlas_height);

        // Copy images to destination.
        let mut img = DynamicImage::new_rgba8(atlas_width, atlas_height);
        let mut frames = HashMap::new();
        let mut offset = 0;
        for (name, source) in sources.drain(..) {
            let coord0 = TexCoord {
                s: offset as f32 / atlas_width as f32,
                t: 0.0f32,
            };
            let coord1 = TexCoord {
                s: (offset + source.width()) as f32 / atlas_width as f32,
                t: source.height() as f32 / atlas_height as f32,
            };

            frames.insert(
                name,
                Frame {
                    coord0,
                    coord1,
                    width: source.width() as f32,
                    height: source.height() as f32,
                },
            );

            img.copy_from(&source, offset, 0);
            offset += source.width();
        }

        Ok(Self { img, frames })
    }
}
