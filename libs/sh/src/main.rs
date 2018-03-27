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
extern crate clap;
extern crate reverse;
extern crate sh;

use std::fs;
use std::io::prelude::*;
use clap::{Arg, App};
use sh::{Instr, CpuShape};

fn main() {
    let matches = App::new("OpenFA shape tool")
        .version("0.0.1")
        .author("Terrence Cole <terrence.d.cole@gmail.com>")
        .about("Slice up shape data for digestion.")
        .arg(Arg::with_name("all")
            .long("--all")
            .short("-a")
            .takes_value(false)
            .required(false))
        .arg(Arg::with_name("INPUT")
            .help("The shape(s) to show")
            .multiple(true)
            .required(true))
        .get_matches();

    for name in matches.values_of("INPUT").unwrap() {
        let mut fp = fs::File::open(name).unwrap();
        let mut data = Vec::new();
        fp.read_to_end(&mut data).unwrap();

        let shape = CpuShape::new(&data).unwrap();
        for (i, instr) in shape.instrs.iter().enumerate() {
            if matches.is_present("all") {
                println!("{}: {}", i, instr.show());
                continue;
            }
            match instr {
//                &Instr::X86Code(ref bc) => {
//                    let filename = format!("/tmp/instr{}.bin", i);
//                    let mut buffer = fs::File::create(filename).unwrap();
//                    buffer.write(&bc.code).unwrap();
//                }
//                &Instr::UnkJumpIfLowDetail(ref x) => {
//                    let next_instr = find_instr_at_offset(x.next_offset(), &shape.instrs);
//                    println!("{}, {}: {}", name, instr.show(),
//                             next_instr.map(|i| { i.show() }).unwrap_or("NONE".to_owned()));
//                }
//                &Instr::UnkJumpIfNotShown(ref x) => {
//                    let next_instr = find_instr_at_offset(x.next_offset(), &shape.instrs);
//                    println!("{}, {}: {}", name, instr.show(),
//                             next_instr.map(|i| { i.show() }).unwrap_or("NONE".to_owned()));
//                }
                &Instr::TrailerUnknown(ref x) => {
                    if x.data[0] == 0xEE {

                        println!("{:25}: {}", name, instr.show());
                    }
                }
                _ => {}
            }
        }
    }
}

fn find_instr_at_offset(offset: usize, instrs: &[Instr]) -> Option<&Instr> {
    for instr in instrs.iter() {
        if instr.at_offset() == offset {
            return Some(instr);
        }
    }
    return None;
}
