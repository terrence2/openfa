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
use anyhow::{bail, ensure, Result};
use i386::{ByteCode, Instr, Memonic, Operand};
use log::trace;
use peff::{PortableExecutable, Trampoline};
use reverse::bs2s;
use std::{collections::HashSet, fs::File, io::Write};

pub const MC_LOAD_BASE: u32 = 0xAA00_0000;

#[derive(Debug, Eq, PartialEq)]
enum ReturnKind {
    Interp,
    Error,
    Jump(usize),
}

impl ReturnKind {
    fn from_name(s: &str, target: usize) -> Result<Self> {
        Ok(match s {
            "do_start_interp" => ReturnKind::Interp,
            "_ErrorExit" => ReturnKind::Error,
            "_MISSIONSuccess@0" => ReturnKind::Jump(target),
            "@OBJAlias@8" => ReturnKind::Jump(target),
            "@OBJGet@4" => ReturnKind::Jump(target),
            _ => bail!("unexpected return trampoline name"),
        })
    }
}

#[derive(Default)]
pub struct Disassembler {
    instrs: Vec<Instr>,
}

impl Disassembler {
    // Track internal jumps so that we can discover all code paths and data voids.
    pub fn disassemble_fragment(
        &mut self,
        offset: usize,
        pe: &PortableExecutable,
    ) -> Result<ByteCode> {
        // Seed external jumps with our implicit initial jump.
        let mut offset = 0usize;
        let mut jump_targets = HashSet::new();
        jump_targets.insert(offset);

        while !jump_targets.is_empty() {
            trace!(
                "top of loop at {:04X} with external jumps: {:?}",
                offset,
                jump_targets
                    .iter()
                    .map(|n| format!("{:04X}", n))
                    .collect::<Vec<_>>()
            );

            // If we've found an external jump, that's good evidence that this is x86 code, so just
            // go ahead and decode that.
            if jump_targets.contains(&offset) {
                jump_targets.remove(&offset);
                trace!("ip reached external jump");

                let (bc, return_state) =
                    Self::disassemble_to_ret(&pe.code[offset..], offset, &pe.trampolines)?;
                trace!("decoded {} instructions", bc.instrs.len());

                Self::find_external_jumps(offset, &bc, &mut jump_targets);
                trace!(
                    "new external jumps: {:?}",
                    jump_targets
                        .iter()
                        .map(|n| format!("{:04X}", n))
                        .collect::<Vec<_>>(),
                );

                let bc_size = bc.size as usize;
                // vinstrs.push(Self::make_code(bc, pe, *offset));
                offset += bc_size;

                // If we're jumping into normal x86 code we should expect to resume
                // running more code right below the call.
                match return_state {
                    ReturnKind::Error => {
                        let message = Self::read_name(&pe.code[offset..])?;
                        let length = message.len() + 1;
                        // vinstrs.push(Instr::X86Message(X86Message {
                        //     offset: *offset,
                        //     message,
                        // }));
                        offset += length;
                        jump_targets.insert(offset);
                        continue;
                    }
                    ReturnKind::Jump(target) => {
                        jump_targets.insert(target);
                        continue;
                    }
                    ReturnKind::Interp => {}
                };
            }
        }

        unimplemented!()
    }

    fn read_name(n: &[u8]) -> Result<String> {
        let end_offset: usize = n.iter().position(|&c| c == 0).unwrap();
        Ok(std::str::from_utf8(&n[..end_offset])?.to_owned())
    }

    // Note: excluding return-to-trampoline and loops.
    fn find_external_jumps(base: usize, bc: &ByteCode, external_jumps: &mut HashSet<usize>) {
        let mut ip = 0;
        let ip_end = bc.size as usize;
        for instr in bc.instrs.iter() {
            ip += instr.size();
            if instr.is_jump() {
                let delta = Self::operand_to_offset(&instr.operands[0]);
                let ip_target = ip + delta;
                if ip_target >= ip_end {
                    external_jumps.insert(base + ip_target);
                }
            };
        }
    }

