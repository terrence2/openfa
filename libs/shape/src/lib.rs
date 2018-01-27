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

mod errors {
    error_chain!{}
}
use errors::{Error, ErrorKind, Result, ResultExt};

use std::{mem, str};

pub struct Shape {
    pub vertices: Vec<[u16; 3]>
}

pub struct Header {
    pub magic: u16,
    pub unk1: u16,
    pub unk2: u16,
    pub unk3: u16,
    pub unk4: u16,
    pub unk5: u16,
    pub unk6: u16,
    pub unk7: u16,
    pub unk8: u16,
    pub unk9: u16
}

fn format_hex_bytes(buf: &[u8]) -> String {
    let mut out = Vec::new();
    for (i, &b) in buf.iter().enumerate() {
        out.push(format!("{:02X} ", b));
        if (i + 1) % 16 == 0 {
            out.push(" ".to_owned());
        }
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

    pub fn new(path: &str, data: &[u8]) -> Result<String> {
//        let mut vertices = Vec::new();

        let pe = peff::PE::parse(data).chain_err(|| "parse pe")?;
        let mut offset = 0;

        let header_ptr: *const Header = pe.code.as_ptr() as *const Header;
        let header: &Header = unsafe { &*header_ptr };
        offset += mem::size_of::<Header>();

        let source = Self::read_name(&pe.code[offset..]).unwrap();
        offset += source.len() + 1;

        let rem = &pe.code[offset..];
        //return Ok(format!("{:04X}| {} - {} - {}", header.unk3, format_hex_bytes(&rem[0..50]), source, path));
        return Ok(format!("{} - {} - {}", format_hex_bytes(&rem[0..50]), source, path));



        //if pe.code[14] != 0xf2 {
//            println!("{:?} - {} - {}", pe.relocs, format_hex_bytes(&pe.code[0..20]), path);
        //}
//        if pe.thunks.is_some() {
//            println!("{}", path);
//        }
        //println!("{}", pe.relocs.first().unwrap_or(&0u32));

//        let words: &[u16] = unsafe { mem::transmute(&pe.code[0..]) };
//        assert!(words[0] == 0xFFFF);
//        vertices.push([words[1], words[2], words[3]]);
//        vertices.push([words[4], words[5], words[6]]);


//        //println!("{} - {}", pe.code.len(), path);
//        //println!("{:?}", pe.code);
//        let bytes: &[u8] = &pe.code;
//        let mut ip = 0usize;
//        while ip < bytes.len() {
//            let word: u16 = unsafe { *(bytes[ip..].as_ptr() as *const u16) };
//            match word {
//                0x82 => {
//                    ip += 2;
//                    let dwords: &[u32] = unsafe { mem::transmute(&bytes[ip..]) };
//                    let cnt = dwords[0];
//                    ip += 4;
//                    println!("found {} verts at offset {} in {}, prefix {:?}", cnt, ip * 2, path, &pe.code[0..ip*2]);
//                    let words: &[u16] = unsafe { mem::transmute(&bytes[ip..]) };
//                    for i in 0..cnt {
//                        let x = words[ip];
//                        let y = words[ip + 1];
//                        let z = words[ip + 2];
//                        ip += 3;
//                        vertices.push([x, y, z]);
//                    }
//                    break;
//                },
//                _ => ip += 1
//            }
//        }

//        return Ok(Shape{ vertices });
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
            println!("AT: {}", path);

            if path == "./test_data/F4J_D.SH" {

                let mut fp = fs::File::open(entry.path()).unwrap();
                let mut data = Vec::new();
                fp.read_to_end(&mut data).unwrap();

                let foo = Shape::new(&path, &data).unwrap();
                rv.push(foo);

            }

            //assert_eq!(format!("./test_data/{}", t.object.file_name), path);
            //rv.push(format!("{:?} <> {} <> {}",
            //                t.object.unk_explosion_type,
            //                t.object.long_name, path));
        }
        rv.sort();
        for v in rv {
            println!("{}", v);
        }
    }
}
