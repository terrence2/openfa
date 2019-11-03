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
use codepage_437::{FromCp437, CP437_CONTROL};
use failure::{ensure, Fallible};
use fnt::Fnt;
use gpu::GPU;
use i386::{Interpreter, Reg};
use image::{GrayImage, ImageBuffer, Luma};
use lazy_static::lazy_static;
use log::trace;
use rusttype::{Font, Point, Scale};
use std::collections::HashMap;

lazy_static! {
    static ref CP437_TO_CHAR: HashMap<u8, char> = {
        let dos: Vec<u8> = (1..255).collect();
        let utf = String::from_cp437(dos, &CP437_CONTROL);
        (1..255).zip(utf.chars()).collect()
    };
}

const SCREEN_SCALE: [f32; 2] = [320f32, 240f32];

#[derive(Debug)]
pub struct GlyphFrame {
    // Left and right texture coordinates.
    pub s0: f32,
    pub s1: f32,

    // Width scaled into the right perspective for rendering.
    pub advance_width: f32,
    pub left_side_bearing: f32,
}

pub enum GlyphCache {
    FNT(GlyphCacheFNT),
    TTF(GlyphCacheTTF),
}

impl GlyphCache {
    pub fn new_transparent_fnt(
        fnt: &Fnt,
        bind_group_layout: &wgpu::BindGroupLayout,
        gpu: &mut GPU,
    ) -> Fallible<Self> {
        Ok(GlyphCache::FNT(GlyphCacheFNT::new_transparent_fnt(
            fnt,
            bind_group_layout,
            gpu,
        )?))
    }

    pub fn new_ttf(
        bytes: &'static [u8],
        bind_group_layout: &wgpu::BindGroupLayout,
        gpu: &mut GPU,
    ) -> Fallible<Self> {
        Ok(GlyphCache::TTF(GlyphCacheTTF::new_ttf(
            bytes,
            bind_group_layout,
            gpu,
        )?))
    }

    pub fn render_height(&self) -> f32 {
        match self {
            GlyphCache::FNT(fnt) => fnt.render_height,
            GlyphCache::TTF(ttf) => ttf.render_height,
        }
    }

    pub fn create_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            bindings: &[
                wgpu::BindGroupLayoutBinding {
                    binding: 0,
                    visibility: wgpu::ShaderStage::FRAGMENT,
                    ty: wgpu::BindingType::SampledTexture {
                        multisampled: false,
                        dimension: wgpu::TextureViewDimension::D2,
                    },
                },
                wgpu::BindGroupLayoutBinding {
                    binding: 1,
                    visibility: wgpu::ShaderStage::FRAGMENT,
                    ty: wgpu::BindingType::Sampler,
                },
            ],
        })
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        match self {
            GlyphCache::FNT(fnt) => &fnt.bind_group,
            GlyphCache::TTF(ttf) => &ttf.bind_group,
        }
    }

    pub fn can_render_char(&self, c: char) -> bool {
        match self {
            GlyphCache::FNT(fnt) => fnt.glyph_frames.contains_key(&c),
            GlyphCache::TTF(ttf) => ttf.glyph_frames.contains_key(&c),
        }
    }

    pub fn frame_for(&self, c: char) -> &GlyphFrame {
        match self {
            GlyphCache::FNT(fnt) => &fnt.glyph_frames[&c],
            GlyphCache::TTF(ttf) => &ttf.glyph_frames[&c],
        }
    }

    pub fn pair_kerning(&self, a: char, b: char) -> f32 {
        match self {
            GlyphCache::FNT(_) => 0f32,
            GlyphCache::TTF(ttf) => ttf.font.pair_kerning(ttf.scale, a, b),
        }
    }

    fn upload_texture_luma(
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

        let transfer_buffer = gpu
            .device()
            .create_buffer_mapped(image_data.len(), wgpu::BufferUsage::all())
            .fill_from_slice(&image_data);
        let texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
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
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { todo: 0 });
        encoder.copy_buffer_to_texture(
            wgpu::BufferCopyView {
                buffer: &transfer_buffer,
                offset: 0,
                row_pitch: extent.width,
                image_height: extent.height,
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
        gpu.device().poll(true);

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

    fn make_sampler(device: &wgpu::Device) -> wgpu::Sampler {
        device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            lod_min_clamp: 0f32,
            lod_max_clamp: 9_999_999f32,
            compare_function: wgpu::CompareFunction::Never,
        })
    }
}

pub struct GlyphCacheFNT {
    // These get composited in software, then uploaded in a single texture.
    bind_group: wgpu::BindGroup,

    // The positions of glyphs within the texture (as needed for layout later)
    // are stored in a map by glyph index.
    glyph_frames: HashMap<char, GlyphFrame>,

    // The intended render height of the font in vulkan coordinates.
    render_height: f32,
}