    fn operand_to_offset(op: &Operand) -> usize {
        // Note that we cannot rely on negative jumps being encoded with a signed instr.
        let delta = match op {
            Operand::Imm32s(delta) => *delta as isize,
            Operand::Imm32(delta) => *delta as i32 as isize,
            _ => {
                trace!("Detected indirect jump target: {}", op);
                0
            }
        };
        if delta < 0 {
            trace!("Skipping loop of {} bytes", delta);
            return 0usize;
        }
        delta as usize
    }

    fn disassemble_to_ret(
        code: &[u8],
        offset: usize,
        trampolines: &[Trampoline],
    ) -> Result<(ByteCode, ReturnKind)> {
        // Note that there are internal calls that we need to filter out, so we
        // have to consult the list of trampolines to find the interpreter return.
        let maybe_bc =
            ByteCode::disassemble_until(MC_LOAD_BASE as usize + offset, code, |instrs, _rem| {
                let last = &instrs[instrs.len() - 1];
                if last.memonic == Memonic::Jump {
                    return true;
                }
                if instrs.len() < 2 {
                    return false;
                }
                let ret = &instrs[instrs.len() - 1];
                let push = &instrs[instrs.len() - 2];
                if ret.memonic == Memonic::Return && push.memonic == Memonic::Push {
                    if let Operand::Imm32s(v) = push.operands[0] {
                        let reltarget = (v as u32).wrapping_sub(MC_LOAD_BASE);
                        let trampoline =
                            Self::find_trampoline_for_offset(reltarget as usize, trampolines);
                        return trampoline.name == "do_start_interp"
                            || trampoline.name == "_ErrorExit"
                            || trampoline.name == "_MISSIONSuccess@0"
                            || trampoline.name == "@OBJAlias@8"
                            || trampoline.name == "@OBJGet@4";
                    }
                }
                false
            });
        if let Err(e) = maybe_bc {
            i386::DisassemblyError::maybe_show(&e, code);
            bail!("Don't know how to disassemble at {}: {:?}", offset, e);
        }
        let mut bc = maybe_bc?;
        ensure!(bc.instrs.len() >= 1, "expected at least 1 instructions");

        // Annotate any memory read in this block with the source.
        let mut push_value = 0;
        for instr in bc.instrs.iter_mut() {
            let mut context = None;
            for op in &instr.operands {
                if let Operand::Memory(ref mr) = op {
                    let mt = Self::find_trampoline_for_target(mr.displacement as u32, trampolines);
                    if let Ok(tramp) = mt {
                        context = Some(tramp.name.to_owned());
                    }
                }
            }
            if let Some(s) = context {
                instr.set_context(&s);
            }
            if instr.memonic == Memonic::Push {
                if let Operand::Imm32s(v) = instr.operands[0] {
                    push_value = (v as u32).wrapping_sub(MC_LOAD_BASE) as usize;
                }
            }
            if instr.memonic == Memonic::Return {
                let trampoline = Self::find_trampoline_for_offset(push_value, trampolines);
                instr.set_context(&trampoline.name);
            }
        }

        match bc.instrs[bc.instrs.len() - 1].memonic {
            Memonic::Jump => {
                let target = &bc.instrs[bc.instrs.len() - 1];
                if let Operand::Imm32(v) = target.operands[0] {
                    Ok((bc, ReturnKind::Jump(v as usize + 5)))
                } else if let Operand::Imm32s(v) = target.operands[0] {
                    Ok((bc, ReturnKind::Jump(v as usize + 5)))
                } else {
                    panic!(
                        "found jump without immediate target: {:?}",
                        target.operands[0]
                    )
                }
            }
            Memonic::Return => {
                // Look for the jump target to figure out where we need to continue decoding.
                let target = &bc.instrs[bc.instrs.len() - 2];
                let target_addr = Self::find_pushed_address(target)?;
                let tramp = Self::find_trampoline_for_target(target_addr, trampolines)?;

                // The argument pointer always points to just after the code segment.
                let arg0 = &bc.instrs[bc.instrs.len() - 3];
                let arg0_ptr = Self::find_pushed_address(arg0)? - MC_LOAD_BASE;
                ensure!(
                    arg0_ptr as usize == offset + bc.size as usize,
                    "expected second stack arg to point after code block"
                );

                Ok((bc, ReturnKind::from_name(&tramp.name, arg0_ptr as usize)?))
            }
            _ => {
                panic!("only expected to stop decoding at jump or ret")
            }
        }
    }

