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
extern crate i386;
extern crate reverse;
extern crate sh;

use clap::{App, Arg};
use sh::{CpuShape, Instr};
use std::collections::HashMap;
use std::fs;
use std::io::prelude::*;

fn main() {
    let matches = App::new("OpenFA shape tool")
        .version("0.0.1")
        .author("Terrence Cole <terrence.d.cole@gmail.com>")
        .about("Slice up shape data for digestion.")
        .arg(
            Arg::with_name("all")
                .long("--all")
                .takes_value(false)
                .required(false)
                .conflicts_with_all(&["last"]),
        )
        .arg(
            Arg::with_name("last")
                .long("--last")
                .takes_value(false)
                .required(false),
        )
        .arg(
            Arg::with_name("unknown")
                .long("--unknown")
                .takes_value(false)
                .required(false),
        )
        .arg(
            Arg::with_name("memory")
                .long("--memory")
                .takes_value(false)
                .required(false),
        )
        .arg(
            Arg::with_name("INPUT")
                .help("The shape(s) to show")
                .multiple(true)
                .required(true),
        )
        .get_matches();

    for name in matches.values_of("INPUT").unwrap() {
        let mut fp = fs::File::open(name).unwrap();
        let mut data = Vec::new();
        fp.read_to_end(&mut data).unwrap();
        println!("At: {}", name);

        let shape = CpuShape::new(&data).unwrap();

        if matches.is_present("all") {
            for (i, instr) in shape.instrs.iter().enumerate() {
                println!("{}: {}", i, instr.show());
            }
        } else if matches.is_present("last") {
            let fmt = shape
                .instrs
                .last()
                .map(|i| i.show())
                .ok_or("NO INSTRUCTIONS")
                .unwrap();
            println!("{:20}: {}", name, fmt);
        } else if matches.is_present("unknown") {
            for i in shape.instrs.iter() {
                if let sh::Instr::UnknownUnknown(u) = i {
                    println!("{:20}: {}", name, i.show());
                }
            }
        } else if matches.is_present("memory") {
            let mut dedup = HashMap::new();
            for vinstr in shape.instrs {
                if let sh::Instr::X86Code(x86) = vinstr {
                    for instr in x86.bytecode.instrs {
                        for operand in instr.operands {
                            if let i386::Operand::Memory(memref) = operand {
                                let key = format!("{}", memref);
                                *dedup.entry(key).or_insert(0) += 1;
                            }
                        }
                    }
                }
            }
            let mut memrefs = dedup.keys().map(|s| s.to_owned()).collect::<Vec<String>>();
            memrefs.sort();
            for memref in memrefs.iter() {
                println!("{} - {}", dedup[memref], memref);
            }
        }

        //        for (i, instr) in shape.instrs.iter().enumerate() {
        //            if matches.is_present("all") {
        //                println!("{}: {}", i, instr.show());
        //                continue;
        //            }
        //            match instr {
        ////                &Instr::X86Code(ref bc) => {
        ////                    let filename = format!("/tmp/instr{}.bin", i);
        ////                    let mut buffer = fs::File::create(filename).unwrap();
        ////                    buffer.write(&bc.code).unwrap();
        ////                }
        ////                &Instr::UnkJumpIfLowDetail(ref x) => {
        ////                    let next_instr = find_instr_at_offset(x.next_offset(), &shape.instrs);
        ////                    println!("{}, {}: {}", name, instr.show(),
        ////                             next_instr.map(|i| { i.show() }).unwrap_or("NONE".to_owned()));
        ////                }
        ////                &Instr::UnkJumpIfNotShown(ref x) => {
        ////                    let next_instr = find_instr_at_offset(x.next_offset(), &shape.instrs);
        ////                    println!("{}, {}: {}", name, instr.show(),
        ////                             next_instr.map(|i| { i.show() }).unwrap_or("NONE".to_owned()));
        ////                }
        //                &Instr::TrailerUnknown(ref x) => {
        //                    if x.data[0] == 0xEE {
        //
        //                        println!("{:25}: {}", name, instr.show());
        //                    }
        //                }
        //                _ => {}
        //            }
        //        }
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
