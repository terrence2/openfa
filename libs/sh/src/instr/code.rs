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
use failure::{bail, ensure, Fallible};
use i386::{ByteCode, Memonic, Operand};
use lazy_static::lazy_static;
use log::trace;
use peff::{Thunk, PE};
use reverse::bs2s;
use std::{cmp, collections::HashSet, mem};

lazy_static! {
    pub static ref DATA_RELOCATIONS: HashSet<String> = {
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
    };
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct X86Trampoline {
    // Offset is from the start of the code section.
    pub offset: usize,

    // The name attached to the thunk that would populate this trampoline.
    pub name: String,

    // Where this trampoline would indirect to, if jumped to.
    pub target: u32,

    // Shape files call into engine functions by setting up a stack frame
    // and then returning. The target of this is always one of these trampolines
    // stored at the tail of the PE. Store the in-memory location of the
    // thunk for fast comparison with relocated addresses.
    pub mem_location: u32,

    // Whatever tool was used to link .SH's bakes in a direct pointer to the GOT
    // PLT base (e.g. target) as the data location. Presumably when doing
    // runtime linking, it uses the IAT's name as a tag and rewrites the direct
    // load to the real address of the symbol (and not a split read of the code
    // and reloc in the GOT). These appear to be both global and per-object data
    // depending on the data -- e.g. brentObjectId is probably per-object and
    // _currentTicks is probably global?
    //
    // A concrete example; if the inline assembly does:
    //    `mov %ebp, [<addr of GOT of data>]`
    //
    // The runtime would presumably use the relocation of the above addr as an
    // opportunity to rewrite the load as a reference to the real memory. We
    // need to take all of this into account when interpreting the referencing
    // code.
    pub is_data: bool,
}

impl X86Trampoline {
    pub const SIZE: usize = 6;

    pub fn has_trampoline(offset: usize, pe: &PE) -> bool {
        pe.section_info.contains_key(".idata")
            && pe.code.len() >= offset + 6
            && pe.code[offset] == 0xFF
            && pe.code[offset + 1] == 0x25
    }

    pub fn from_pe(offset: usize, pe: &PE) -> Fallible<Self> {
        ensure!(Self::has_trampoline(offset, pe), "not a trampoline");
        let target = {
            let vp: &[u32] = unsafe { mem::transmute(&pe.code[offset + 2..offset + 6]) };
            vp[0]
        };

        let thunk = Self::find_matching_thunk(target, pe)?;
        let is_data = DATA_RELOCATIONS.contains(&thunk.name);
        Ok(X86Trampoline {
            offset,
            name: thunk.name.clone(),
            target,
            mem_location: SHAPE_LOAD_BASE + offset as u32,
            is_data,
        })
    }

    fn find_matching_thunk<'a>(addr: u32, pe: &'a PE) -> Fallible<&'a Thunk> {
        // The thunk table is code and therefore should have had a relocation entry
        // to move those pointers when we called relocate on the PE.
        trace!(
            "looking for target 0x{:X} in {} thunks",
            addr,
            pe.thunks.len()
        );
        for thunk in pe.thunks.iter() {
            if addr == thunk.vaddr {
                return Ok(thunk);
            }
        }

        // That said, not all SH files actually contain relocations for the thunk
        // targets(!). This is yet more evidence that they're not actually using
        // LoadLibrary to put shapes in memory. They're probably only using the
        // relocation list to rewrite data access with the thunks as tags. We're
        // using relocation, however, to help decode. So if the thunks are not
        // relocated automatically we have to check the relocated value
        // manually.
        let thunk_target = pe.relocate_thunk_pointer(SHAPE_LOAD_BASE, addr);
        trace!(
            "looking for target 0x{:X} in {} thunks",
            thunk_target,
            pe.thunks.len()
        );
        for thunk in pe.thunks.iter() {
            if thunk_target == thunk.vaddr {
                return Ok(thunk);
            }
        }

        // Also, in USNF, some of the thunks contain the base address already,
        // so treat them like a normal code pointer.
        let thunk_target = pe.relocate_pointer(SHAPE_LOAD_BASE, addr);
        trace!(
            "looking for target 0x{:X} in {} thunks",
            thunk_target,
            pe.thunks.len()
        );
        for thunk in pe.thunks.iter() {
            if thunk_target == thunk.vaddr {
                return Ok(thunk);
            }
        }

        bail!("did not find thunk with a target of {:08X}", thunk_target)
    }

    pub fn size(&self) -> usize {
        6
    }

    pub fn magic(&self) -> &'static str {
        "Tramp"
    }

    pub fn at_offset(&self) -> usize {
        self.offset
    }

    pub fn show(&self) -> String {
        format!(
            "@{:04X} {}Tramp{}: {}{}{} = {:04X}",
            self.offset,
            ansi().yellow().bold(),
            ansi(),
            ansi().yellow(),
            self.name,
            ansi(),
            self.target
        )
    }
}

#[derive(Debug, Eq, PartialEq)]
enum ReturnKind {
    Interp,
    Error,
}

impl ReturnKind {
    fn from_name(s: &str) -> Fallible<Self> {
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

    fn instr_is_relative_jump(instr: &i386::Instr) -> bool {
        match instr.memonic {
            Memonic::Call => true,
            Memonic::Jump => true,
            Memonic::Jcc(ref _cc) => true,
            _ => false,
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

    // Note: excluding return-to-trampoline and loops.
    fn find_external_jumps(base: usize, bc: &ByteCode, external_jumps: &mut HashSet<usize>) {
        let mut ip = 0;
        let ip_end = bc.size as usize;
        for instr in bc.instrs.iter() {
            ip += instr.size();
            if Self::instr_is_relative_jump(&instr) {
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

    fn find_pushed_address(target: &i386::Instr) -> Fallible<u32> {
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
        trampolines: &[X86Trampoline],
    ) -> Fallible<&X86Trampoline> {
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

    fn find_trampoline_for_offset(offset: usize, trampolines: &[X86Trampoline]) -> &X86Trampoline {
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
        trampolines: &[X86Trampoline],
    ) -> Fallible<(ByteCode, ReturnKind)> {
        // Note that there are internal calls that we need to filter out, so we
        // have to consult the list of trampolines to find the interpreter return.
        let maybe_bc =
            i386::ByteCode::disassemble_until(SHAPE_LOAD_BASE as usize + offset, code, |instrs| {
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
            });
        if let Err(e) = maybe_bc {
            i386::DisassemblyError::maybe_show(&e, &code);
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
                println!("PUSH OFFSET: {:08X}", push_value);
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

    fn make_code(bc: ByteCode, pe: &PE, offset: usize) -> Instr {
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
        pe: &PE,
        trampolines: &[X86Trampoline],
        trailer: &[Instr],
        vinstrs: &mut Vec<Instr>,
    ) -> Fallible<()> {
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
            if external_jumps.contains(&offset) {
                external_jumps.remove(&offset);
                trace!("ip reached external jump");

                let (bc, return_state) =
                    Self::disassemble_to_ret(&pe.code[*offset..], *offset, trampolines)?;
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
            if external_jumps.contains(&offset) {
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
            let maybe = RawShape::read_instr(offset, pe, trampolines, trailer, vinstrs);
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