    fn find_trampoline_for_offset(offset: usize, trampolines: &[Trampoline]) -> &Trampoline {
        for trampoline in trampolines {
            if trampoline.offset == offset {
                return trampoline;
            }
        }
        panic!("expected all returns to jump to a trampoline")
    }

    fn find_trampoline_for_target(
        target_addr: u32,
        trampolines: &[Trampoline],
    ) -> Result<&Trampoline> {
        for tramp in trampolines {
            trace!(
                "checking {:08X} against {:20} @ loc:{:08X}",
                target_addr,
                tramp.name,
                tramp.mem_location
            );
            if target_addr == tramp.mem_location {
                return Ok(tramp);
            }
        }
        bail!("no matching trampoline for exit")
    }

    fn find_pushed_address(target: &i386::Instr) -> Result<u32> {
        ensure!(target.memonic == i386::Memonic::Push, "expected push");
        ensure!(target.operands.len() == 1, "expected one operand");
        if let Operand::Imm32s(addr) = target.operands[0] {
            Ok(addr as u32)
        } else {
            bail!("expected imm32s operand")
        }
    }
}

pub struct MissionCommandScript {}

impl MissionCommandScript {
    pub fn from_bytes(data: &[u8], name: &str) -> Result<Self> {
        let mut pe = peff::PortableExecutable::from_bytes(data)?;

        // Do default relocation to a high address. This makes offsets appear
        // 0-based and tags all local pointers with an obvious flag.
        pe.relocate(MC_LOAD_BASE)?;

        // let mut p = std::env::current_dir()?;
        // let mut p = p.parent().unwrap();
        // let mut p = p.parent().unwrap();
        // let mut p = p.parent().unwrap().to_owned();
        // p.push("__dump__");
        // p.push("mc");
        // p.push(name.replace(':', "_"));
        // let mut p = p.with_extension("asm").to_owned();
        // println!("pwd: {:?}", p);
        // let mut fp = File::create(p)?;
        // fp.write(&pe.code);

        let mut disasm = Disassembler::default();
        disasm.disassemble_fragment(0, &pe)?;

        // println!("ABOUT TO DISASM: {} : {}", name, bs2s(&pe.code[0..20]));
        // let bc = ByteCode::disassemble_until(0, &pe.code, |codes, rem| {
        //     let last = &codes[codes.len() - 1];
        //     (last.memonic == Memonic::Return && rem[0] == 0 && rem[1] == 0)
        //         || last.memonic == Memonic::Jump
        // })?;
        // println!("{}\n---------", name);
        // println!("{}", bc);
        // println!();

        Ok(Self {})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lib::Libs;
    use simplelog::{Config, LevelFilter, TermLogger};

    #[test]
    fn it_works() -> Result<()> {
        TermLogger::init(LevelFilter::Trace, Config::default())?;

        let libs = Libs::for_testing()?;
        for (game, _palette, catalog) in libs.all() {
            for fid in catalog.find_with_extension("MC")? {
                let meta = catalog.stat(fid)?;
                // println!("At: {}:{:13} @ {}", game.test_dir, meta.name(), meta.path());

                if game.test_dir == "FA" && meta.name() == "U34.MC" {
                    let data = catalog.read(fid)?;
                    let _mc = MissionCommandScript::from_bytes(
                        data.as_ref(),
                        &format!("{}:{}", game.test_dir, meta.name()),
                    )?;
                }
            }
        }

        Ok(())
    }
}
