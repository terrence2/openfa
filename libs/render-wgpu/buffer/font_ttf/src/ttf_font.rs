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
use failure::Fallible;
use glyph_cache::{FontInterface, GlyphCache, GlyphFrame};
use gpu::GPU;
use image::{GrayImage, Luma};
use lazy_static::lazy_static;
use log::trace;
use rusttype::{Font, Point, Scale};
use std::collections::HashMap;

const SCREEN_SCALE: [f32; 2] = [320f32, 240f32];

lazy_static! {
    static ref CP437_TO_CHAR: HashMap<u8, char> = {
        let dos: Vec<u8> = (1..255).collect();
        let utf = String::from_cp437(dos, &CP437_CONTROL);
        (1..255).zip(utf.chars()).collect()
    };
}

pub struct TtfFont {
    bind_group: wgpu::BindGroup,

    // Map to positions in the glyph cache.
    glyph_frames: HashMap<char, GlyphFrame>,

    // The actual font data.
    font: Font<'static>,

    // The rendered scale of the font.
    scale: Scale,

    render_height: f32,
}

impl FontInterface for TtfFont {
    fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    fn render_height(&self) -> f32 {
        self.render_height
    }

    fn can_render_char(&self, c: char) -> bool {
        self.glyph_frames.contains_key(&c)
    }

    fn frame_for(&self, c: char) -> &GlyphFrame {
        &self.glyph_frames[&c]
    }

    fn pair_kerning(&self, a: char, b: char) -> f32 {
        self.font.pair_kerning(self.scale, a, b)
    }
}

impl TtfFont {
    pub fn new(
        bytes: &'static [u8],
        bind_group_layout: &wgpu::BindGroupLayout,
        gpu: &mut GPU,
    ) -> Fallible<Box<dyn FontInterface>> {
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
            label: Some("glyph-cache-TTF-bind-group"),
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

        Ok(Box::new(Self {
            bind_group,
            glyph_frames,
            font,
            scale,
            render_height: scale.y / (SCREEN_SCALE[1] * additional_scale),
        }) as Box<dyn FontInterface>)
    }
}
