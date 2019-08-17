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
use i386::{Interpreter, Reg};
use image::{GrayImage, ImageBuffer, Luma};
use lazy_static::lazy_static;
use log::trace;
use rusttype::{Font, Point, Scale};
use std::{collections::HashMap, sync::Arc};
use vulkano::{
    descriptor::descriptor_set::{DescriptorSet, PersistentDescriptorSet},
    device::Device,
    format::Format,
    image::{Dimensions, ImmutableImage},
    pipeline::GraphicsPipelineAbstract,
    sampler::{Filter, MipmapMode, Sampler, SamplerAddressMode},
    sync::GpuFuture,
};
use window::GraphicsWindow;

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
        pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
        window: &GraphicsWindow,
    ) -> Fallible<Self> {
        Ok(GlyphCache::FNT(GlyphCacheFNT::new_transparent_fnt(
            fnt, pipeline, window,
        )?))
    }

    pub fn new_ttf(
        bytes: &'static [u8],
        pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
        window: &GraphicsWindow,
    ) -> Fallible<Self> {
        Ok(GlyphCache::TTF(GlyphCacheTTF::new_ttf(
            bytes, pipeline, window,
        )?))
    }

    pub fn render_height(&self) -> f32 {
        match self {
            GlyphCache::FNT(fnt) => fnt.render_height,
            GlyphCache::TTF(ttf) => ttf.render_height,
        }
    }

    pub fn descriptor_set(&self) -> Arc<dyn DescriptorSet + Send + Sync> {
        match self {
            GlyphCache::FNT(fnt) => fnt.pds.clone(),
            GlyphCache::TTF(ttf) => ttf.pds.clone(),
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
        window: &GraphicsWindow,
        image_buf: ImageBuffer<Luma<u8>, Vec<u8>>,
    ) -> Fallible<(Arc<ImmutableImage<Format>>, Box<dyn GpuFuture>)> {
        let image_dim = image_buf.dimensions();
        let image_data = image_buf.into_raw().clone();

        let dimensions = Dimensions::Dim2d {
            width: image_dim.0,
            height: image_dim.1,
        };
        let (texture, tex_future) = ImmutableImage::from_iter(
            image_data.iter().cloned(),
            dimensions,
            Format::R8Unorm,
            window.queue(),
        )?;
        trace!("uploading texture with {} bytes", image_dim.0 * image_dim.1);
        Ok((texture, Box::new(tex_future) as Box<dyn GpuFuture>))
    }

    fn make_sampler(device: Arc<Device>) -> Fallible<Arc<Sampler>> {
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

pub struct GlyphCacheFNT {
    // These get composited in software, then uploaded in a single texture.
    pds: Arc<dyn DescriptorSet + Send + Sync>,

    // The positions of glyphs within the texture (as needed for layout later)
    // are stored in a map by glyph index.
    glyph_frames: HashMap<char, GlyphFrame>,

    // The intended render height of the font in vulkan coordinates.
    render_height: f32,
}

impl GlyphCacheFNT {
    pub fn new_transparent_fnt(
        fnt: &Fnt,
        pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
        window: &GraphicsWindow,
    ) -> Fallible<Self> {
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

        let (texture, tex_future) = GlyphCache::upload_texture_luma(window, buf)?;
        tex_future.then_signal_fence_and_flush()?.cleanup_finished();
        let sampler = GlyphCache::make_sampler(window.device())?;

        let pds = Arc::new(
            PersistentDescriptorSet::start(pipeline.clone(), 0)
                .add_sampled_image(texture.clone(), sampler.clone())?
                .build()?,
        );

        Ok(Self {
            pds,
            glyph_frames,
            render_height: fnt.height as f32 / SCREEN_SCALE[1],
        })
    }
}

pub struct GlyphCacheTTF {
    pds: Arc<dyn DescriptorSet + Send + Sync>,

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
        pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
        window: &GraphicsWindow,
    ) -> Fallible<Self> {
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

        let (texture, tex_future) = GlyphCache::upload_texture_luma(window, buf)?;
        tex_future.then_signal_fence_and_flush()?.cleanup_finished();
        let sampler = GlyphCache::make_sampler(window.device())?;

        let pds = Arc::new(
            PersistentDescriptorSet::start(pipeline.clone(), 0)
                .add_sampled_image(texture.clone(), sampler.clone())?
                .build()?,
        );

        Ok(Self {
            pds,
            glyph_frames,
            font,
            scale,
            render_height: scale.y / (SCREEN_SCALE[1] * additional_scale),
        })
    }
}
