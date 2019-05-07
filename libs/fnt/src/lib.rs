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
use ansi::ansi;
use codepage_437::{BorrowFromCp437, FromCp437, CP437_CONTROL};
use failure::{bail, ensure, Fallible};
use i386::{ByteCode, Interpreter, Reg};
use image::LumaA;
use peff::PE;
use reverse::bs2s;
use std::{collections::HashMap, mem};

pub struct GlyphInfo {
    pub glyph_index: u8,
    pub glyph_char: String,
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
            if span.is_empty() {
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

            glyphs.insert(
                glyph_index,
                GlyphInfo {
                    glyph_index,
                    glyph_char,
                    bytecode,
                },
            );
        }

        Ok(Self { height, glyphs })
    }

    pub fn analyze(&self, game: &str, name: &str) -> Fallible<()> {
        for glyph_index in 0..=255 {
            if !self.glyphs.contains_key(&glyph_index) {
                continue;
            }
            let glyph = &self.glyphs[&glyph_index];
            println!("{:<2} - {:04X}:", glyph.glyph_char, glyph.glyph_index);
            println!("{}", glyph.bytecode.show_relative(0));

            let mut buf = image::ImageBuffer::new(20u32, self.height as u32);
            for x in 0..20 {
                for y in 0..self.height {
                    buf.put_pixel(x, y as u32, image::LumaA { data: [0, 0] });
                }
            }

            let mut interp = Interpreter::new();
            interp.add_code(&glyph.bytecode);
            interp.push_stack_value(0x60_0000);
            interp.add_trampoline(0x60_0000, "finish", 0);
            interp.set_register_value(Reg::BH, 0x40_0000);
            interp.add_write_port(
                0x40_0000,
                Box::new(move |v| {
                    println!("PUT {} to 0", v);
                    // buf.put_pixel(
                    //     0,
                    //     0,
                    //     LumaA {
                    //         data: [v as u8, 255],
                    //     },
                    // );
                }),
            );
            let rv = interp.interpret(0)?;
            let (trampoline_name, args) = rv.ok_trampoline()?;
            ensure!(trampoline_name == "finish", "expect return to finish");
            ensure!(args.is_empty(), "expect no args out");

            let img = image::ImageLumaA8(buf);
            let mut ch = glyph.glyph_char.clone();
            if ch == "/" {
                ch = format!("{}", glyph_index);
            }
            let filename = format!(
                "../../dump/fnt/{}/{}-char-{:02X}-{}.png",
                game, name, glyph_index, ch
            );
        }
        Ok(())
    }

    /*
    fn make_bitmap(c: u8) -> String {
        let mut bitmap = String::new();
        let mut b = c;
        for _ in 0..8 {
            bitmap += if b & 0b1000_0000 > 0 { "*" } else { " " };
            b <<= 1;
        }
        bitmap
    }

    pub fn analyze(&self, _game: &str, _name: &str) -> Fallible<()> {
        for glyph_index in 0..=255 {
            if !self.glyphs.contains_key(&glyph_index) {
                continue;
            }
            let glyph = &self.glyphs[&glyph_index];
            println!("{:<2} - {:04X}:", glyph.glyph_char, glyph.glyph_index);

            for row in &glyph.rows {
                if row.is_empty() {
                    println!("    |");
                    continue;
                }
                ensure!(row.len() >= 2, "at least 2 bytes");

                let row = if row[0] != 0x66 {
                    let mut tmp = vec![];
                    tmp.push(0);
                    for a in row {
                        tmp.push(*a);
                    }
                    tmp
                } else {
                    row.clone()
                };

                let mut bitmap0 = String::new();
                bitmap0 += &(Self::make_bitmap(row[0]) + "|");
                bitmap0 += &(Self::make_bitmap(row[1]) + "|  ");
                //bitmap0 += "|  ";

                let mut bitmap2 = String::new();
                for c in &row[2..] {
                    bitmap2 += &Self::make_bitmap(*c);
                }
                println!("    |{:<20} ({:<50} - {}", bitmap0, bitmap2, bs2s(&row));

                // ensure!(
                //     row[0] == 0x66 || row[0] == 0x88 || row[0] == 0x89,
                //     "66, 88, or 89"
                // );
                //ensure!(row[1] & 1 == 1, "last bit of second byte set");
            }
        }

        /*
        let mut out = String::new();
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

            let span = &pe.code[target..next - 1];
            if span.is_empty() {
                continue;
            }
            for &c in span {
                ensure!(c != 0xC3, "no C3 in the middle");
            }
            let mut buf = image::ImageBuffer::new(20u32, height);
            for x in 0..20 {
                for y in 0..height {
                    buf.put_pixel(x, y, image::LumaA { data: [0, 0] });
                }
            }
            let mut y_off = 0;
            let mut x_off = 0;

            println!("{:2>} - {:04X}:", char_in_unicode, i - 1);

            //let a = bs2s(span).trim_end().to_owned();
            let mut a = String::new();
            let mut bitmap = String::new();
            let mut j = 0;
            while j < span.len() {
                if j == 0 {
                    ensure!(span[0] != 0x7, "no 7 at start?");
                    ensure!(span[0] != 0x47, "no 47 at start?");
                }

                if span[j] != 3 {
                    let mut b = span[j];
                    for _ in 0..8 {
                        bitmap += if b & 0b1000_0000 > 0 { "*" } else { " " };
                        b <<= 1;
                    }
                }

                if span[j] == 0x88 {
                    a += &format!("{}{}{} ", ansi().yellow(), "\u{2B22}", ansi());
                    j += 1;
                } else if span[j] == 0x66 {
                    ensure!(span[j + 1] == 0x89, "expect 89 after 66");
                    a += &format!("{}{}{} ", ansi().green().dimmed(), "\u{2B22}", ansi());
                    j += 2;
                } else if span[j] == 0x89 {
                    a += &format!("{}{}{} ", ansi().magenta(), "\u{2B22}", ansi());
                    j += 1;
                } else if span[j] == 0x7 {
                    a += &format!("{}{}{} ", ansi().cyan(), "\u{2977}", ansi());
                    j += 1;
                } else if span[j] == 0x47 {
                    a += &format!("{}{}{} ", ansi().magenta(), "\u{2975}", ansi());
                    j += 1;
                } else if j < span.len() - 1 && span[j] == 0x03 && span[j + 1] == 0xF9 {
                    y_off += 1;
                    x_off = 0;
                    a += &format!("{}{}{}  ", ansi().red().bright(), "\u{2938}", ansi());
                    println!("    {:<80} | {}", bitmap, a);
                    a = String::new();
                    bitmap = String::new();
                    j += 2;
                } else {
                    let v = span[j];
                    a += &bs2s(&span[j..=j]);
                    j += 1;
                    //buf.put_pixel(x_off, y_off, image::LumaA { data: [v, 0xFF] });
                    x_off += 1;
                }
            }


            ensure!(y_off == height, "expected exactly {} lines", height);

            let img = image::ImageLumaA8(buf);
            if char_in_unicode == "/" {
                char_in_unicode = format!("{}", rawchar);
            }
            let filename = format!(
                "../../dump/fnt/{}/{}-char-{:02X}-{}.png",
                game, name, rawchar, char_in_unicode
            );
            //img.save(filename)?;
            //println!("{}", bs2s(&pe.code[target..target + 8]));
            out = String::new();
        }

        //println!("{:?}", pe.relocs);
        //println!("{}", bs2s(&pe.code));
        //println!("{:02}: {}", height, out);
        */

        Ok(())
    }
    */
}

#[cfg(test)]
mod tests {
    use super::*;
    use failure::Error;
    use omnilib::OmniLib;

    #[test]
    fn it_can_parse_all_fnt_files() -> Fallible<()> {
        assert_eq!(2 + 2, 4);

        let omni = OmniLib::new_for_test_in_games(&[
            "USNF", "MF", "ATF", "ATFNATO", "ATFGOLD", "USNF97", "FA",
        ])?;
        for (game, name) in omni.find_matching("*.FNT")?.iter() {
            // if name != "MAPFONT.FNT" {
            //     continue;
            // }
            println!(
                "At: {}:{:13} @ {}",
                game,
                name,
                omni.path(game, name)
                    .or_else::<Error, _>(|_| Ok("<none>".to_string()))?
            );

            let lib = omni.library(game);
            let fnt = Fnt::from_bytes(&lib.load(name)?)?;
            fnt.analyze(game, name)?;
        }

        Ok(())
    }
}
