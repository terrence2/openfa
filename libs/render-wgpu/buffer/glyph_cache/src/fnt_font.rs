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
use crate::{glyph_cache::GlyphCache, FontInterface, GlyphFrame};
use codepage_437::{FromCp437, CP437_CONTROL};
use failure::{ensure, Fallible};
use fnt::Fnt;
use gpu::GPU;
use i386::{Interpreter, Reg};
use image::{GrayImage, ImageBuffer, Luma};
use lazy_static::lazy_static;
use log::trace;
use std::collections::HashMap;

const SCREEN_SCALE: [f32; 2] = [320f32, 240f32];

lazy_static! {
    static ref CP437_TO_CHAR: HashMap<u8, char> = {
        let dos: Vec<u8> = (1..255).collect();
        let utf = String::from_cp437(dos, &CP437_CONTROL);
        (1..255).zip(utf.chars()).collect()
    };
}

pub struct FntFont {
    // These get composited in software, then uploaded in a single texture.
    bind_group: wgpu::BindGroup,

    // The positions of glyphs within the texture (as needed for layout later)
    // are stored in a map by glyph index.
    glyph_frames: HashMap<char, GlyphFrame>,

    // The intended render height of the font in vulkan coordinates.
    render_height: f32,
}

impl FontInterface for FntFont {
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

    fn pair_kerning(&self, _a: char, _b: char) -> f32 {
        0f32
    }
}

impl FntFont {
    pub fn new(
        fnt: &Fnt,
        bind_group_layout: &wgpu::BindGroupLayout,
        gpu: &mut GPU,
    ) -> Fallible<Box<dyn FontInterface>> {
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
            label: Some("glyph-cache-FNT-bind-group"),
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
            render_height: fnt.height as f32 / SCREEN_SCALE[1],
        }) as Box<dyn FontInterface>)
    }
}
