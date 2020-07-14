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
use crate::{font_interface::FontInterface, glyph_frame::GlyphFrame};

use codepage_437::{FromCp437, CP437_CONTROL};
use failure::{ensure, Fallible};
use fnt::Fnt;
use gpu::GPU;
use image::{GrayImage, ImageBuffer, Luma};
use log::trace;
use rusttype::{Font, Point, Scale};
use std::collections::HashMap;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct GlyphCacheIndex(usize);

impl GlyphCacheIndex {
    pub fn new(index: usize) -> Self {
        Self(index)
    }

    pub fn index(self) -> usize {
        self.0
    }
}

pub struct GlyphCache {
    index: GlyphCacheIndex,
    font: Box<dyn FontInterface>,
}

impl GlyphCache {
    /*
    pub fn new_transparent_fnt(
        fnt: &Fnt,
        index: GlyphCacheIndex,
        bind_group_layout: &wgpu::BindGroupLayout,
        gpu: &mut GPU,
    ) -> Fallible<Self> {
        Ok(Self {
            index,
            font: Box::new(FntFont::new_transparent_fnt(fnt, bind_group_layout, gpu)?),
        })
    }

    pub fn new_ttf(
        bytes: &'static [u8],
        index: GlyphCacheIndex,
        bind_group_layout: &wgpu::BindGroupLayout,
        gpu: &mut GPU,
    ) -> Fallible<Self> {
        Ok(Self {
            index,
            font: Box::new(TtfFont::new_ttf(bytes, bind_group_layout, gpu)?),
        })
    }
     */
    pub fn new(index: GlyphCacheIndex, font: Box<dyn FontInterface>) -> Self {
        Self { index, font }
    }

    pub fn render_height(&self) -> f32 {
        self.font.render_height()
    }

    pub fn create_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("glyph-cache-bind-group-layout"),
            bindings: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStage::FRAGMENT,
                    ty: wgpu::BindingType::SampledTexture {
                        dimension: wgpu::TextureViewDimension::D2,
                        component_type: wgpu::TextureComponentType::Uint,
                        multisampled: false,
                    },
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStage::FRAGMENT,
                    ty: wgpu::BindingType::Sampler { comparison: false },
                },
            ],
        })
    }

    pub fn index(&self) -> GlyphCacheIndex {
        self.index
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        self.font.bind_group()
    }

    pub fn can_render_char(&self, c: char) -> bool {
        self.font.can_render_char(c)
    }

    pub fn frame_for(&self, c: char) -> &GlyphFrame {
        self.font.frame_for(c)
    }

    pub fn pair_kerning(&self, a: char, b: char) -> f32 {
        self.font.pair_kerning(a, b)
    }

    pub(crate) fn upload_texture_luma(
        gpu: &mut GPU,
        image_buf: ImageBuffer<Luma<u8>, Vec<u8>>,
    ) -> Fallible<wgpu::TextureView> {
        let image_dim = image_buf.dimensions();
        let extent = wgpu::Extent3d {
            width: image_dim.0,
            height: image_dim.1,
            depth: 1,
        };
        let image_data = image_buf.into_raw();

        let transfer_buffer = gpu.push_buffer(
            "glyph-cache-transfer-buffer",
            &image_data,
            wgpu::BufferUsage::all(),
        );
        let texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("glyph-cache-texture"),
            size: extent,
            array_layer_count: 1,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsage::all(),
        });
        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("glyph-cache-command-encoder"),
            });
        encoder.copy_buffer_to_texture(
            wgpu::BufferCopyView {
                buffer: &transfer_buffer,
                offset: 0,
                bytes_per_row: extent.width,
                rows_per_image: extent.height,
            },
            wgpu::TextureCopyView {
                texture: &texture,
                mip_level: 0,
                array_layer: 0,
                origin: wgpu::Origin3d::ZERO,
            },
            extent,
        );
        gpu.queue_mut().submit(&[encoder.finish()]);
        gpu.device().poll(wgpu::Maintain::Wait);

        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor {
            format: wgpu::TextureFormat::R8Unorm,
            dimension: wgpu::TextureViewDimension::D2,
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            level_count: 1, // mip level
            base_array_layer: 0,
            array_layer_count: 1,
        });

        Ok(texture_view)
    }

    pub(crate) fn make_sampler(device: &wgpu::Device) -> wgpu::Sampler {
        device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            lod_min_clamp: 0f32,
            lod_max_clamp: 9_999_999f32,
            compare: wgpu::CompareFunction::Never,
        })
    }
}
