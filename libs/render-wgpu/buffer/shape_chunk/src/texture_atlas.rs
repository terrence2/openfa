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
use anyhow::{ensure, Result};
use gpu::GPU;
use image::DynamicImage;
use log::trace;
use pal::Palette;
use pic::Pic;
use std::{borrow::Cow, collections::HashMap};

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
    fn new(offset: [u32; 2], pic: &Pic) -> Self {
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
        }
    }

    pub(crate) fn tex_coord_at(&self, raw: [u16; 2]) -> [f32; 2] {
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

const ATLAS_WIDTH0: usize = 1024 + 4 * 2 + 2;
const ATLAS_STRIDE: usize = GPU::stride_for_row_size(ATLAS_WIDTH0 as u32 * 4) as usize;
const ATLAS_WIDTH: usize = ATLAS_STRIDE / 4;
const ATLAS_HEIGHT: usize = 4098;
const ATLAS_PLANE_SIZE: usize = ATLAS_STRIDE * ATLAS_HEIGHT;

// Load padded/wrapped 256px wide strips into a 2048+ 2D image slices stacked into
// a Texture2DArray for upload to the GPU. Each Atlas contains the textures for many
// different shapes.
pub(crate) struct MegaAtlas {
    // The stack of images that we are building into.
    // Note: we cannot build directly into gpu mapped memory because Texture2DArray
    // needs to know how many slices and we don't have that up front.
    images: Vec<Vec<u8>>,

    // For each image, for each of the 4 columns in the image, how many vertical
    // pixels are used.
    utilization: Vec<[usize; 4]>,

    // Map from the image that was inserted to it's texture coords.
    frames: HashMap<String, Frame>,
}

impl MegaAtlas {
    pub(crate) fn new() -> Result<Self> {
        Ok(Self {
            images: vec![vec![0; ATLAS_PLANE_SIZE]],
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

    pub(crate) fn push(
        &mut self,
        name: &str,
        pic: &Pic,
        data: Cow<'_, [u8]>,
        palette: &Palette,
    ) -> Result<Frame> {
        // If we have already loaded the texture, just return the existing frame.
        if let Some(frame) = self.frames.get(name) {
            return Ok(frame.clone());
        }

        ensure!(
            pic.width == 256,
            format!("non-standard image width: {}", pic.width)
        );
        ensure!(pic.height + 2 < ATLAS_HEIGHT as u32, "source too tall");
        let (layer, column) = if let Some(first_fit) = self.find_first_fit(pic.height as usize) {
            first_fit
        } else {
            self.images.push(vec![0; ATLAS_PLANE_SIZE]);
            self.utilization.push([0; 4]);
            (self.images.len() - 1, 0)
        };

        // Each column in 256 + 2px -- use that to infer the offsets.
        let offset = [
            (column * 258 + 1) as u32,
            self.utilization[layer][column] as u32 + 1,
        ];
        let write_pointer = &mut self.images[layer];
        Pic::decode_into_buffer(palette, write_pointer, ATLAS_WIDTH, offset, pic, &data)?;

        // FIXME: fill in border with a copy of the other side.

        // Update the utilization.
        self.utilization[layer][column] = (offset[1] + pic.height + 1) as usize;

        // Build the frame.
        self.frames.insert(name.to_owned(), Frame::new(offset, pic));
        trace!("mega-atlas loaded {}", name);

        Ok(self.frames[name].clone())
    }

    pub(crate) fn finish(self, gpu: &mut gpu::GPU) -> Result<wgpu::TextureView> {
        if DUMP_ATLAS {
            for (layer, buffer) in self.images.iter().enumerate() {
                let mut img = DynamicImage::new_rgba8(ATLAS_WIDTH as u32, ATLAS_HEIGHT as u32);
                img.as_mut_rgba8().unwrap().copy_from_slice(&buffer);
                println!("saving...");
                img.save(&format!("./mega-atlas-{}.png", layer))?;
                println!("saved!");
            }
        }

        let extent = wgpu::Extent3d {
            width: ATLAS_WIDTH as u32,
            height: ATLAS_HEIGHT as u32,
            depth: 1, //self.images.len() as u32,
        };
        let texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("shape-chunk-atlas-texture"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsage::all(),
        });
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("shape-chunk-atlas-texture-view"),
            format: None,
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            level_count: None,
            base_array_layer: 0,
            array_layer_count: None, //NonZeroU32::new(self.images.len() as u32),
        });

        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("shape-chunk-texture-atlas-uploader-command-encoder"),
            });
        for (_i, layer) in self.images.iter().enumerate() {
            let buffer = gpu.push_buffer(
                "shape-chunk-texture-atlas-upload-buffer",
                &layer,
                wgpu::BufferUsage::COPY_SRC,
            );

            encoder.copy_buffer_to_texture(
                wgpu::BufferCopyView {
                    buffer: &buffer,
                    layout: wgpu::TextureDataLayout {
                        offset: 0,
                        bytes_per_row: extent.width * 4,
                        rows_per_image: extent.height,
                    },
                },
                wgpu::TextureCopyView {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                },
                wgpu::Extent3d {
                    width: ATLAS_WIDTH as u32,
                    height: ATLAS_HEIGHT as u32,
                    depth: 1, // i as u32,
                },
            );
        }
        gpu.queue_mut().submit(vec![encoder.finish()]);

        Ok(texture_view)
    }

    // Size in bytes of the final upload.
    pub fn atlas_size(&self) -> usize {
        ATLAS_PLANE_SIZE * self.images.len()
    }

    pub fn make_sampler(device: &wgpu::Device) -> wgpu::Sampler {
        device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shape-chunk-atlas-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            lod_min_clamp: 0f32,
            lod_max_clamp: 9_999_999f32,
            anisotropy_clamp: None,
            compare: None,
            border_color: None,
        })
    }

    pub fn make_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shape-chunk-texture-atlas-bind-group-layout"),
            entries: &[
                // Shared Shape Texture Atlas
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStage::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: true,
                        sample_type: wgpu::TextureSampleType::Uint,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStage::FRAGMENT,
                    ty: wgpu::BindingType::Sampler {
                        filtering: true,
                        comparison: false,
                    },
                    count: None,
                },
            ],
        })
    }
}
