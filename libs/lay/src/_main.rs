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
#![cfg_attr(feature = "cargo-clippy", allow(transmute_ptr_to_ptr))]

use clap::{App, Arg};
use failure::{bail, Error};
use lay::Layer;
use reverse::{b2h, Color, Escape};
use std::{fs, io::Read, mem, path::Path};

fn main() -> Result<(), Error> {
    let matches = App::new("OpenFA layer tool")
        .version("0.0.1")
        .author("Terrence Cole <terrence.d.cole@gmail.com>")
        .about("Slice up LAY data for digestion.")
        .arg(
            Arg::with_name("nowrap")
                .long("--nowrap")
                .short("-w")
                .help("Wrap at 32 bytes")
                .required(false),
        )
        .arg(
            Arg::with_name("INPUT")
                .help("The layer(s) to show")
                .multiple(true)
                .required(true),
        )
        .get_matches();

    // let step = 0x40;
    // for i in 0..0x100 {
    //     print!("{:04X}", i * step);
    //     for i in 0..(step * 3 - 4) {
    //         print!(" ");
    //     }
    // }
    // println!("");

    for name in matches.values_of("INPUT").unwrap() {
        let mut fp = fs::File::open(name).unwrap();
        let mut data = Vec::new();
        fp.read_to_end(&mut data).unwrap();

        let mut pe = peff::PE::parse(&data)?;
        pe.relocate(0x0000_0000)?;

        println!("RELOCS: {:?}", pe.relocs);
        println!("THUNKS: {:?}", pe.thunks);

        let _lay = Layer::from_pe(Path::new(name).file_stem().unwrap().to_str().unwrap(), &pe, lib)?;

        let mut extents = Vec::new();
        for reloc in pe.relocs {
            extents.push((0u8, reloc, reloc + 4));
        }
        for thunk in pe.thunks {
            let pos = thunk.vaddr as u32;
            println!("POS: {:08X}", pos);
            extents.push((1u8, pos, pos + 6));
        }

        // let p = 5 * 32 + 10;
        // let p = 16 * 32 + 22 - 1;
        // let pal_data = &pe.code[p as usize..p as usize + 0x300];
        // let pal = Palette::from_bytes(&pal_data)?;
        // pal.dump_png(&format!("pal-{:04X}", p));

        let mut out = Vec::new();
        for (i, b) in pe.code.iter().enumerate() {
            if is_extent_end(i, &extents) {
                Escape::new().put(&mut out);
            }
            if !matches.is_present("no-wrap") && i % 32 == 0 {
                out.push('\n');
                for c in format!("{:04X}| ", i).chars() {
                    out.push(c);
                }
            }
            if let Some(u) = is_extent_start(i, &extents) {
                match u {
                    0 => {
                        let ptr: &[u32] = unsafe { mem::transmute(&pe.code[i..i + 4]) };
                        let p = ptr[0];
                        extents.push((2, p + 4, p + 0x100));
                        extents.push((3, p, p + 4));
                        // let pal_data = &pe.code[p as usize..p as usize + 0x300];
                        // let pal = Palette::from_bytes(&pal_data)?;
                        // pal.dump_png(&format!("pal-{:04X}", p));
                        Escape::new().fg(Color::Red).put(&mut out)
                    }
                    1 => Escape::new().fg(Color::Blue).put(&mut out),
                    2 => Escape::new().fg(Color::Cyan).put(&mut out),
                    3 => Escape::new().bg(Color::Cyan).put(&mut out),
                    _ => bail!("unknown kind tag {}", u),
                };
            }
            b2h(*b, &mut out);
            out.push(' ');
        }
        println!("{}", out.iter().collect::<String>());
        // println!("vadder: {:04X}", pe.code_vaddr);
        // for reloc in pe.relocs {
        //     println!("reloc: {:04X}", reloc);
        // }
        // for thunk in pe.thunks {
        //     println!("thunk: {:?}", thunk);
        // }
        //println!("{:25}: {}", name, bs2s(&pe.code));
    }

    Ok(())
}

fn is_extent_start(i: usize, extents: &[(u8, u32, u32)]) -> Option<u8> {
    let mut out = None;
    for &(kind, st, _) in extents.iter() {
        if i as u32 == st {
            if let Some(k) = out {
                if k > kind {
                    out = Some(kind);
                }
            } else {
                out = Some(kind);
            }
        }
    }
    out
}

fn is_extent_end(i: usize, extents: &[(u8, u32, u32)]) -> bool {
    for &(_, _, ed) in extents.iter() {
        if i as u32 == ed {
            return true;
        }
    }
    false
}
