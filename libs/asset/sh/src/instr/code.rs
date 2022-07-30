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
use crate::{instr::read_name, Instr, RawShape, UnknownData, SHAPE_LOAD_BASE};
use ansi::ansi;
use anyhow::{bail, ensure, Result};
use i386::{ByteCode, Memonic, Operand};
use log::trace;
use once_cell::sync::Lazy;
use peff::{PortableExecutable, Trampoline};
use reverse::bs2s;
use std::{cmp, collections::HashSet};

pub static DATA_RELOCATIONS: Lazy<HashSet<String>> = Lazy::new(|| {
    [
        "_currentTicks",
        "_effectsAllowed",
        "brentObjId",
        "viewer_x",
        "viewer_z",
        "xv32",
        "zv32",
    ]
    .iter()
    .map(|&n| n.to_owned())
    .collect()
});

#[derive(Debug, Eq, PartialEq)]
enum ReturnKind {
    Interp,
    Error,
}

impl ReturnKind {
    fn from_name(s: &str) -> Result<Self> {
        Ok(match s {
            "do_start_interp" => ReturnKind::Interp,
            "_ErrorExit" => ReturnKind::Error,
            _ => bail!("unexpected return trampoline name"),
        })
    }
}

#[derive(Debug)]
pub struct X86Message {
    offset: usize,
    message: String,
}

impl X86Message {
    pub fn size(&self) -> usize {
        self.message.len() + 1
    }

    pub fn magic(&self) -> &'static str {
        "Message"
    }

    pub fn at_offset(&self) -> usize {
        self.offset
    }

    pub fn show(&self) -> String {
        format!(
            "@{:04X} {}Messg{}: {}{}{}",
            self.offset,
            ansi().red().bold(),
            ansi(),
            ansi().red(),
            self.message,
            ansi()
        )
    }
}

#[derive(Debug)]
pub struct X86Code {
    pub offset: usize,
    pub length: usize,
    pub data: *const u8,
    pub bytecode: ByteCode,
    pub have_header: bool,
}

impl X86Code {
    pub const MAGIC: u8 = 0xF0;

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

