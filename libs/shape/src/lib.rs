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
#[macro_use]
extern crate error_chain;
extern crate peff;
extern crate ansi;

mod errors {
    error_chain!{}
}
use errors::{Error, ErrorKind, Result, ResultExt};

use std::{cmp, mem, str};
use std::collections::HashSet;
use ansi::{Span, Color};

pub struct Shape {
    pub vertices: Vec<[u16; 3]>
}

fn format_hex_bytes(offset: usize, buf: &[u8]) -> String {
    let mut out = Vec::new();
    for (i, &b) in buf.iter().enumerate() {
        out.push(format!("{:02X} ", b));
//        if (offset + i + 1) % 16 == 0 {
//            out.push(" ".to_owned());
//        }
    }
    return out.drain(..).collect::<String>();
}

impl Shape {
    fn read_name(n: &[u8]) -> Result<String> {
        let end_offset: usize = n.iter().position(|&c| c == 0).unwrap();
        return Ok(str::from_utf8(&n[..end_offset]).chain_err(|| "names should be utf8 encoded")?.to_owned());
    }

    // Read a 16 bit prefix and then read that many byres.
    fn read_prefix16_bytes(buf: &[u8]) -> Result<&[u8]> {
        let header_buf: &[u16] = unsafe { mem::transmute(buf) };
        let header = header_buf[0] as usize;
        if header + 2 > buf.len() {
            bail!("too short");
        }
        return Ok(&buf[2..2 + header]);
    }

    pub fn code(buf: &[u8], offset: usize, len: usize, c: Color) -> String {
        let rng = &buf[offset..(offset + len)];
        let hd = Span::new(&format_hex_bytes(offset, &rng[0..2])).background(c);
        let rm = Span::new(&format_hex_bytes(offset + 2, &rng[2..])).foreground(c);
        return format!("{}{}", hd.format(), rm.format());
    }

    pub fn code_ellipsize(buf: &[u8], offset: usize, len: usize, cut_to: usize, c: Color) -> String {
        assert!(len > cut_to);
        assert!(cut_to > 4);
        assert_eq!(cut_to % 2, 0);
        let h = cut_to / 2;
        let rngl = &buf[offset..offset + h];
        let rngr = &buf[offset + len - h..offset + len];
        let hd = Span::new(&format_hex_bytes(offset, &rngl[0..2])).background(c);
        let l = Span::new(&format_hex_bytes(0, &rngl[2..])).foreground(c);
        let r = Span::new(&format_hex_bytes(0, rngr)).foreground(c);
        return format!("{}{}{}", hd.format(), l.format(), r.format());
    }

    pub fn data(buf: &[u8], offset: usize, len: usize, c: Color) -> String {
        let rng = &buf[offset..(offset + len)];
        let rm = Span::new(&format_hex_bytes(offset, &rng[0..])).foreground(c);
        return rm.format();
    }

