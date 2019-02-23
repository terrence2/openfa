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
use failure::Fallible;
use reverse::b2h;
use sh::{CpuShape, Instr};
use simplelog::*;
use std::io::prelude::*;
use std::{collections::HashMap, fs, path::PathBuf};
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(name = "OpenFA shape slicing tool")]
struct Opt {
    /// Trace execution
    #[structopt(short = "v", long = "verbose")]
    verbose: bool,

    /// Show all
    #[structopt(short = "a", long = "all")]
    show_all: bool,

    /// Show matching instructions
    #[structopt(short = "m", long = "matching")]
    show_matching: Option<String>,

    /// Show last instructions
    #[structopt(short = "l", long = "last")]
    show_last: bool,

    /// Show unknown instructions
    #[structopt(short = "u", long = "unknown")]
    show_unknown: bool,

    /// Show memory refs in x86
    #[structopt(short = "x", long = "memory")]
    show_memory: bool,

    /// Elide the name in output
    #[structopt(short = "n", long = "no-name")]
    quiet: bool,

    /// Files to process
    #[structopt(name = "FILE", parse(from_os_str))]
    files: Vec<PathBuf>
}

fn main() -> Fallible<()> {
    let opt = Opt::from_args();
    let level = if opt.verbose {
        LevelFilter::Trace
    } else {
        LevelFilter::Warn
    };
    TermLogger::init(level, Config::default())?;

    for name in &opt.files {
        let mut fp = fs::File::open(name).unwrap();
        let mut data = Vec::new();
        fp.read_to_end(&mut data).unwrap();
        //println!("At: {}", name);

        let shape = CpuShape::from_bytes(&data).unwrap();

        if opt.show_all {
            for (i, instr) in shape.instrs.iter().enumerate() {
                println!("{:3}: {}", i, instr.show());
            }
        } else if let Some(ref target) = opt.show_matching {
            for instr in &shape.instrs {
                if instr.magic() == target {
                    if opt.quiet {
                        println!("{}", instr.show());
                    } else {
                        println!("{:60}: {}", name.as_os_str().to_str().unwrap(), instr.show());
                    }
                }
            }
        } else if opt.show_last {
            let fmt = shape
                .instrs
                .last()
                .map(|i| i.show())
                .ok_or("NO INSTRUCTIONS")
                .unwrap();
            println!("{:20}: {}", name.as_os_str().to_str().unwrap(), fmt);
        } else if opt.show_unknown {
            for i in shape.instrs.iter() {
                if let sh::Instr::UnknownUnknown(unk) = i {
                    //println!("{:20}: {}", name, i.show());
                    println!("{}, {:20}", format_unk(&unk.data), name.as_os_str().to_str().unwrap());
                }
            }
        } else if opt.show_memory {
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

    Ok(())
}

fn format_unk(xs: &[u8]) -> String {
    let mut out = Vec::new();
    for &x in xs.iter() {
        out.push(' ');
        if x >= 0x21 && x <= 0x5E || x >= 0x61 && x <= 0x7E {
            out.push(' ');
            out.push(x as char);
        } else {
            b2h(x, &mut out);
        }
    }
    out.iter().collect()
}

fn _find_instr_at_offset(offset: usize, instrs: &[Instr]) -> Option<&Instr> {
    for instr in instrs.iter() {
        if instr.at_offset() == offset {
            return Some(instr);
        }
    }
    None
}
