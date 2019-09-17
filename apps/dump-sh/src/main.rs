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
use omnilib::{make_opt_struct, OmniLib};
use reverse::b2h;
use sh::{Instr, RawShape, SHAPE_LOAD_BASE};
use simplelog::*;
use std::{collections::HashMap, fs};
use structopt::StructOpt;

make_opt_struct!(#[structopt(
    name = "shdump",
    about = "OpenFA shape slicing and discovery tooling"
)]
Opt {
    #[structopt(short = "v", long = "verbose", help = "Trace execution")]
    verbose => bool,

    #[structopt(short = "a", long = "all", help = "Show all")]
    show_all => bool,

    #[structopt(short = "m", long = "matching", help = "Show matching instructions")]
    show_matching => Option<String>,

    #[structopt(short = "r", long = "matching-memref", help = "Show matching memory loads")]
    show_matching_memref => Option<String>,

    #[structopt(short = "p", long = "plus", default_value = "0", help = "Show count after matching")]
    show_after_matching => usize,

    #[structopt(short = "l", long = "last", help = "Show last instructions")]
    show_last => bool,

    #[structopt(short = "u", long = "unknown", help = "Show unknown instructions")]
    show_unknown => bool,

    #[structopt(short = "x", long = "memory", help = "Show all i386 memory refs")]
    show_memory => bool,

    #[structopt(short = "n", long = "no-name", help = "Elide names in output")]
    quiet => bool,

    #[structopt(short = "d", long = "dump-code", help = "Dump all i386 code fragments")]
    dump_code => bool,

    #[structopt(short = "c", long = "custom", help = "Custom")]
    custom => bool,

    #[structopt(help = "Shape files to display.")]
    omni_inputs => Vec<String>
});

#[allow(clippy::cognitive_complexity)] // Impossible to organize if you don't know what the goal is.
fn main() -> Fallible<()> {
    let opt = Opt::from_args();
    let level = if opt.verbose {
        LevelFilter::Trace
    } else {
        LevelFilter::Warn
    };
    TermLogger::init(level, Config::default())?;

    let (omni, inputs) = opt.find_inputs(&opt.omni_inputs)?;
    for (game, name) in &inputs {
        let lib = omni.library(&game);
        let data = lib.load(name)?;
        let shape = RawShape::from_bytes(&data)?;

        if opt.show_all {
            for (i, instr) in shape.instrs.iter().enumerate() {
                println!("{:3}: {}", i, instr.show());
            }
        } else if let Some(ref target) = opt.show_matching {
            for (i, instr) in shape.instrs.iter().enumerate() {
                if instr.magic() == target {
                    let mut frags = vec![instr.show()];
                    for j in 0..opt.show_after_matching {
                        if i + 1 + j < shape.instrs.len() {
                            frags.push(shape.instrs[i + 1 + j].show())
                        }
                    }
                    let out = frags.join("; ");

                    if opt.quiet {
                        println!("{}", out);
                    } else {
                        println!("{:60}: {}", name, out);
                    }
                }
            }
        } else if let Some(ref target) = opt.show_matching_memref {
            for sh_instr in shape.instrs.iter() {
                if let sh::Instr::X86Code(x86) = sh_instr {
                    let mut pos = 0;
                    for instr in &x86.bytecode.instrs {
                        for operand in &instr.operands {
                            if let i386::Operand::Memory(memref) = operand {
                                if let Ok(tramp) = shape.lookup_trampoline_by_offset(
                                    memref.displacement.wrapping_sub(SHAPE_LOAD_BASE as i32) as u32,
                                ) {
                                    if &tramp.name == target {
                                        println!(
                                            "{} @ {} in {}:{} -> {}",
                                            tramp.name,
                                            sh_instr.at_offset(),
                                            game,
                                            name,
                                            instr.show_relative(sh_instr.at_offset() + pos)
                                        );
                                    }
                                }
                            }
                        }
                        pos += instr.size();
                    }
                }
            }
        } else if opt.show_last {
            let fmt = shape
                .instrs
                .last()
                .map(sh::Instr::show)
                .ok_or("NO INSTRUCTIONS")
                .unwrap();
            println!("{:20}: {}", name, fmt);
        } else if opt.show_unknown {
            for i in shape.instrs.iter() {
                if let sh::Instr::UnknownUnknown(unk) = i {
                    //println!("{:20}: {}", name, i.show());
                    println!("{}, {:20}", format_unk(&unk.data), name);
                }
            }
        } else if opt.show_memory {
            let mut dedup = HashMap::new();
            for vinstr in shape.instrs {
                if let sh::Instr::X86Code(x86) = vinstr {
                    for instr in &x86.bytecode.instrs {
                        for operand in &instr.operands {
                            if let i386::Operand::Memory(memref) = operand {
                                let key = format!("{}", memref);
                                *dedup.entry(key).or_insert(0) += 1;
                            }
                        }
                    }
                }
            }
            let mut memrefs = dedup
                .keys()
                .map(std::borrow::ToOwned::to_owned)
                .collect::<Vec<String>>();
            memrefs.sort();
            for memref in memrefs.iter() {
                println!("{} - {}", dedup[memref], memref);
            }
        } else if opt.dump_code {
            fs::create_dir_all(&format!("dump/i386/{}", game))?;
            for vinstr in shape.instrs {
                if let sh::Instr::X86Code(ref x86) = vinstr {
                    let filename = format!(
                        "dump/i386/{}/{}-{:04X}.i386",
                        game,
                        name,
                        vinstr.at_offset()
                    );
                    let mut v: Vec<u8> = Vec::new();
                    let start = if x86.have_header { 2 } else { 0 };
                    for i in start..x86.length {
                        v.push(unsafe { *x86.data.add(i) });
                    }
                    fs::write(&filename, &v)?;
                }
            }
        } else if opt.custom {
            let mut offset = 0;
            while offset < shape.instrs.len() {
                let instr = &shape.instrs[offset];
                if let sh::Instr::X86Code(_) = instr {
                    let suc = &shape.instrs[offset + 1];
                    if let sh::Instr::UnknownData(_) = suc {
                        let suc2 = &shape.instrs[offset + 2];
                        if let sh::Instr::X86Code(_) = suc2 {
                            println!("{} - {:?}", suc.magic(), name);
                            //println!("{}", suc.magic());
                        }
                        offset += 1;
                    }
                    offset += 1;
                }
                offset += 1;
            }
        }
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