    pub fn new(path: &str, data: &[u8]) -> Result<(Vec<[i16;3]>, String)> {
        let mut verts = Vec::new();

        let pe = peff::PE::parse(data).chain_err(|| "parse pe")?;
        let mut offset = 0;
        let mut cnt = 0;
        let mut out = "".to_owned();
        let show = false;
        let mut pics = Vec::new();
        let mut unk1 = 0;

        loop {
            let code: &[u16] = unsafe { mem::transmute(&pe.code[offset..]) };
            if code[0] == 0xFFFF {
                if show {
                    out += &Self::code(&pe.code, offset, 14, Color::Blue);
                }
                offset += 14;
            } else if code[0] == 0x00F2 {
                if show {
                    out += &Self::code(&pe.code, offset, 4, Color::Purple);
                }
                offset += 4;
            } else if code[0] == 0x00DA {
                // DA 00 00 00
                if show {
                    out += &Self::code(&pe.code, offset, 4, Color::Purple);
                }
                offset += 4;
            } else if code[0] == 0x00CA {
                // CA 00 00 00
                if show {
                    out += &Self::code(&pe.code, offset, 4, Color::Cyan);
                }
                offset += 4;
            } else if code[0] == 0x00B8 {
                // B8 00 01 00
                if show {
                    out += &Self::code(&pe.code, offset, 4, Color::Cyan);
                }
                offset += 4;
            } else if code[0] == 0x0042 {
                let s = Self::read_name(&pe.code[offset + 2..]).unwrap();
                if show {
                    out += &Self::code(&pe.code, offset, 2 + s.len() + 1, Color::Yellow);
                }
                offset += 2 + s.len() + 1;
            } else if code[0] == 0x00E2 {
                // E2 00  5F 7A 73 75 35 37 2E 50 49 43 00 00 00 00
                pics.push(Self::read_name(&pe.code[offset + 2..]).unwrap());
                if show {
                    out += &Self::code(&pe.code, offset, 16, Color::Yellow);
                }
                offset += 16;
            } else if code[0] == 0x007A {
                if show {
                    out += &Self::code(&pe.code, offset, 10, Color::Green);
                }
                offset += 10;
            } else if code[0] == 0x00CE {
                //CE 00  00 5E  00 00 00 0D  00 00 00 11  00 00 00 00  00 AC FF FF  00 AC FF FF  00 22  00 00 00 54  00 00  00 AC FF FF  00 22  00 00
                if show {
                    out += &Self::code(&pe.code, offset, 40, Color::Cyan);
                }
                offset += 40;
            } else if code[0] == 0x0078 {
                // 78 00 00 00 BC 01 82 00 90 01 00 00
                if show {
                    out += &Self::code(&pe.code, offset, 12, Color::Blue);
                }
                offset += 12;
            } else if code[0] == 0x00C8 {
                // C8 00 E6 00 10 00 33 11
                // C8 00 E6 00 21 00 61 0F
                if show {
                    out += &Self::code(&pe.code, offset, 8, Color::Purple);
                }
                offset += 8;
            } else if code[0] == 0x00A6 {
                // A6 00 5B 0F 01 00
                if show {
                    out += &Self::code(&pe.code, offset, 6, Color::Cyan);
                }
                offset += 6;
            } else if code[0] == 0x0082 {
                let n_coords = code[1] as usize;
                //let unused = code[2];
                offset += 4;
                let sz = 3;
                //if show {
                    let length = 2 + 2 + n_coords * sz;
                    out += &Self::code_ellipsize(&pe.code, offset - 4, length, 18, Color::Green);
                //}
                for i in 0..n_coords {
                    verts.push([code[offset + 0] as i16, code[offset + 1] as i16, code[offset + 2] as i16]);
                    offset += sz;
                }
            } else {
                for &reloc in pe.relocs.iter() {
                    let base = reloc as i64 - offset as i64;
                    if base >= 4 {
                        out += &Self::data(&pe.code, offset, base as usize, Color::White);
                        out += &Self::data(&pe.code, offset + base as usize, 4, Color::Red);
                        offset += base as usize + 4;
                    }
                }
                let remainder = cmp::min(1500, pe.code.len() - offset);
                out += &format_hex_bytes(offset, &pe.code[offset..offset+remainder]);
//                let buffer = &pe.code[offset..offset + remainder];
//                let fmt = format_hex_bytes(offset, buffer);


                //out += &;
                out += "... - ";
                out += path;
                break;
            }
            cnt += 1;
        }

        // Ensure we still haven't hit any relocs.
//        for &reloc in pe.relocs.iter() {
//            assert!(reloc > offset as u32);
//        }

        let mut num_thunk = 0;
        if let Some(thunks) = pe.thunks.clone() {
            num_thunk = thunks.len();
        }

        let is_s = if path.contains("_S.SH") { "t" } else { "f" };
        return Ok((verts, format!("{:04X}| {} => {:?}", unk1, out, pe.thunks)));


//        let header_ptr: *const Header = pe.code.as_ptr() as *const Header;
//        let header: &Header = unsafe { &*header_ptr };
//        let header_span = Span::new(&format_hex_bytes(0, &pe.code[0..mem::size_of::<Header>()])).foreground(Color::Blue);
//        offset += mem::size_of::<Header>();
//
//        let source = Self::read_name(&pe.code[offset..]).unwrap();
//        let source_span = Span::new(&format_hex_bytes(offset, &pe.code[offset..(offset + source.len() + 1)])).foreground(Color::Yellow);
//        offset += source.len() + 1;
//
//        let rem = &pe.code[offset..];
//        return Ok(format!("{:02}| {}{}{} - {}", source.len(),
//                          header_span.format(),
//                          source_span.format(),
//                          format_hex_bytes(offset, &rem[0..50]),
//                          path));
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::prelude::*;
    use super::*;

    #[test]
    fn it_works() {
        let mut rv: Vec<String> = Vec::new();
        let paths = fs::read_dir("./test_data").unwrap();
        for i in paths {
            let entry = i.unwrap();
            let path = format!("{}", entry.path().display());
            //println!("AT: {}", path);

            //if path == "./test_data/MIG21.SH" {
            if true {

                let mut fp = fs::File::open(entry.path()).unwrap();
                let mut data = Vec::new();
                fp.read_to_end(&mut data).unwrap();

                let (_verts, desc) = Shape::new(&path, &data).unwrap();
                rv.push(desc);

            }

            //assert_eq!(format!("./test_data/{}", t.object.file_name), path);
            //rv.push(format!("{:?} <> {} <> {}",
            //                t.object.unk_explosion_type,
            //                t.object.long_name, path));
        }
        rv.sort();

        let mut set = HashSet::new();
        for v in rv {
            set.insert((&v[0..2]).to_owned());
            println!("{}", v);
        }
        println!("{:?}", set);
    }
}
