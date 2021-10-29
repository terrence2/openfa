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
use codepage_437::{FromCp437, CP437_CONTROL};
use fnt::Fnt;
use font_common::{FontAdvance, FontInterface};
use gpu::{
    size::{AbsSize, LeftBound},
    Gpu,
};
use i386::{Interpreter, Reg};
use image::{GenericImage, GenericImageView, GrayImage, Luma};
use lazy_static::lazy_static;
use log::trace;
use parking_lot::RwLock;
use std::{collections::HashMap, sync::Arc};

// FIXME: 11px at 240px tall is 4.583..% of the screen, which is what
//        we target at a scaling of 1.0 below.
//const SCREEN_SCALE: [f32; 2] = [320f32, 240f32];

lazy_static! {
    static ref CP437_TO_CHAR: HashMap<u8, char> = {
        let dos: Vec<u8> = (1..255).collect();
        let utf = String::from_cp437(dos, &CP437_CONTROL);
        (1..255).zip(utf.chars()).collect()
    };
}

#[derive(Debug)]
struct GlyphFrame {
    x_offset: u32,
    width: i32,
}

#[derive(Debug)]
pub struct FntFont {
    // These get composited in software, then uploaded in a single texture.
    // texture_view: wgpu::TextureView,
    // sampler: wgpu::Sampler,
    glyphs: GrayImage,
    height: u32,

    // The positions of glyphs within the texture (as needed for layout later)
    // are stored in a map by glyph index.
    glyph_frames: HashMap<char, GlyphFrame>,
}

// FIXME: rewrite this once we have a visual test
impl FontInterface for FntFont {
    // global metrics
    fn units_per_em(&self) -> f32 {
        self.height as f32
    }

    fn advance_style(&self) -> FontAdvance {
        FontAdvance::Mono
    }

    // vertical metrics
    fn ascent(&self, scale: AbsSize) -> AbsSize {
        scale * (self.height as f32 / self.units_per_em())
    }

    fn descent(&self, _scale: AbsSize) -> AbsSize {
        AbsSize::zero()
    }

    fn line_gap(&self, _scale: AbsSize) -> AbsSize {
        AbsSize::zero()
    }

    // horizontal metrics
    fn advance_width(&self, c: char, scale: AbsSize) -> AbsSize {
        if let Some(frame) = self.glyph_frames.get(&c) {
            scale * (frame.width as f32 / self.units_per_em())
        } else if c == ' ' {
            self.ascent(scale) * 0.6
        } else {
            self.advance_width('?', scale)
        }
    }

    fn left_side_bearing(&self, _c: char, _scale: AbsSize) -> AbsSize {
        AbsSize::zero()
    }

    fn pair_kerning(&self, _a: char, _b: char, _scale: AbsSize) -> AbsSize {
        AbsSize::zero()
    }

    fn exact_bounding_box(
        &self,
        c: char,
        scale: AbsSize,
    ) -> ((AbsSize, AbsSize), (AbsSize, AbsSize)) {
        self.pixel_bounding_box(c, scale)
    }

    fn pixel_bounding_box(
        &self,
        c: char,
        scale: AbsSize,
    ) -> ((AbsSize, AbsSize), (AbsSize, AbsSize)) {
        if self.glyph_frames.contains_key(&c) || c == ' ' {
            let ascent = self.ascent(scale);
            let advance = self.advance_width(c, scale);
            (
                (AbsSize::zero(), AbsSize::zero()),
                (advance.round(), ascent.round()),
            )
        } else {
            self.pixel_bounding_box('?', scale)
        }
    }

    // rendering
    fn render_glyph(&self, c: char, scale: AbsSize) -> GrayImage {
        // Note: Rendering is done via pic or x86 assembly, so we can't really scale effectively.
        //       Instead we set up the above numbers so that upscaling works upscale well.
        if let Some(frame) = self.glyph_frames.get(&c) {
            let src = self
                .glyphs
                .view(frame.x_offset, 0, frame.width as u32, self.height);
            let mut out = GrayImage::from_pixel(frame.width as u32, self.height, Luma([0]));
            out.copy_from(&src, 0, 0).unwrap();
            out
        } else if c == ' ' {
            GrayImage::new(1, 1)
        } else {
            self.render_glyph('?', scale)
        }
    }
}

impl FntFont {
    pub fn from_fnt(fnt: &Fnt) -> Result<Arc<RwLock<dyn FontInterface>>> {
        trace!("GlyphCacheFNT::new");

        let mut width = 0;
        for glyph_index in 0..=255 {
            if !fnt.glyphs.contains_key(&glyph_index) {
                continue;
            }
            width += fnt.glyphs[&glyph_index].width;
        }
        width = Gpu::stride_for_row_size(width as u32) as i32;

        let buf = GrayImage::from_pixel(width as u32, fnt.height as u32, Luma([0]));

        // FIXME: move all of this to FNT code.
        let mut interp = Interpreter::new();
        interp.add_trampoline(0x60_0000, "finish", 0);
        interp.set_register_value(Reg::EAX, 0xFF_FF_FF_FF);
        interp.set_register_value(Reg::ECX, width as u32);
        interp.set_register_value(Reg::EDI, 0x30_0000);
        interp.map_writable(0x30_0000, buf.into_raw())?;

        let mut x = 0i32;
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
                    x_offset: x as u32,
                    width: glyph.width,
                    // s0: x as f32 / width as f32,
                    // s1: (x + glyph.width) as f32 / width as f32,
                    // advance_width: glyph.width as f32 / SCREEN_SCALE[0],
                    // left_side_bearing: 0f32,
                },
            );
            x += glyph.width;
            interp.set_register_value(Reg::EDI, 0x30_0000 + x as u32);
        }

        let plane = interp.unmap_writable(0x30_0000)?;
        let buf =
            GrayImage::from_raw(width as u32, fnt.height as u32, plane).expect("same parameters");

        /*
        let texture_view = upload_texture_luma("fnt-face-texture-view", buf, gpu)?;
        let sampler = gpu.device().create_sampler(&wgpu::SamplerDescriptor {
            label: Some("fnt-face-sampler"),
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
        });

        Ok(Box::new(Self {
            texture_view,
            sampler,
            glyph_frames,
            render_height: fnt.height as f32 / SCREEN_SCALE[1],
        }) as Box<dyn FontInterface>)
         */
        Ok(Arc::new(RwLock::new(Self {
            glyphs: buf,
            glyph_frames,
            height: fnt.height as u32,
        })))
    }
}
