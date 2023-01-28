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
use crate::{Instr, RawShape, UnknownData};
use ansi::ansi;
use anyhow::Result;
use i386::{ByteCode, Disassembler, MemBlock};
use log::trace;
use once_cell::sync::Lazy;
use peff::PortableExecutable;
use std::collections::HashSet;

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

    fn make_code(bc: ByteCode, pe: &PortableExecutable) -> Instr {
        let bc_size = bc.size();
        let offset = bc.start_offset();
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

    pub fn handle_interspersed_data(
        end_addr: usize,
        offset: &mut usize,
        pe: &PortableExecutable,
        trailer: &[Instr],
        vinstrs: &mut Vec<Instr>,
    ) -> Result<()> {
        trace!(
            "handling_interspered_data at: 0x{:08X}, is_instr: {}",
            *offset,
            Instr::is_instruction_header(&pe.code[*offset..])
        );

        if Instr::is_instruction_header(&pe.code[*offset..]) {
            trace!("Instr at 0x{:04X}", *offset);
            RawShape::read_instr(offset, pe, trailer, vinstrs)?;
            if *offset > end_addr {
                let last = vinstrs.pop().unwrap();
                trace!("stripping over-eager read: {}", last.show());
                *offset -= last.size();
                assert_eq!(*offset, last.at_offset());
                assert!(*offset <= end_addr);
                panic!("here")
            }
            return Ok(());
        }

        // Find first zero byte for string checking.
        if let Some(end_offset) = (pe.code[*offset..end_addr]).iter().position(|&v| v == 0u8) {
            if let Ok(message) = std::str::from_utf8(&pe.code[*offset..*offset + end_offset]) {
                trace!(
                    "Found message at 0x{:04X}: len: {}, {}",
                    *offset,
                    message.len(),
                    message
                );
                let msg = Instr::X86Message(X86Message {
                    offset: *offset,
                    message: message.to_owned(),
                });
                *offset += msg.size();
                vinstrs.push(msg);
                return Ok(());
            }
        }

        // Fall through to just taking everything
        let length = end_addr - *offset;
        trace!("using DEBRIS.SH hack; {} pad bytes", length);
        vinstrs.push(Instr::UnknownData(UnknownData {
            offset: *offset,
            length,
            data: pe.code[*offset..*offset + length].to_owned(),
        }));
        *offset += length;
        assert_eq!(*offset, end_addr, "expected to consume to end");

        Ok(())
    }

    fn merge(&mut self, other: ByteCode) {
        self.length += other.size();
        self.bytecode.merge(other);
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

        let mut disasm = Disassembler::default();
        disasm.add_non_standard_retpoline("do_start_interp");
        disasm.add_non_standard_retpoline("_ErrorExit");
        disasm.disassemble_at(*offset + 2, pe)?;
        let view = disasm.build_memory_view(pe);
        // for block in &view {
        //     println!("{}", block);
        // }
        for block in view {
            match block {
                MemBlock::Code(bytecode) => {
                    assert!(
                        bytecode.start_offset() == *offset
                            || bytecode.start_offset() - 2 == *offset
                    );
                    trace!("Code at 0x{:08X}", *offset);
                    // TODO: Is it actually worth it to merge? Do we need to?
                    if let Some(Instr::X86Code(prior_code)) = vinstrs.last_mut() {
                        let prior_end = prior_code.bytecode.end_offset();
                        if prior_end == bytecode.start_offset() {
                            *offset += bytecode.size();
                            prior_code.merge(bytecode);
                        } else {
                            let instr = Self::make_code(bytecode, pe);
                            *offset += instr.size();
                            vinstrs.push(instr);
                        }
                    } else {
                        let instr = Self::make_code(bytecode, pe);
                        *offset += instr.size();
                        vinstrs.push(instr);
                    }
                }
                MemBlock::Data {
                    start_offset, data, ..
                } => {
                    assert_eq!(start_offset, *offset);
                    let end_offset = start_offset + data.len();

                    while *offset < end_offset && pe.code[*offset] != 0xF0 {
                        Self::handle_interspersed_data(end_offset, offset, pe, trailer, vinstrs)?;
                    }

                    // WAVE2.SH smuggles an F0_00 with an escape jump after the E4_00 in the
                    // data, but before the jump targets for continuation (skipping the E4_00)
                    // in code blocks above.
                    if *offset < end_offset {
                        trace!("using WAVE2 hack; sh-reachable x86 between blocks");
                        assert_eq!(pe.code[*offset], 0xF0);
                        assert_eq!(pe.code[*offset + 1], 0x00);
                        let fragment = &pe.code[*offset + 2..end_offset];
                        // Note: if the data ends at F0_00, it will be automagically counted
                        //       in the next code block because we extract from the PE directly.
                        if !fragment.is_empty() {
                            let bc = Disassembler::disassemble_fragment_at_virtual_offset(
                                fragment,
                                *offset + 2,
                                pe,
                            )?;
                            let instr = Self::make_code(bc, pe);
                            *offset += instr.size();
                            assert_eq!(*offset, end_offset);
                            vinstrs.push(instr);
                        }
                    }

                    #[cfg(debug_assertions)]
                    {
                        let mut expect_offset = 0;
                        for instr in vinstrs.iter() {
                            assert_eq!(
                                expect_offset,
                                instr.at_offset(),
                                "instr size and offset misaligned at 0x{expect_offset:08X}"
                            );
                            expect_offset += instr.size();
                        }
                    }
                }
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
