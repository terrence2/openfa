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
use sh::{Instr, RawShape};
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

    #[structopt(short = "i", long = "i386", help = "Account for i386 forms")]
    i386 => bool,

    #[structopt(short = "c", long = "custom", help = "Custom")]
    custom => bool
});

#[allow(clippy::cyclomatic_complexity)] // Impossible to organize if you don't know what the goal is.
fn main() -> Fallible<()> {
    let opt = Opt::from_args();
    let level = if opt.verbose {
        LevelFilter::Trace
    } else {
        LevelFilter::Warn
    };
    TermLogger::init(level, Config::default())?;

    let (omni, inputs) = opt.find_inputs()?;
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
        } else if opt.i386 {
            fn is_start_interp(instr: &i386::Instr, sh: &RawShape) -> bool {
                if let i386::Operand::Imm32s(x) = instr.operands[0] {
                    let abs_offset = x as u32 - sh::SHAPE_LOAD_BASE;
                    if let Ok(tramp) = sh.lookup_trampoline_by_offset(abs_offset) {
                        if tramp.name == "do_start_interp" {
                            return true;
                        }
                    }
                }
                false
            }

            fn is_memref_to_tramp(op: &i386::Operand, sh: &RawShape, name: &str) -> bool {
                if let i386::Operand::Memory(i386::MemRef {
                    displacement,
                    base: None,
                    index: None,
                    scale: 1,
                    segment: None,
                    ..
                }) = op
                {
                    let abs_offset = *displacement as u32 - sh::SHAPE_LOAD_BASE;
                    if let Ok(tramp) = sh.lookup_trampoline_by_offset(abs_offset) {
                        if tramp.name == name {
                            return true;
                        }
                    }
                }
                false
            }

            /*
                1081: @4A0B X86Code: F0 00 66 83 3D 70 4D 00 AA 01 75 11 68 22 4A 00 AA 68 88 4D 00 AA C3
                @00|4A0D: 66 83 3D 70 4D 00 AA 01  Compare([0xAA004D70], 0x1) [_PLgearDown]
                @08|4A15: 75 11                   Jcc(Unary(Check(ZF, false)))(0x11 -> 0x4A28)
                @0A|4A17: 68 22 4A 00 AA          Push(0xAA004A22)
                @0F|4A1C: 68 88 4D 00 AA          Push(0xAA004D88)
                @14|4A21: C3                      Return() [do_start_interp]
                1082: @4A22 Unk12: 12 00| 10 00  (target:4A36)
                1083: @4A26 X86Code: F0 00 68 33 4A 00 AA 68 88 4D 00 AA C3
                @00|4A28: 68 33 4A 00 AA          Push(0xAA004A33)
                @05|4A2D: 68 88 4D 00 AA          Push(0xAA004D88)
                @0A|4A32: C3                      Return() [do_start_interp]
            */
            fn match_show_part0(instr: &sh::Instr) -> bool {
                if let sh::Instr::X86Code(sh::X86Code {
                    have_header: true,
                    bytecode: i386::ByteCode { instrs: bc, .. },
                    ..
                }) = instr
                {
                    if bc[0].memonic == i386::Memonic::Compare
                        && bc[2].memonic == i386::Memonic::Push
                        && bc[3].memonic == i386::Memonic::Push
                        && bc[4].memonic == i386::Memonic::Return
                    {
                        if let i386::Memonic::Jcc(_cond) = bc[1].memonic {
                            return true;
                        }
                    }
                }
                false
            }

            fn match_show_part1(instr: &sh::Instr) -> bool {
                if let sh::Instr::Unmask(_) = instr {
                    return true;
                }
                false
            }

            fn match_return_to_interp(instr: &sh::Instr, sh: &RawShape) -> bool {
                if let sh::Instr::X86Code(sh::X86Code {
                    have_header: true,
                    bytecode: i386::ByteCode { instrs: bc, .. },
                    ..
                }) = instr
                {
                    if bc[0].memonic == i386::Memonic::Push
                        && bc[1].memonic == i386::Memonic::Push
                        && bc[2].memonic == i386::Memonic::Return
                        && is_start_interp(&bc[1], sh)
                    {
                        return true;
                    }
                }
                false
            }

            fn match_show(offset: usize, sh: &RawShape) -> bool {
                if offset > sh.instrs.len() - 3 {
                    return false;
                }
                if match_show_part0(&sh.instrs[offset])
                    && match_show_part1(&sh.instrs[offset + 1])
                    && match_return_to_interp(&sh.instrs[offset + 2], sh)
                {
                    //println!("SHOW: {}", sh.instrs[offset + 1].show());
                    return true;
                }
                false
            }

            /*
            @00|191F: 66 83 3D 70 4D 00 AA 01  Compare([0xAA004D70], 0x1) [_PLgearDown]
            @08|1927: 75 36                   Jcc(Unary(Check(ZF, false)))(0x36 -> 0x195F)
            @0A|1929: E8 00 00 00 00          Call(0x0 -> 0x192E)
            @0F|192E: 5B                      Pop(EBX)
            @10|192F: 81 C3 21 00 00 00       Add(EBX, 0x21)
            @16|1935: 66 A1 76 4D 00 AA       Move(AX, [0xAA004D76]) [_PLgearPos]

            @1C|193B: 66 D1 F8                Sar(AX, 0x1)

            @1F|193E: 66 89 43 08             Move([EBX+0x8], AX)
            @23|1942: 68 4D 19 00 AA          Push(0xAA00194D)
            @28|1947: 68 88 4D 00 AA          Push(0xAA004D88)
            @2D|194C: C3                      Return() [do_start_interp]
            386: @194D UnkC4: C4 00| EA FF F8 FF FE FF 00 00 00 00 00 00 68 22 t:(-22,-8,-2) a:(0,0,0) 68 22 (target:3BC5)
            387: @195D X86Code: F0 00 68 6A 19 00 AA 68 88 4D 00 AA C3
            @00|195F: 68 6A 19 00 AA          Push(0xAA00196A)
            @05|1964: 68 88 4D 00 AA          Push(0xAA004D88)
            @0A|1969: C3                      Return() [do_start_interp]
            */
            fn match_xform_part0(instr: &sh::Instr) -> bool {
                if let sh::Instr::X86Code(sh::X86Code {
                    have_header: true,
                    bytecode: i386::ByteCode { instrs: bc, .. },
                    ..
                }) = instr
                {
                    if bc[0].memonic == i386::Memonic::Compare
                        && bc[2].memonic == i386::Memonic::Call
                        && bc[3].memonic == i386::Memonic::Pop
                        && bc[4].memonic == i386::Memonic::Add
                    {
                        if let i386::Memonic::Jcc(_cond) = bc[1].memonic {
                            return true;
                        }
                    }
                }
                false
            }

            fn match_xform_part1(instr: &sh::Instr) -> bool {
                if let sh::Instr::XformUnmask(_) = instr {
                    return true;
                }
                false
            }

            fn match_xform(offset: usize, sh: &RawShape) -> bool {
                if offset > sh.instrs.len() - 3 {
                    return false;
                }
                if match_xform_part0(&sh.instrs[offset])
                    && match_xform_part1(&sh.instrs[offset + 1])
                    && match_return_to_interp(&sh.instrs[offset + 2], sh)
                {
                    //println!("XFORM: {}", sh.instrs[offset + 1].show());
                    return true;
                }
                false
            }

            /*
            @00|1A20: E8 00 00 00 00          Call(0x0 -> 0x1A25)
            @05|1A25: 5B                      Pop(EBX)
            @06|1A26: 81 C3 1E 00 00 00       Add(EBX, 0x1E)
            @0C|1A2C: 66 A1 AC 55 00 AA       Move(AX, [0xAA0055AC]) [_PLcanardPos]
            @12|1A32: 66 89 43 08             Move([EBX+0x8], AX)
            @16|1A36: 68 41 1A 00 AA          Push(0xAA001A41)
            @1B|1A3B: 68 D0 55 00 AA          Push(0xAA0055D0)
            @20|1A40: C3                      Return() [do_start_interp]
            393: @1A41 UnkC4: C4 00| 00 00 05 00 D8 FF 00 00 00 00 00 00 1E 1A t:(0,5,-40) a:(0,0,0) 1E 1A (target:346F)
            394: @1A51 X86Code: F0 00 68 5E 1A 00 AA 68 D0 55 00 AA C3
            @00|1A53: 68 5E 1A 00 AA          Push(0xAA001A5E)
            @05|1A58: 68 D0 55 00 AA          Push(0xAA0055D0)
            @0A|1A5D: C3                      Return() [do_start_interp]
            */
            fn match_control_part0(instr: &sh::Instr) -> bool {
                if let sh::Instr::X86Code(sh::X86Code {
                    have_header: true,
                    bytecode: i386::ByteCode { instrs: bc, .. },
                    ..
                }) = instr
                {
                    if bc[0].memonic == i386::Memonic::Call
                        && bc[1].memonic == i386::Memonic::Pop
                        && bc[2].memonic == i386::Memonic::Add
                        && bc[3].memonic == i386::Memonic::Move
                    {
                        return true;
                    }
                }
                false
            }

            fn match_control_part1(instr: &sh::Instr) -> bool {
                if let sh::Instr::XformUnmask(_) = instr {
                    return true;
                }
                false
            }

            fn match_control(offset: usize, sh: &RawShape) -> bool {
                if offset > sh.instrs.len() - 3 {
                    return false;
                }
                if match_control_part0(&sh.instrs[offset])
                    && match_control_part1(&sh.instrs[offset + 1])
                    && match_return_to_interp(&sh.instrs[offset + 2], sh)
                {
                    //println!("CONTROL: {}", sh.instrs[offset + 1].show());
                    return true;
                }
                false
            }

            /*  Effects Allowed
            @00|0062: 53                      Push(EBX)
            @01|0063: BB 9C 07 00 AA          Move(EBX, 0xAA00079C)
            @06|0068: F7 83 8E 00 00 00 00 00 01 00  Test([EBX+0x8E], 0x10000)
            @10|0072: 74 14                   Jcc(Unary(Check(ZF, true)))(0x14 -> 0x88)
            @12|0074: 81 0D 96 07 00 AA 00 00 01 00  Or([0xAA000796], 0x10000) [_effectsAllowed]
            @1C|007E: 81 0D 90 07 00 AA 00 00 01 00  Or([0xAA000790], 0x10000) [_effects]
            @26|0088: 5B                      Pop(EBX)
            @27|0089: 68 94 00 00 AA          Push(0xAA000094)
            @2C|008E: 68 A2 07 00 AA          Push(0xAA0007A2)
            @31|0093: C3                      Return() [do_start_interp]
            */
            fn match_effects(offset: usize, sh: &RawShape) -> bool {
                if let sh::Instr::X86Code(sh::X86Code {
                    have_header: true,
                    bytecode: i386::ByteCode { instrs: bc, .. },
                    ..
                }) = &sh.instrs[offset]
                {
                    if bc[0].memonic == i386::Memonic::Push
                        && bc[1].memonic == i386::Memonic::Move
                        && bc[2].memonic == i386::Memonic::Test
                        && bc[4].memonic == i386::Memonic::Or
                        && bc[5].memonic == i386::Memonic::Or
                        && bc[6].memonic == i386::Memonic::Pop
                        && bc[7].memonic == i386::Memonic::Push
                        && bc[8].memonic == i386::Memonic::Push
                        && bc[9].memonic == i386::Memonic::Return
                        && is_start_interp(&bc[8], sh)
                        && is_memref_to_tramp(&bc[4].operands[0], sh, "_effectsAllowed")
                        && is_memref_to_tramp(&bc[5].operands[0], sh, "_effects")
                    {
                        return true;
                    }
                }
                false
            }

            /* Lightening
            @00|005E: C6 05 F6 41 00 AA 00    Move([0xAA0041F6], 0x0) [lighteningAllowed]
            @07|0065: 68 70 00 00 AA          Push(0xAA000070)
            @0C|006A: 68 F0 41 00 AA          Push(0xAA0041F0)
            @11|006F: C3                      Return() [do_start_interp]
            */
            fn match_lightening(offset: usize, sh: &RawShape) -> bool {
                if let sh::Instr::X86Code(sh::X86Code {
                    have_header: true,
                    bytecode: i386::ByteCode { instrs: bc, .. },
                    ..
                }) = &sh.instrs[offset]
                {
                    if bc[0].memonic == i386::Memonic::Move
                        && bc[1].memonic == i386::Memonic::Push
                        && bc[2].memonic == i386::Memonic::Push
                        && bc[3].memonic == i386::Memonic::Return
                        && is_start_interp(&bc[2], sh)
                        && is_memref_to_tramp(&bc[0].operands[0], sh, "lighteningAllowed")
                    {
                        return true;
                    }
                }
                false
            }

            let mut matching_show = 0;
            let mut matching_xform = 0;
            let mut matching_control = 0;
            let mut matching_effects = 0;
            let mut matching_lightening = 0;
            let mut nonmatching = 0;
            let mut offset = 0;
            while offset < shape.instrs.len() {
                let instr = &shape.instrs[offset];
                if let sh::Instr::X86Code(_) = instr {
                    if match_show(offset, &shape) {
                        matching_show += 1;
                        offset += 2;
                    } else if match_xform(offset, &shape) {
                        matching_xform += 1;
                        offset += 2;
                    } else if match_control(offset, &shape) {
                        matching_control += 1;
                        offset += 2;
                    } else if match_effects(offset, &shape) {
                        matching_effects += 1;
                    } else if match_lightening(offset, &shape) {
                        matching_lightening += 1;
                    } else {
                        nonmatching += 1;
                    }
                }
                offset += 1;
            }
            if matching_show
                + matching_xform
                + matching_control
                + matching_effects
                + matching_lightening
                + nonmatching
                > 0
            {
                println!(
                    "{} non, show: {}, xform: {}, control: {}, effects: {}, lightening: {}, name: {}",
                    nonmatching,
                    matching_show,
                    matching_xform,
                    matching_control,
                    matching_effects,
                    matching_lightening,
                    name
                );
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