    fn lowest_jump(jumps: &HashSet<usize>) -> usize {
        let mut lowest = usize::max_value();
        for &jump in jumps {
            if jump < lowest {
                lowest = jump;
            }
        }
        lowest
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

    fn find_trampoline_for_offset(offset: usize, trampolines: &[Trampoline]) -> &Trampoline {
        for trampoline in trampolines {
            if trampoline.offset == offset {
                return trampoline;
            }
        }
        panic!("expected all returns to jump to a trampoline")
    }

    fn disassemble_to_ret(
        code: &[u8],
        offset: usize,
        trampolines: &[Trampoline],
    ) -> Result<(ByteCode, ReturnKind)> {
        // Note that there are internal calls that we need to filter out, so we
        // have to consult the list of trampolines to find the interpreter return.
        let maybe_bc = i386::ByteCode::disassemble_until(
            SHAPE_LOAD_BASE as usize + offset,
            code,
            |instrs, _rem| {
                if instrs.len() < 2 {
                    return false;
                }
                let ret = &instrs[instrs.len() - 1];
                let push = &instrs[instrs.len() - 2];
                if ret.memonic == Memonic::Return && push.memonic == Memonic::Push {
                    if let Operand::Imm32s(v) = push.operands[0] {
                        let reltarget = (v as u32).wrapping_sub(SHAPE_LOAD_BASE);
                        let trampoline =
                            Self::find_trampoline_for_offset(reltarget as usize, trampolines);
                        return trampoline.name == "do_start_interp"
                            || trampoline.name == "_ErrorExit";
                    }
                }
                false
            },
        );
        if let Err(e) = maybe_bc {
            i386::DisassemblyError::maybe_show(&e, code);
            bail!("Don't know how to disassemble at {}: {:?}", offset, e);
        }
        let mut bc = maybe_bc?;
        ensure!(bc.instrs.len() >= 3, "expected at least 3 instructions");

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
                    push_value = (v as u32).wrapping_sub(SHAPE_LOAD_BASE) as usize;
                }
            }
            if instr.memonic == Memonic::Return {
                let trampoline = Self::find_trampoline_for_offset(push_value, trampolines);
                instr.set_context(&trampoline.name);
            }
        }

        // Look for the jump target to figure out where we need to continue decoding.
        let target = &bc.instrs[bc.instrs.len() - 2];
        let target_addr = Self::find_pushed_address(target)?;
        let tramp = Self::find_trampoline_for_target(target_addr, trampolines)?;

        // The argument pointer always points to just after the code segment.
        let arg0 = &bc.instrs[bc.instrs.len() - 3];
        let arg0_ptr = Self::find_pushed_address(arg0)? - SHAPE_LOAD_BASE;
        ensure!(
            arg0_ptr as usize == offset + bc.size as usize,
            "expected second stack arg to point after code block"
        );

        Ok((bc, ReturnKind::from_name(&tramp.name)?))
    }

    fn make_code(bc: ByteCode, pe: &PortableExecutable, offset: usize) -> Instr {
        let bc_size = bc.size as usize;
        let have_header = pe.code[offset - 2] == 0xF0;
        let section_offset = if have_header { offset - 2 } else { offset };
        let section_length = bc_size + if have_header { 2 } else { 0 };
        Instr::X86Code(X86Code {
            offset: section_offset,
            length: section_length,
            data: pe.code[section_offset..].as_ptr(),
            bytecode: bc,
            have_header,
        })
    }

    pub fn from_bytes(
        _name: &str,
        offset: &mut usize,
        pe: &PortableExecutable,
        trailer: &[Instr],
        vinstrs: &mut Vec<Instr>,
    ) -> Result<()> {
        let section = &pe.code[*offset..];
        assert_eq!(section[0], Self::MAGIC);
        assert_eq!(section[1], 0);
        *offset += 2;

        // Seed external jumps with our implicit F0 section jump.
        let mut external_jumps = HashSet::new();
        external_jumps.insert(*offset);

        while !external_jumps.is_empty() {
            trace!(
                "top of loop at {:04X} with external jumps: {:?}",
                *offset,
                external_jumps
                    .iter()
                    .map(|n| format!("{:04X}", n))
                    .collect::<Vec<_>>()
            );

            // If we've found an external jump, that's good evidence that this is x86 code, so just
            // go ahead and decode that.
            if external_jumps.contains(offset) {
                external_jumps.remove(offset);
                trace!("ip reached external jump");

                let (bc, return_state) =
                    Self::disassemble_to_ret(&pe.code[*offset..], *offset, &pe.trampolines)?;
                trace!("decoded {} instructions", bc.instrs.len());

                Self::find_external_jumps(*offset, &bc, &mut external_jumps);
                trace!(
                    "new external jumps: {:?}",
                    external_jumps
                        .iter()
                        .map(|n| format!("{:04X}", n))
                        .collect::<Vec<_>>(),
                );

                let bc_size = bc.size as usize;
                vinstrs.push(Self::make_code(bc, pe, *offset));
                *offset += bc_size;

                // If we're jumping into normal x86 code we should expect to resume
                // running more code right below the call.
                match return_state {
                    ReturnKind::Error => {
                        let message = read_name(&pe.code[*offset..])?;
                        let length = message.len() + 1;
                        vinstrs.push(Instr::X86Message(X86Message {
                            offset: *offset,
                            message,
                        }));
                        *offset += length;
                        external_jumps.insert(*offset);
                        continue;
                    }
                    ReturnKind::Interp => {}
                };

                // TODO: on Error's try to print the message.
            }

            // If three is a Return in the middle of code, we'll get here; jump back in.
            if external_jumps.contains(offset) {
                continue;
            }

            // If we have more jumps, continue looking for virtual instructions in case they do
            // not have an F0 around them.
            if external_jumps.is_empty()
                || Self::lowest_jump(&external_jumps) < *offset
                || *offset >= pe.code.len()
            {
                trace!("no more external jumps: breaking");
                break;
            }

            // Otherwise, we are between code segments. There may be vinstrs
            // here, or maybe more x86 instructions, or maybe some raw data.
            // Look for a vinstr and it there is one, decode it. Otherwise
            // treat it as raw data.

            // Note: We do not expect another F0 while we have external jumps to find.

            // Sandwiched instructions
            // Unmask
            //   12  -- 16 bit
            //   6E  -- 32 bit
            // Unmask and Xform
            //   C4  -- 16 bit
            //   C6  -- 32 bit
            // Jump
            //   48  --
            // Pile of code
            //   F0
            //   X86Message
            // Unknown
            //   E4  -- Only in wave1/2
            //   Data

            trace!(
                "trying vinstr at: {}",
                bs2s(&pe.code[*offset..(*offset + 10).min(pe.code.len())])
            );
            let saved_offset = *offset;
            let mut have_vinstr = true;
            let maybe = RawShape::read_instr(offset, pe, trailer, vinstrs);
            #[allow(clippy::if_same_then_else)]
            if let Err(_e) = maybe {
                have_vinstr = false;
            } else if let Some(&Instr::UnknownUnknown(_)) = vinstrs.last() {
                vinstrs.pop();
                *offset = saved_offset;
                have_vinstr = false;
            } else if let Some(&Instr::TrailerUnknown(_)) = vinstrs.last() {
                // We still have external jumps to track down, so our data blob just
                // happened to contain zeros. Keep going.
                vinstrs.pop();
                *offset = saved_offset;
                have_vinstr = false;
            }

            if !have_vinstr && *offset < Self::lowest_jump(&external_jumps) {
                // There is no instruction here, so assume data. Find the closest jump
                // target remaining and fast-forward there.
                trace!(
                    "Adding data block @{:04X}: {}",
                    *offset,
                    bs2s(&pe.code[*offset..cmp::min(pe.code.len(), *offset + 80)])
                );
                let mut end = Self::lowest_jump(&external_jumps);
                if pe.code[end - 2] == 0xF0 && pe.code[end - 1] == 0x00 {
                    end -= 2;
                }
                vinstrs.push(Instr::UnknownData(UnknownData {
                    offset: *offset,
                    length: end - *offset,
                    data: pe.code[*offset..end].to_vec(),
                }));
                *offset = end;
            }
        }

        Ok(())
    }

    pub fn code_offset(&self, base: u32) -> u32 {
        base + (self.offset as u32) + if self.have_header { 2 } else { 0 }
    }

    pub fn size(&self) -> usize {
        self.length
    }

    pub fn magic(&self) -> &'static str {
        "F0"
    }

    pub fn at_offset(&self) -> usize {
        self.offset
    }

    pub fn show(&self) -> String {
        let show_offset = if self.have_header {
            self.offset + 2
        } else {
            self.offset
        };
        let hdr = if self.have_header { "F0 00" } else { "     " };
        format!(
            "@{:04X} {}X86Cd{}: {}{}{}|\n  {}",
            self.offset,
            ansi().green().bold(),
            ansi(),
            ansi().green().bold(),
            hdr,
            ansi(),
            self.bytecode.show_relative(show_offset).trim()
        )
    }
}
