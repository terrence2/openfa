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
#![allow(clippy::transmute_ptr_to_ptr)]

use codepage_437::{FromCp437, CP437_CONTROL};
use failure::{bail, ensure, Fallible};
use i386::{ByteCode, Interpreter, Reg};
use image::{ImageBuffer, LumaA};
use peff::PE;
use std::{collections::HashMap, mem};

// Save chars to png when testing.
const DUMP_CHARS: bool = false;

// Note: a list of all FNT related resources. It's not yet clear how these pieces fit
// together for all scenarios yet. I believe that the PIC's below hold blending data
// suitable for several fonts in a certain color against various background.
const _FONT_FILES: [&str; 12] = [
    "4X12.FNT",
    "4X6.FNT",
    "HUD00.FNT",
    "HUD01.FNT",
    "HUD11.FNT",
    "HUDSYM00.FNT",
    "HUDSYM01.FNT",
    "HUDSYM11.FNT",
    "MAPFONT.FNT",
    "WIN00.FNT",
    "WIN01.FNT",
    "WIN11.FNT",
];

const _FONT_BACKGROUNDS: [&str; 21] = [
    "ARMFONT.PIC",
    "BODYFONT.PIC",
    "BOLDFONT.PIC",
    "FNTWPNB.PIC",
    "FNTWPNY.PIC",
    "FONT4X6.PIC",
    "FONTACD.PIC",
    "FONTACT.PIC",
    "FONTDFD.PIC",
    "FONTDFT.PIC",
    "HEADFONT.PIC",
    "LRGFONT.PIC",
    "MAPFONT.PIC",
    "MENUFONT.PIC",
    "MFONT320.PIC",
    "MPFONT.PIC",
    "PANELFNT.PIC",
    "PANLFNT2.PIC",
    "SMLFONT.PIC",
    "VIDEOFNT.PIC",
    "WHEELFNT.PIC",
];

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum Font {
    HUD11,
}

impl Font {
    pub fn name(&self) -> &'static str {
        match self {
            Self::HUD11 => "hud11",
        }
    }
}

pub struct GlyphInfo {
    pub glyph_index: u8,
    pub glyph_char: String,
    pub width: i32,
    pub bytecode: ByteCode,
}

pub struct Fnt {
    pub height: usize,
    pub glyphs: HashMap<u8, GlyphInfo>,
}

const FNT_LOAD_BASE: u32 = 0x0000_0000;

impl Fnt {
    pub fn from_bytes(bytes: &[u8]) -> Fallible<Self> {
        let mut pe = PE::from_bytes(bytes)?;
        pe.relocate(FNT_LOAD_BASE)?;

        let dwords: &[u32] = unsafe { mem::transmute(&pe.code[0..1028]) };
        let height = dwords[0] as usize;

        let mut glyphs = HashMap::new();
        for i in 1..256 {
            let target = dwords[i] as usize;
            ensure!(target < pe.code.len(), "out of bounds");
            let next = if i < 255 {
                dwords[i + 1] as usize
            } else {
                pe.code.len() - 10
            };
            ensure!(next <= pe.code.len(), "next out of bounds");
            ensure!(pe.code[next - 1] == 0xC3, "expected to end in C3");

            let span = &pe.code[target..next];
            if span.len() == 1 {
                // No glyph at index
                continue;
            }

            let glyph_index = (i - 1) as u8;
            let glyph_char = String::from_cp437(vec![glyph_index], &CP437_CONTROL);

            let maybe_bytecode = ByteCode::disassemble_until(0, span, |_| false);
            if let Err(e) = maybe_bytecode {
                i386::DisassemblyError::maybe_show(&e, &span);
                bail!("Don't know how to disassemble at {}: {:?}", 0, e);
            }
            let bytecode = maybe_bytecode?;

            // Compute glyph width by observing the rightmost write
            let mut width = 0;
            for instr in &bytecode.instrs {
                for operand in &instr.operands {
                    if let i386::Operand::Memory(memref) = operand {
                        let offset = memref.displacement + i32::from(memref.size) - 1;
                        width = width.max(offset);
                    }
                }
            }
            width += 2;

            glyphs.insert(
                glyph_index,
                GlyphInfo {
                    glyph_index,
                    glyph_char,
                    width,
                    bytecode,
                },
            );
        }

        Ok(Self { height, glyphs })
    }

    pub fn analyze(&self, game: &str, name: &str) -> Fallible<()> {
        const WIDTH: usize = 0x20;

        for glyph_index in 0..=255 {
            if !self.glyphs.contains_key(&glyph_index) {
                continue;
            }
            let glyph = &self.glyphs[&glyph_index];
            println!("{:<2} - {:04X}:", glyph.glyph_char, glyph.glyph_index);
            println!("{}", glyph.bytecode.show_relative(0));

            {
                let mut interp = Interpreter::new();
                interp.add_code(glyph.bytecode.clone());
                interp.push_stack_value(0x60_0000);
                interp.add_trampoline(0x60_0000, "finish", 0);
                interp.set_register_value(Reg::EAX, 0xFFFF_FFFF);
                interp.set_register_value(Reg::ECX, WIDTH as u32);

                let mut edi_map = Vec::with_capacity(WIDTH * self.height);
                edi_map.resize(WIDTH * self.height + 4, 0x00);
                interp.set_register_value(Reg::EDI, 0x30_0000);
                interp.map_writable(0x30_0000, edi_map)?;

                let rv = interp.interpret(0)?;
                let (trampoline_name, args) = rv.ok_trampoline()?;
                ensure!(trampoline_name == "finish", "expect return to finish");
                ensure!(args.is_empty(), "expect no args out");

                let mut buf = ImageBuffer::new(WIDTH as u32, self.height as u32);
                let mut edi_map = interp.unmap_writable(0x30_0000)?;
                edi_map.truncate(WIDTH * self.height);
                for (i, v) in edi_map.iter().enumerate() {
                    //println!("{} => {}x{}", i, i % WIDTH, i / WIDTH);
                    buf.put_pixel(
                        (i % WIDTH) as u32,
                        (i / WIDTH) as u32,
                        LumaA { data: [*v, *v] },
                    );
                }
                Self::save_char(buf, game, name, glyph)?;
            }
        }
        Ok(())
    }

    fn save_char(
        buf: ImageBuffer<LumaA<u8>, Vec<u8>>,
        game: &str,
        name: &str,
        glyph: &GlyphInfo,
    ) -> Fallible<()> {
        let img = image::ImageLumaA8(buf);
        if DUMP_CHARS {
            let mut ch = glyph.glyph_char.clone();
            if ch == "/" {
                ch = format!("{}", glyph.glyph_index);
            }
            let filename = format!(
                "../../dump/fnt/{}/{}-char-{:02X}-{}.png",
                game, name, glyph.glyph_index, ch
            );
            img.save(filename)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lib::CatalogBuilder;

    #[test]
    fn it_can_parse_all_fnt_files() -> Fallible<()> {
        let (mut catalog, inputs) = CatalogBuilder::build_and_select(&["*:*.FNT".to_owned()])?;
        for &fid in &inputs {
            let label = catalog.file_label(fid)?;
            catalog.set_default_label(&label);
            let game = label.split(':').last().unwrap();
            let meta = catalog.stat_sync(fid)?;
            println!(
                "At: {}:{:13} @ {}",
                game,
                meta.name,
                meta.path
                    .unwrap_or_else(|| "<none>".into())
                    .to_string_lossy()
            );
            let fnt = Fnt::from_bytes(&catalog.read_sync(fid)?)?;
            fnt.analyze(game, &meta.name)?;
        }

        Ok(())
    }
}