impl GlyphCacheFNT {
    pub fn new_transparent_fnt(
        fnt: &Fnt,
        bind_group_layout: &wgpu::BindGroupLayout,
        gpu: &mut GPU,
    ) -> Fallible<Self> {
        trace!("GlyphCacheFNT::new");

        let mut width = 0;
        for glyph_index in 0..=255 {
            if !fnt.glyphs.contains_key(&glyph_index) {
                continue;
            }
            width += fnt.glyphs[&glyph_index].width;
        }

        let mut buf = GrayImage::new(width as u32, fnt.height as u32);
        for p in buf.pixels_mut() {
            *p = Luma { data: [0] };
        }

        let mut interp = Interpreter::new();
        interp.add_trampoline(0x60_0000, "finish", 0);
        interp.set_register_value(Reg::EAX, 0xFF_FF_FF_FF);
        interp.set_register_value(Reg::ECX, width as u32);
        interp.set_register_value(Reg::EDI, 0x30_0000);
        interp.map_writable(0x30_0000, buf.into_raw())?;

        let mut x = 0;
        let mut glyph_frames = HashMap::new();
        for glyph_index in 0..=255 {
            if !fnt.glyphs.contains_key(&glyph_index) {
                continue;
            }
            let glyph = &fnt.glyphs[&glyph_index];

            interp.clear_code();
            interp.add_code(glyph.bytecode.clone());
            interp.push_stack_value(0x60_0000);

            let rv = interp.interpret(0)?;
            let (trampoline_name, args) = rv.ok_trampoline()?;
            ensure!(trampoline_name == "finish", "expect return to finish");
            ensure!(args.is_empty(), "expect no args out");

            glyph_frames.insert(
                CP437_TO_CHAR[&glyph_index],
                GlyphFrame {
                    s0: x as f32 / width as f32,
                    s1: (x + glyph.width) as f32 / width as f32,
                    advance_width: glyph.width as f32 / SCREEN_SCALE[0],
                    left_side_bearing: 0f32,
                },
            );
            x += glyph.width;
            interp.set_register_value(Reg::EDI, 0x30_0000 + x as u32);
        }

        let plane = interp.unmap_writable(0x30_0000)?;
        let buf =
            GrayImage::from_raw(width as u32, fnt.height as u32, plane).expect("same parameters");

        let texture_view = GlyphCache::upload_texture_luma(gpu, buf)?;
        let sampler = GlyphCache::make_sampler(gpu.device());

        let bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            layout: bind_group_layout,
            bindings: &[
                wgpu::Binding {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::Binding {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        Ok(Self {
            bind_group,
            glyph_frames,
            render_height: fnt.height as f32 / SCREEN_SCALE[1],
        })
    }
}

pub struct GlyphCacheTTF {
    bind_group: wgpu::BindGroup,

    // Map to positions in the glyph cache.
    glyph_frames: HashMap<char, GlyphFrame>,

    // The actual font data.
    font: Font<'static>,

    // The rendered scale of the font.
    scale: Scale,

    render_height: f32,
}

impl GlyphCacheTTF {
    pub fn new_ttf(
        bytes: &'static [u8],
        bind_group_layout: &wgpu::BindGroupLayout,
        gpu: &mut GPU,
    ) -> Fallible<Self> {
        trace!("GlyphCacheTTF::new");

        let font = Font::from_bytes(bytes)?;

        let scale = Scale::uniform(64.0);
        let additional_scale = 8f32;
        const ORIGIN: Point<f32> = Point { x: 0.0, y: 0.0 };
        let mut glyph_frames = HashMap::new();

        // Find our aggregate width.
        let v_metrics = font.v_metrics(scale);
        let height = v_metrics.ascent - v_metrics.descent;
        let pixel_height = (height).ceil() as u32;
        let mut pixel_width = 0u32;
        for i in 1..255 {
            let c = CP437_TO_CHAR[&i];
            let glyph = font.glyph(c).scaled(scale).positioned(ORIGIN);
            if let Some(bb) = glyph.pixel_bounding_box() {
                pixel_width += (bb.max.x - bb.min.x) as u32 + 1;
            }
        }

        // Extract all necessary glyphs to a texture and upload to GPU.
        let mut buf = GrayImage::new(pixel_width, pixel_height);
        let mut offset = 0;
        for i in 1..255 {
            let c = CP437_TO_CHAR[&i];
            let raw_glyph = font.glyph(c).scaled(scale);
            let h_metrics = raw_glyph.h_metrics();
            let glyph = raw_glyph.positioned(ORIGIN);
            if let Some(bb) = glyph.pixel_bounding_box() {
                glyph.draw(|x, y, v| {
                    buf.put_pixel(
                        offset + x,
                        (v_metrics.ascent + bb.min.y as f32 + y as f32).floor() as u32,
                        Luma([(v * 255.0) as u8]),
                    )
                });
                let glyph_width = (bb.max.x - bb.min.x) as u32;
                glyph_frames.insert(
                    c,
                    GlyphFrame {
                        s0: offset as f32 / pixel_width as f32,
                        s1: (offset + glyph_width) as f32 / pixel_width as f32,
                        advance_width: h_metrics.advance_width
                            / (SCREEN_SCALE[0] * additional_scale),
                        left_side_bearing: h_metrics.left_side_bearing
                            / (SCREEN_SCALE[0] * additional_scale),
                    },
                );
                offset += glyph_width + 1;
            }
        }

        let texture_view = GlyphCache::upload_texture_luma(gpu, buf)?;
        let sampler = GlyphCache::make_sampler(gpu.device());

        let bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            layout: bind_group_layout,
            bindings: &[
                wgpu::Binding {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::Binding {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        Ok(Self {
            bind_group,
            glyph_frames,
            font,
            scale,
            render_height: scale.y / (SCREEN_SCALE[1] * additional_scale),
        })
    }
}
