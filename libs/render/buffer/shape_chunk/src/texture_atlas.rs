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
use image::DynamicImage;
use log::trace;
use pal::Palette;
use pic::Pic;
use std::{borrow::Cow, collections::HashMap, ops::DerefMut, sync::Arc};
use vulkano::{
    buffer::{BufferUsage, CpuAccessibleBuffer},
    command_buffer::AutoCommandBufferBuilder,
    device::Device,
    format::Format,
    image::{Dimensions, ImageLayout, ImageUsage, ImmutableImage, MipmapsCount},
    sampler::{Filter, MipmapMode, Sampler, SamplerAddressMode},
};
use window::GraphicsWindow;

const DUMP_ATLAS: bool = false;

#[derive(Clone, Debug)]
pub struct TexCoord {
    pub s: f32,
    pub t: f32,
}

#[derive(Clone, Debug)]
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

const ATLAS_WIDTH: usize = 1024 + 4 * 2 + 2;
const ATLAS_HEIGHT: usize = 4098;
const ATLAS_PLANE_SIZE: usize = ATLAS_WIDTH * ATLAS_HEIGHT * 4;

// Load padded/wrapped 256px wide strips into a 2048+ 2D image slices stacked into
// a Texture2DArray for upload to the GPU. Each Atlas contains the textures for many
// different shapes.
pub struct MegaAtlas {
    // The stack of images that we are building into.
    // Note: we cannot build directly into gpu mapped memory because Texture2DArray
    // needs to know how many slices and we don't have that up front.
    images: Vec<Arc<CpuAccessibleBuffer<[u8]>>>,

    // For each image, for each of the 8 columns in the image, how many vertical
    // pixels are used.
    utilization: Vec<[usize; 4]>,

    // Map from the image that was inserted to it's texture coords.
    frames: HashMap<String, Frame>,
}

impl MegaAtlas {
    pub fn new(window: &GraphicsWindow) -> Fallible<Self> {
        let buffer: Arc<CpuAccessibleBuffer<[u8]>> = unsafe {
            CpuAccessibleBuffer::raw(
                window.device(),
                ATLAS_PLANE_SIZE,
                BufferUsage::all(),
                vec![window.queue().family()],
            )?
        };
        Ok(Self {
            images: vec![buffer],
            utilization: vec![[0; 4]],
            frames: HashMap::new(),
        })
    }

    // FIXME: we're using First-Fit, which is probably not optimal.
    fn find_first_fit(&self, height: usize) -> Option<(usize, usize)> {
        for (layer, util_array) in self.utilization.iter().enumerate() {
            for (column, util) in util_array.iter().enumerate() {
                if (ATLAS_HEIGHT - *util) >= height + 2 {
                    return Some((layer, column));
                }
            }
        }
        None
    }

    pub fn push(
        &mut self,
        name: &str,
        pic: &Pic,
        data: Cow<'_, [u8]>,
        palette: &Palette,
        window: &GraphicsWindow,
    ) -> Fallible<Frame> {
        // If we have already loaded the texture, just return the existing frame.
        if let Some(frame) = self.frames.get(name) {
            return Ok(frame.clone());
        }

        ensure!(
            pic.width == 256,
            format!("non-standard image width: {}", pic.width)
        );
        ensure!(pic.height + 2 < ATLAS_HEIGHT as u32, "source too tall");
        let first_fit = self.find_first_fit(pic.height as usize);
        let (layer, column) = if first_fit.is_none() {
            let buffer: Arc<CpuAccessibleBuffer<[u8]>> = unsafe {
                CpuAccessibleBuffer::raw(
                    window.device(),
                    ATLAS_PLANE_SIZE,
                    BufferUsage::all(),
                    vec![window.queue().family()],
                )?
            };
            self.images.push(buffer);
            self.utilization.push([0; 4]);
            (self.images.len() - 1, 0)
        } else {
            first_fit.unwrap()
        };

        // Each column in 256 + 2px -- use that to infer the offsets.
        let offset = [
            (column * 258 + 1) as u32,
            self.utilization[layer][column] as u32 + 1,
        ];
        let mut write_pointer = self.images[layer].write()?;
        Pic::decode_into_buffer(
            palette,
            write_pointer.deref_mut(),
            ATLAS_WIDTH,
            offset,
            pic,
            &data,
        )?;

        // FIXME: fill in border with a copy of the other side.

        // Update the utilization.
        self.utilization[layer][column] = (offset[1] + pic.height + 1) as usize;

        // Build the frame.
        self.frames.insert(
            name.to_owned(),
            Frame {
                coord0: TexCoord {
                    s: offset[0] as f32 / ATLAS_WIDTH as f32,
                    t: offset[1] as f32 / ATLAS_HEIGHT as f32,
                },
                coord1: TexCoord {
                    s: (offset[0] + pic.width) as f32 / ATLAS_WIDTH as f32,
                    t: (offset[1] + pic.height) as f32 / ATLAS_HEIGHT as f32,
                },
                width: pic.width as f32,
                height: pic.height as f32,
            },
        );
        println!("mega-atlas frame {}: {:?}", name, self.frames[name]);

        Ok(self.frames[name].clone())
    }

    pub fn finish(
        self,
        mut cbb: AutoCommandBufferBuilder,
        window: &GraphicsWindow,
    ) -> Fallible<(AutoCommandBufferBuilder, Arc<ImmutableImage<Format>>)> {
        if DUMP_ATLAS {
            for (layer, buffer) in self.images.iter().enumerate() {
                let reader = buffer.read()?;
                let mut img = DynamicImage::new_rgba8(ATLAS_WIDTH as u32, ATLAS_HEIGHT as u32);
                img.as_mut_rgba8().unwrap().copy_from_slice(&reader);
                println!("saving...");
                img.save(&format!("./mega-atlas-{}.png", layer))?;
                println!("saved!");
            }
        }

        let (image, upload) = ImmutableImage::uninitialized(
            window.device(),
            Dimensions::Dim2dArray {
                width: ATLAS_WIDTH as u32,
                height: ATLAS_HEIGHT as u32,
                array_layers: self.images.len() as u32,
            },
            Format::R8G8B8A8Unorm,
            MipmapsCount::One,
            ImageUsage {
                transfer_destination: true,
                sampled: true,
                ..ImageUsage::none()
            },
            ImageLayout::General,
            vec![window.queue().family()],
        )?;
        let upload = Arc::new(upload);

        for (i, buffer) in self.images.iter().enumerate() {
            cbb = cbb.copy_buffer_to_image_dimensions(
                buffer.clone(),
                upload.clone(),
                [0, 0, 0],
                [ATLAS_WIDTH as u32, ATLAS_HEIGHT as u32, 1],
                i as u32,
                1,
                0,
            )?;
        }

        Ok((cbb, image))
    }

    // Size in bytes of the final upload.
    pub fn atlas_size(&self) -> usize {
        4 * ATLAS_WIDTH * ATLAS_HEIGHT * self.images.len()
    }

    pub fn make_sampler(device: Arc<Device>) -> Fallible<Arc<Sampler>> {
        let sampler = Sampler::new(
            device.clone(),
            Filter::Nearest,
            Filter::Nearest,
            MipmapMode::Nearest,
            SamplerAddressMode::ClampToEdge,
            SamplerAddressMode::ClampToEdge,
            SamplerAddressMode::ClampToEdge,
            0.0,
            1.0,
            0.0,
            0.0,
        )?;

        Ok(sampler)
    }
}
