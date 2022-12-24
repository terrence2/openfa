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
use crate::lut::{AddressingMethod, OpCodeDef, OperandDef, OperandType};
use ansi::ansi;
use anyhow::{bail, ensure, Result};
use log::{debug, trace};
use peff::{PortableExecutable, Trampoline};
use reverse::bs2s;
use std::{
    collections::{HashSet, VecDeque},
    fmt,
    fmt::Write,
    mem,
};
use thiserror::Error;

pub use crate::lut::{Memonic, HAS_INLINE_REG, OPCODES, PREFIX_CODES, USE_REG_OPCODES};
use std::fmt::Formatter;

#[derive(Debug, Error)]
pub enum DisassemblyError {
    #[error("unknown opcode/ext: {:?}", op)]
    UnknownOpcode { ip: usize, op: (u16, u8) },
    #[error("disassembly stopped in middle of instruction")]
    TooShort { phase: &'static str },
}

impl DisassemblyError {
    pub fn maybe_show(e: &anyhow::Error, code: &[u8]) -> bool {
        if let Some(&DisassemblyError::UnknownOpcode { ip, op: (op, ext) }) =
            e.downcast_ref::<DisassemblyError>()
        {
            debug!("Unknown OpCode: 0x{:2X} /{}", op, ext);
            let line1 = bs2s(&code[0..(ip + 20).min(code.len())]);
            let mut line2 = String::new();
            for _ in 0..(ip - 1) * 3 {
                line2 += "-";
            }
            line2 += "^";
            debug!("{}", line1);
            debug!("{}", line2);

            use std::fs::File;
            use std::io::*;
            let name = "error";
            let tmp_name = format!("/tmp/{}-{}.x86", name, 0);
            let mut file = File::create(tmp_name).unwrap();
            file.write_all(&code[0..]).unwrap();

            true
        } else {
            false
        }
    }
}

#[allow(clippy::upper_case_acronyms)]
#[derive(Clone, Debug)]
pub enum Reg {
    AL,
    BL,
    CL,
    DL,
    AH,
    BH,
    CH,
    DH,

    AX,
    BX,
    CX,
    DX,
    SP,
    BP,
    SI,
    DI,

    EAX,
    ECX,
    EDX,
    EBX,
    ESP,
    EBP,
    ESI,
    EDI,
    EIP,

    SS,
    CS,
    DS,
    ES,
    FS,
}

impl Reg {
    pub fn all_registers() -> Vec<Reg> {
        vec![
            Reg::EAX,
            Reg::EBX,
            Reg::ECX,
            Reg::EDX,
            Reg::ESP,
            Reg::EBP,
            Reg::ESI,
            Reg::EDI,
            Reg::EIP,
            Reg::SS,
            Reg::CS,
            Reg::DS,
            Reg::ES,
            Reg::FS,
        ]
    }

    pub fn num_registers() -> usize {
        13
    }

    pub fn to_offset(&self) -> usize {
        match self {
            // Unique regs
            Reg::EAX => 0,
            Reg::EBX => 1,
            Reg::ECX => 2,
            Reg::EDX => 3,
            Reg::ESP => 4,
            Reg::EBP => 5,
            Reg::ESI => 6,
            Reg::EDI => 7,
            Reg::EIP => 8,
            Reg::SS => 9,
            Reg::CS => 10,
            Reg::DS => 11,
            Reg::ES => 12,
            Reg::FS => 13,

            // 16 bit versions
            Reg::AX => 0,
            Reg::BX => 1,
            Reg::CX => 2,
            Reg::DX => 3,
            Reg::SP => 4,
            Reg::BP => 5,
            Reg::SI => 6,
            Reg::DI => 7,

            // 8 bit low
            Reg::AL => 0,
            Reg::BL => 1,
            Reg::CL => 2,
            Reg::DL => 3,

            // 8 bit high
            Reg::AH => 0,
            Reg::BH => 1,
            Reg::CH => 2,
            Reg::DH => 3,
        }
    }

    pub fn is_reg16(&self) -> bool {
        matches!(
            self,
            Reg::AX | Reg::BX | Reg::CX | Reg::DX | Reg::SP | Reg::BP | Reg::SI | Reg::DI
        )
    }

    pub fn is_low8(&self) -> bool {
        matches!(self, Reg::AL | Reg::BL | Reg::CL | Reg::DL)
    }

    pub fn is_high8(&self) -> bool {
        matches!(self, Reg::AH | Reg::BH | Reg::CH | Reg::DH)
    }
}

impl fmt::Display for Reg {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

// size @ [base + index*scale + disp]
#[derive(Clone, Debug)]
pub struct MemRef {
    pub displacement: i32,
    pub base: Option<Reg>,
    pub index: Option<Reg>,
    pub scale: u8,
    pub segment: Option<Reg>,
    pub size: u8, // one of 1, 2, or 4
}

impl MemRef {
    fn base(base: Reg, size: u8, prefix: &OpPrefix) -> Self {
        MemRef {
            displacement: 0,
            base: Some(base),
            index: None,
            scale: 1,
            segment: Self::segment(prefix),
            size,
        }
    }

    fn base_plus_segment(base: Reg, size: u8, segment: Reg) -> Self {
        MemRef {
            displacement: 0,
            base: Some(base),
            index: None,
            scale: 1,
            segment: Some(segment),
            size,
        }
    }

    fn base_plus_displacement(base: Reg, displacement: i32, size: u8, prefix: &OpPrefix) -> Self {
        MemRef {
            displacement,
            base: Some(base),
            index: None,
            scale: 1,
            segment: Self::segment(prefix),
            size,
        }
    }

    fn displacement(displacement: i32, size: u8, prefix: &OpPrefix) -> Self {
        MemRef {
            displacement,
            base: None,
            index: None,
            scale: 1,
            segment: Self::segment(prefix),
            size,
        }
    }

    fn full(
        scale: u8,
        index: Reg,
        base: Reg,
        displacement: i32,
        size: u8,
        prefix: &OpPrefix,
    ) -> Self {
        MemRef {
            displacement,
            base: Some(base),
            index: Some(index),
            scale,
            segment: Self::segment(prefix),
            size,
        }
    }

    fn segment(prefix: &OpPrefix) -> Option<Reg> {
        if prefix.use_fs_segment {
            Some(Reg::FS)
        } else {
            None
        }
    }

    fn size_for_type(ty: OperandType, state: &OperandDecodeState) -> Result<u8> {
        Ok(match ty {
            OperandType::b => 1,
            OperandType::v => {
                if state.prefix.toggle_operand_size {
                    2
                } else {
                    4
                }
            }
            OperandType::w => 2,
            _ => bail!("size_for_type for handled operand type: {:?}", ty),
        })
    }
}

impl fmt::Display for MemRef {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let seg = if let Some(ref r) = self.segment {
            format!("{:?}:", r)
        } else {
            "".to_owned()
        };
        let size = match self.size {
            1 => "BYTE PTR ",
            2 => "WORD PTR ",
            _ => "DWORD PTR ",
        };
        match (&self.base, &self.index) {
            (&Some(ref base), &Some(ref index)) => write!(
                f,
                "{}{}[{:?}+{:?}*{}+0x{:X}]",
                seg, size, base, index, self.scale, self.displacement
            ),
            (&Some(ref base), &None) => {
                write!(f, "{}{}[{:?}+0x{:X}]", seg, size, base, self.displacement)
            }
            (&None, &Some(ref index)) => write!(
                f,
                "{}{}[{:?}*{}+0x{:X}]",
                seg, size, index, self.scale, self.displacement
            ),
            (&None, &None) => write!(f, "{}[0x{:X}]", seg, self.displacement),
        }
    }
}

struct OperandDecodeState {
    prefix: OpPrefix,
    op: u16,
    modrm: Option<u8>,
}

impl OperandDecodeState {
    fn initial(prefix: OpPrefix, op: u16) -> Self {
        Self {
            prefix,
            op,
            modrm: None,
        }
    }

    fn read_modrm(&mut self, code: &[u8], ip: &mut usize) -> Result<(u8, u8, u8)> {
        if let Some(b) = self.modrm {
            return Ok(Operand::modrm(b));
        }
        ensure!(
            code.len() > *ip,
            DisassemblyError::TooShort {
                phase: "op read modrm"
            }
        );
        let b = code[*ip];
        *ip += 1;
        let out = Operand::modrm(b);
        // println!(
        //     "modrm: {:2X} => {:8b} => mod: {}, reg: {}, rm: {}",
        //     b, b, out.0, out.1, out.2
        // );
        self.modrm = Some(b);

        Ok(out)
    }

    #[allow(clippy::unusual_byte_groupings)]
    fn read_sib(&mut self, mod_: u8, code: &[u8], ip: &mut usize) -> Result<(u8, Reg, Reg)> {
        ensure!(
            code.len() > *ip,
            DisassemblyError::TooShort {
                phase: "op read sib"
            }
        );
        let sib = code[*ip];
        *ip += 1;

        let scale = sib >> 6;
        let index = (sib & 0b00_111_000) >> 3;
        let base = sib & 0b00_000_111;

        let scale = match scale {
            0 => 1,
            1 => 2,
            2 => 4,
            4 => 8,
            _ => panic!("scale out of range"),
        };
        let index = match index {
            0 => Reg::EAX,
            1 => Reg::ECX,
            2 => Reg::EDX,
            3 => Reg::EBX,
            4 => bail!("illegal index register in SIB"),
            5 => Reg::EBP,
            6 => Reg::ESI,
            7 => Reg::EDI,
            _ => panic!("index out of range"),
        };
        let base = match base {
            0 => Reg::EAX,
            1 => Reg::ECX,
            2 => Reg::EDX,
            3 => Reg::EBX,
            4 => Reg::ESP,
            5 => {
                if mod_ == 0 {
                    bail!("displacement only base in SIB; maybe technically legal?")
                } else {
                    Reg::EBP
                }
            }
            6 => Reg::ESI,
            7 => Reg::EDI,
            _ => panic!("base out of range"),
        };

        Ok((scale, index, base))
    }
}

#[derive(Clone, Debug)]
pub enum Operand {
    Imm32(u32),
    Imm32s(i32),
    Memory(MemRef),
    Register(Reg),
}

#[allow(non_snake_case)]
impl Operand {
    fn from_bytes(
        code: &[u8],
        ip: &mut usize,
        desc: &OperandDef,
        state: &mut OperandDecodeState,
    ) -> Result<Self> {
        match desc.method {
            AddressingMethod::E => Self::from_bytes_mode_E(code, ip, desc, state),
            AddressingMethod::G => Self::from_bytes_mode_G(code, ip, desc, state),
            AddressingMethod::I => Self::from_bytes_mode_I(code, ip, desc, state),
            AddressingMethod::J => Self::from_bytes_mode_J(code, ip, desc, state),
            AddressingMethod::M => Self::from_bytes_mode_E(code, ip, desc, state), // note: just a subset of E
            AddressingMethod::O => Self::from_bytes_mode_O(code, ip, desc, state),
            AddressingMethod::X => Self::from_bytes_mode_X(code, ip, desc, state),
            AddressingMethod::Y => Self::from_bytes_mode_Y(code, ip, desc, state),
            AddressingMethod::Z => Ok(Self::from_bytes_mode_Z(state)),
            AddressingMethod::Imp => Self::from_bytes_mode_Imp(desc, state),
        }
    }

    fn from_bytes_mode_E(
        code: &[u8],
        ip: &mut usize,
        desc: &OperandDef,
        state: &mut OperandDecodeState,
    ) -> Result<Self> {
        assert!(!state.prefix.toggle_address_size);
        let (mod_, _reg, rm) = state.read_modrm(code, ip)?;
        // Mod indicates the size of the displacement field after the instruction. We've split
        // on mod for now since most addressing uses no displacement and it's simpler this way,
        // but in theory we should combine these and apply displacement from mod only after
        // constructing the operand.
        Ok(match mod_ {
            0b00 => match rm {
                0 | 1 | 2 | 3 | 6 | 7 => match desc.ty {
                    OperandType::b => Operand::Memory(MemRef::base(
                        Self::register(rm),
                        MemRef::size_for_type(desc.ty, state)?,
                        &state.prefix,
                    )),
                    OperandType::v => Operand::Memory(MemRef::base(
                        Self::register(rm),
                        MemRef::size_for_type(desc.ty, state)?,
                        &state.prefix,
                    )),
                    OperandType::w => Operand::Memory(MemRef::base(
                        Self::register(rm),
                        MemRef::size_for_type(desc.ty, state)?,
                        &state.prefix,
                    )),
                    _ => unreachable!("E/00/0-7"),
                },
                4 => {
                    let (scale, index, base) = state.read_sib(mod_, code, ip)?;
                    Operand::Memory(MemRef::full(
                        scale,
                        index,
                        base,
                        0,
                        MemRef::size_for_type(desc.ty, state)?,
                        &state.prefix,
                    ))
                }
                5 => Operand::Memory(MemRef::displacement(
                    Self::read4(code, ip)? as i32,
                    MemRef::size_for_type(desc.ty, state)?,
                    &state.prefix,
                )),
                _ => unreachable!("E/00"),
            },
            0b01 => {
                let base = Self::register(rm);
                let disp8 = Self::read1(code, ip)?;
                Operand::Memory(MemRef::base_plus_displacement(
                    base,
                    i32::from(disp8 as i8),
                    MemRef::size_for_type(desc.ty, state)?,
                    &state.prefix,
                ))
            }
            0b10 => {
                let base = Self::register(rm);
                let disp32 = Self::read4(code, ip)?;
                Operand::Memory(MemRef::base_plus_displacement(
                    base,
                    disp32 as i32,
                    MemRef::size_for_type(desc.ty, state)?,
                    &state.prefix,
                ))
            }
            0b11 => match desc.ty {
                OperandType::b => Operand::Register(Self::register_low(rm)),
                OperandType::w => Operand::Register(Self::register_word(rm)),
                OperandType::v => Operand::Register(Self::maybe_toggle_reg_size(
                    Self::register(rm),
                    state.prefix.toggle_operand_size,
                )),
                _ => unreachable!("E/11"),
            },
            _ => unreachable!("E/"),
        })
    }

    fn from_bytes_mode_G(
        code: &[u8],
        ip: &mut usize,
        desc: &OperandDef,
        state: &mut OperandDecodeState,
    ) -> Result<Self> {
        let (_mod, reg, _rm) = state.read_modrm(code, ip)?;
        Ok(match desc.ty {
            OperandType::b => Operand::Register(Self::register_low(reg)),
            OperandType::v => Operand::Register(Self::maybe_toggle_reg_size(
                Self::register(reg),
                state.prefix.toggle_operand_size,
            )),
            _ => unreachable!(),
        })
    }

    fn from_bytes_mode_I(
        code: &[u8],
        ip: &mut usize,
        desc: &OperandDef,
        state: &mut OperandDecodeState,
    ) -> Result<Self> {
        Ok(match desc.ty {
            OperandType::b => Operand::Imm32(u32::from(Self::read1(code, ip)?)),
            OperandType::bs => Operand::Imm32s(i32::from(Self::read1(code, ip)? as i8)),
            OperandType::v => Self::read_n_32(code, ip, state.prefix.toggle_operand_size, false)?,
            OperandType::vs => Self::read_n_32(code, ip, state.prefix.toggle_operand_size, true)?,
            _ => unreachable!(),
        })
    }

    fn from_bytes_mode_J(
        code: &[u8],
        ip: &mut usize,
        desc: &OperandDef,
        state: &mut OperandDecodeState,
    ) -> Result<Self> {
        Ok(match desc.ty {
            OperandType::bs => Operand::Imm32s(i32::from(Self::read1(code, ip)? as i8)),
            OperandType::v => Self::read_n_32(code, ip, state.prefix.toggle_operand_size, false)?,
            _ => unreachable!(),
        })
    }

    fn from_bytes_mode_O(
        code: &[u8],
        ip: &mut usize,
        desc: &OperandDef,
        state: &mut OperandDecodeState,
    ) -> Result<Self> {
        assert!(!state.prefix.toggle_address_size);
        Ok(match desc.ty {
            OperandType::v => Operand::Memory(MemRef::displacement(
                Self::read4(code, ip)? as i32,
                MemRef::size_for_type(desc.ty, state)?,
                &state.prefix,
            )),
            _ => unreachable!(),
        })
    }

    fn from_bytes_mode_X(
        _code: &[u8],
        _ip: &mut usize,
        desc: &OperandDef,
        state: &mut OperandDecodeState,
    ) -> Result<Self> {
        assert!(!state.prefix.toggle_address_size);
        Ok(Operand::Memory(MemRef::base_plus_segment(
            Self::maybe_toggle_reg_size(Reg::ESI, state.prefix.toggle_operand_size),
            MemRef::size_for_type(desc.ty, state)?,
            Reg::DS,
        )))
    }

    fn from_bytes_mode_Y(
        _code: &[u8],
        _ip: &mut usize,
        desc: &OperandDef,
        state: &mut OperandDecodeState,
    ) -> Result<Self> {
        assert!(!state.prefix.toggle_address_size);
        Ok(Operand::Memory(MemRef::base_plus_segment(
            Self::maybe_toggle_reg_size(Reg::EDI, state.prefix.toggle_operand_size),
            MemRef::size_for_type(desc.ty, state)?,
            Reg::ES,
        )))
    }

    fn from_bytes_mode_Z(state: &mut OperandDecodeState) -> Self {
        Operand::Register(Self::maybe_toggle_reg_size(
            Self::register((state.op & 0b111) as u8),
            state.prefix.toggle_operand_size,
        ))
    }

    fn from_bytes_mode_Imp(desc: &OperandDef, state: &mut OperandDecodeState) -> Result<Self> {
        Ok(match desc.ty {
            OperandType::eAX => Operand::Register(Self::maybe_toggle_reg_size(
                Reg::EAX,
                state.prefix.toggle_operand_size,
            )),
            OperandType::eDX => Operand::Register(Self::maybe_toggle_reg_size(
                Reg::EDX,
                state.prefix.toggle_operand_size,
            )),
            OperandType::AL => Operand::Register(Reg::AL),
            OperandType::SS => Operand::Register(Reg::SS),
            OperandType::const1 => Operand::Imm32(1),
            unk => bail!("unexpected operand type for Imp mode: {:?}", unk),
        })
    }

    fn register(b: u8) -> Reg {
        match b {
            0 => Reg::EAX,
            1 => Reg::ECX,
            2 => Reg::EDX,
            3 => Reg::EBX,
            4 => Reg::ESP,
            5 => Reg::EBP,
            6 => Reg::ESI,
            7 => Reg::EDI,
            _ => unreachable!(),
        }
    }

    fn register_word(b: u8) -> Reg {
        match b {
            0 => Reg::AX,
            1 => Reg::CX,
            2 => Reg::DX,
            3 => Reg::BX,
            4 => Reg::SP,
            5 => Reg::BP,
            6 => Reg::SI,
            7 => Reg::DI,
            _ => unreachable!(),
        }
    }

    fn register_low(b: u8) -> Reg {
        match b {
            0 => Reg::AL,
            1 => Reg::CL,
            2 => Reg::DL,
            3 => Reg::BL,
            4 => Reg::AH,
            5 => Reg::CH,
            6 => Reg::DH,
            7 => Reg::BH,
            _ => unreachable!(),
        }
    }

    fn maybe_toggle_reg_size(reg: Reg, toggle_operand_size: bool) -> Reg {
        if toggle_operand_size {
            match reg {
                Reg::EAX => Reg::AX,
                Reg::EBX => Reg::BX,
                Reg::ECX => Reg::CX,
                Reg::EDX => Reg::DX,
                Reg::ESI => Reg::SI,
                Reg::EBP => Reg::BP,
                _ => unreachable!(),
            }
        } else {
            reg
        }
    }

    fn read_n_32(
        code: &[u8],
        ip: &mut usize,
        toggle_size: bool,
        sign_extend: bool,
    ) -> Result<Operand> {
        Ok(if toggle_size {
            let uw = Self::read2(code, ip)?;
            if sign_extend {
                Operand::Imm32s(i32::from(uw as i16))
            } else {
                Operand::Imm32(u32::from(uw))
            }
        } else {
            let ud = Self::read4(code, ip)?;
            if sign_extend {
                Operand::Imm32s(ud as i32)
            } else {
                Operand::Imm32(ud)
            }
        })
    }

    fn read1(code: &[u8], ip: &mut usize) -> Result<u8> {
        ensure!(
            code.len() > *ip,
            DisassemblyError::TooShort { phase: "op read 1" }
        );
        let b = code[*ip];
        *ip += 1;
        Ok(b)
    }

    fn read2(code: &[u8], ip: &mut usize) -> Result<u16> {
        ensure!(
            code.len() > *ip + 1,
            DisassemblyError::TooShort { phase: "op read 2" }
        );
        let r: &[u16] = unsafe { mem::transmute(&code[*ip..]) };
        let w = r[0];
        *ip += 2;
        Ok(w)
    }

    fn read4(code: &[u8], ip: &mut usize) -> Result<u32> {
        ensure!(
            code.len() > *ip + 3,
            DisassemblyError::TooShort { phase: "op read 4" }
        );
        let r: &[u32] = unsafe { mem::transmute(&code[*ip..]) };
        let dw = r[0];
        *ip += 4;
        Ok(dw)
    }

    fn modrm(b: u8) -> (u8, u8, u8) {
        (b >> 6, (b >> 3) & 0b111, b & 0b111)
    }

    pub fn size(&self) -> u8 {
        match self {
            Operand::Imm32(_) => 4,
            Operand::Imm32s(_) => 4,
            Operand::Register(r) => {
                if r.is_low8() || r.is_high8() {
                    1
                } else if r.is_reg16() {
                    2
                } else {
                    4
                }
            }
            Operand::Memory(mem) => mem.size,
        }
    }

    fn show_relative(&self, base: usize, show_target: bool) -> String {
        match self {
            Operand::Register(ref r) => format!("{:?}", r),
            Operand::Imm32(ref x) => {
                if show_target {
                    format!("0x{:X} -> 0x{:X}", x, *x as usize + base)
                } else {
                    format!("0x{:X}", x)
                }
            }
            Operand::Imm32s(ref x) => {
                if show_target {
                    format!("0x{:X} -> 0x{:X}", x, i64::from(*x) + base as i64)
                } else {
                    format!("0x{:X}", x)
                }
            }
            Operand::Memory(ref mr) => format!("{}", mr),
        }
    }
}

impl fmt::Display for Operand {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        //write!(f, "{}", self.show_relative(0, false))
        match self {
            Operand::Register(ref r) => write!(f, "{:?}", r),
            Operand::Imm32(x) => write!(f, "0x{:X}", x),
            Operand::Imm32s(x) => write!(f, "0x{:X}", x),
            Operand::Memory(ref mr) => write!(f, "{}", mr),
        }
    }
}

#[derive(Debug)]
struct OpPrefix {
    toggle_address_size: bool,
    toggle_operand_size: bool,
    use_fs_segment: bool,
    toggle_repeat: bool,
}

impl OpPrefix {
    fn default() -> Self {
        OpPrefix {
            toggle_address_size: false,
            toggle_operand_size: false,
            use_fs_segment: false,
            toggle_repeat: false,
        }
    }

    fn apply(mut self, b: u8) -> Result<Self> {
        match b {
            0x64 => self.use_fs_segment = true,
            0x66 => self.toggle_operand_size = true,
            0x67 => self.toggle_address_size = true,
            0xF3 => self.toggle_repeat = true,
            _ => bail!("not an op prefix: {}", b),
        }
        Ok(self)
    }

    fn from_bytes(code: &[u8], ip: &mut usize) -> Result<Self> {
        let mut prefix = Self::default();
        while *ip < code.len() && PREFIX_CODES.contains(&code[*ip]) {
            prefix = prefix.apply(code[*ip])?;
            *ip += 1;
        }
        Ok(prefix)
    }
}

#[derive(Clone, Debug)]
pub struct Instr {
    pub memonic: Memonic,
    pub operands: Vec<Operand>,
    pub raw: Vec<u8>,
    pub context: Option<String>,
}

impl Instr {
    pub fn size(&self) -> usize {
        self.raw.len()
    }

    pub fn op(&self, i: usize) -> &Operand {
        &self.operands[i]
    }

    fn read_op(code: &[u8], ip: &mut usize) -> Result<(u16, u8)> {
        ensure!(
            code.len() > *ip,
            DisassemblyError::TooShort { phase: "read_op" }
        );
        let mut op = u16::from(code[*ip]);
        *ip += 1;
        if op == 0x0Fu16 {
            op <<= 8;
            op |= u16::from(code[*ip]);
            *ip += 1;
        }
        let op_ext = if USE_REG_OPCODES.contains(&op) {
            ensure!(
                code.len() > *ip,
                DisassemblyError::TooShort {
                    phase: "decode_op_ext"
                }
            );
            let (_, ext, _) = Operand::modrm(code[*ip]);
            ext
        } else {
            0
        };
        Ok((op, op_ext))
    }

    fn lookup_op<'a>(op: (u16, u8), ip: &mut usize) -> Result<&'a OpCodeDef> {
        if OPCODES.contains_key(&op) {
            return Ok(&OPCODES[&op]);
        }

        // If there is no exact match, then this may be an opcode with the reg embedded in
        // the low bits, so retry with those masked off.
        let base_op = (op.0 & !0b111, 0);
        if HAS_INLINE_REG.contains(&base_op.0) && OPCODES.contains_key(&base_op) {
            return Ok(&OPCODES[&base_op]);
        }

        Err(DisassemblyError::UnknownOpcode { ip: *ip, op }.into())
    }

    pub fn decode_one(code: &[u8], ipo: &mut usize) -> Result<Instr> {
        let initial_ip = *ipo;

        let prefix = OpPrefix::from_bytes(code, ipo)?;

        let op = Self::read_op(code, ipo)?;

        let opcode_desc = Self::lookup_op(op, ipo)?;

        let mut operands = Vec::new();
        let mut decode_state = OperandDecodeState::initial(prefix, op.0);
        for operand_desc in opcode_desc.operands.iter() {
            operands.push(Operand::from_bytes(
                code,
                ipo,
                operand_desc,
                &mut decode_state,
            )?);
        }
        Ok(Instr {
            memonic: opcode_desc.memonic,
            operands,
            raw: code[initial_ip..*ipo].to_vec(),
            context: None,
        })
    }

    pub fn is_jump(&self) -> bool {
        matches!(
            self.memonic,
            Memonic::Jump | Memonic::Call | Memonic::Jcc(_)
        )
    }

    pub fn show_relative(&self, base: usize) -> String {
        let mut s = format!(
            "{}{:24}{} {:?}(",
            ansi().green(),
            bs2s(&self.raw),
            ansi(),
            self.memonic
        );
        for (i, op) in self.operands.iter().enumerate() {
            if i != 0 {
                s += ", ";
            }
            s += &op.show_relative(base + self.size(), self.is_jump());
        }
        s += ")";
        if let Some(ctx) = &self.context {
            s += " [";
            s += ctx;
            s += "]";
        }
        s
    }

    pub fn set_context(&mut self, context: &str) {
        self.context = Some(context.to_owned());
    }
}

impl fmt::Display for Instr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:24} {:?}(", bs2s(&self.raw), self.memonic)?;
        for (i, op) in self.operands.iter().enumerate() {
            if i != 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", op)?;
        }
        write!(f, ")")?;
        if let Some(ctx) = &self.context {
            write!(f, " {}", ctx)?;
        }
        Ok(())
    }
}

// A chunk of code or data to map into the address space.
pub enum MemBlock {
    Code(ByteCode),
    Data {
        start_offset: usize,
        start_addr: u32,
        data: Vec<u8>,
    },
}

impl fmt::Display for MemBlock {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Code(block) => write!(f, "{}", block),
            Self::Data {
                start_offset, data, ..
            } => {
                writeln!(f, "  @{:04X}: {}", start_offset, bs2s(data))?;
                writeln!(
                    f,
                    "  ascii: {}",
                    std::str::from_utf8(data).unwrap_or("not text")
                )
            }
        }
    }
}

#[derive(Default)]
pub struct Disassembler {
    // All offsets are from pe.code start.
    blocks: Vec<ByteCode>,

    // Configuration.
    non_standard_retpoline_names: HashSet<String>,
}

impl Disassembler {
    // Configure with any retpoline targets that expect a non-x86 stack (and thus where the first
    // push target is not a jump back into x86.)
    pub fn add_non_standard_retpoline(&mut self, name: &str) {
        self.non_standard_retpoline_names.insert(name.to_owned());
    }

    pub fn disassemble_fragment_at_virtual_offset(
        code: &[u8],
        offset: usize,
        pe: &PortableExecutable,
    ) -> Result<ByteCode> {
        let mut instrs = Vec::new();
        let initial_ip = 0;
        let mut ip = 0;
        while ip < code.len() {
            let instr = Instr::decode_one(code, &mut ip)?;
            debug!("  @{:04X}: {}", ip, instr);
            instrs.push(instr);
        }
        Ok(ByteCode {
            start_offset: initial_ip + offset,
            start_addr: u32::try_from(initial_ip)?.wrapping_add(pe.relocation_target),
            size: ip - initial_ip,
            instrs,
        })
    }

    /// Find all bytecode and data blocks in the fragment at offset. Uses configuration
    /// for knowing what targets should exit. We can start at an offset into .code, but
    /// we don't know how far to decode because of the variable word length.
    pub fn disassemble_at(&mut self, offset: usize, pe: &PortableExecutable) -> Result<()> {
        // Start with the implicit jump into this code block.
        let mut jump_targets = VecDeque::new();
        jump_targets.push_back(offset);

        // Follow all jumps until there is nothing left to follow.
        while !jump_targets.is_empty() {
            let jump_offset = jump_targets.pop_front().unwrap();
            debug!(
                "disassembling at 0x{:08X} with {} targets remaining",
                jump_offset,
                jump_targets.len()
            );

            // Check if we have already disassembled a block at this offset.
            if !self.has_disassembled_at(jump_offset) {
                let (block, new_jump_targets) = self.disassemble_block_at(jump_offset, pe)?;
                debug!(
                    "disassembled block of {} instrs at 0x{:08X}",
                    block.instrs.len(),
                    jump_offset
                );
                self.insert_block(block);
                debug!("adding {} new jump targets", new_jump_targets.len(),);
                for tgt in new_jump_targets {
                    if let Err(cursor) = jump_targets.binary_search(&tgt) {
                        jump_targets.insert(cursor, tgt);
                    }
                }
            } else {
                debug!("skipping duplicate block {:04X}", jump_offset);
            }
        }

        Ok(())
    }

    // Insert in sorted order
    fn insert_block(&mut self, block: ByteCode) {
        if let Err(cursor) = self
            .blocks
            .binary_search_by_key(&block.start_offset, |v| v.start_offset)
        {
            self.blocks.insert(cursor, block);
        }
    }

    fn has_disassembled_at(&self, offset: usize) -> bool {
        for bc in &self.blocks {
            if offset >= bc.start_offset && offset < (bc.start_offset + bc.size) {
                return true;
            }
        }
        false
    }

    // Execution (may) continue in a straight path until an unconditional jump happens.
    // Conditional jumps off the mainline path expose other blocks that may need to be
    // decoded. Some of these jumps will be above. Some of these jumps maybe into another
    // block. We don't really care about this, but do need to track it.
    fn disassemble_block_at(
        &self,
        offset: usize,
        pe: &PortableExecutable,
    ) -> Result<(ByteCode, HashSet<usize>)> {
        let mut jump_targets = HashSet::new();
        let mut instrs = Vec::new();
        let initial_ipo = offset;
        let mut ipo = offset;
        while ipo < pe.code.len() {
            let mut instr = Instr::decode_one(&pe.code, &mut ipo)?;
            Self::annotate_instruction(
                &mut instr,
                if instrs.is_empty() {
                    None
                } else {
                    Some(&instrs[instrs.len() - 1])
                },
                pe,
            );
            debug!("  @{:04X}: {}", ipo, instr);
            instrs.push(instr);
            if let Some(jump_target) = self.maybe_jump_target(&instrs, ipo, pe) {
                jump_targets.insert(jump_target);
            }
            if Self::is_unconditional_jump(instrs[instrs.len() - 1].memonic) {
                break;
            }
        }
        Ok((
            ByteCode {
                start_offset: initial_ipo,
                size: ipo - initial_ipo,
                start_addr: u32::try_from(initial_ipo)?.wrapping_add(pe.relocation_target),
                instrs,
            },
            jump_targets,
        ))
    }

    fn annotate_instruction(instr: &mut Instr, prev: Option<&Instr>, pe: &PortableExecutable) {
        let mut context = None;
        for op in &instr.operands {
            if let Operand::Memory(ref mr) = op {
                let mt = Self::find_trampoline_for_target(mr.displacement as u32, &pe.trampolines);
                if let Ok(tramp) = mt {
                    context = Some(tramp.name.to_owned());
                }
            }
        }
        if let Some(s) = context {
            instr.set_context(&s);
        }
        if instr.memonic == Memonic::Return {
            if let Some(push) = prev {
                if push.memonic == Memonic::Push {
                    if let Operand::Imm32s(v) = push.operands[0] {
                        let push_value = (v as u32).wrapping_sub(pe.relocation_target) as usize;
                        let trampoline =
                            Self::find_trampoline_for_offset(push_value, &pe.trampolines);
                        instr.set_context(&trampoline.name);
                    }
                }
            }
        }
    }

    // If the given instruction is a jump, return the target offset (from pe.code start).
    fn maybe_jump_target(
        &self,
        instrs: &[Instr],
        ip: usize,
        pe: &PortableExecutable,
    ) -> Option<usize> {
        let instr = &instrs[instrs.len() - 1];
        match instr.memonic {
            Memonic::Jump | Memonic::Jcc(_) => {
                return Some(match &instr.operands[0] {
                    // IP is already at end of instruction
                    Operand::Imm32s(delta) => ip as isize + *delta as isize,
                    // Note that backwards jumps can be encoded as unsigned in the instruction.
                    Operand::Imm32(delta) => ip as isize + *delta as i32 as isize,
                    _ => {
                        panic!("unknown operand kind for jump: {:?}", instr.operands[0]);
                    }
                } as usize);
            }
            Memonic::Return => {
                // If the return is not predictable, we don't need to track it, since we
                // will only have gotten here via a matching call. If the prior op is a
                // push, however, we are using a ret-poline and should track the target.
                if instrs.len() < 2 {
                    return None;
                }
                let push = &instrs[instrs.len() - 2];
                if push.memonic == Memonic::Push {
                    if let Operand::Imm32s(v) = push.operands[0] {
                        let tramp_target = (v as u32).wrapping_sub(pe.relocation_target) as usize;
                        let trampoline =
                            Self::find_trampoline_for_offset(tramp_target, &pe.trampolines);

                        // TODO: maybe there is a better way to filter out do_start_interp?
                        if self.non_standard_retpoline_names.contains(&trampoline.name) {
                            return None;
                        }

                        if instrs.len() < 3 {
                            panic!(
                                "unhandled push operand kind before ret: {:?}",
                                push.operands[0]
                            );
                        }
                        let retaddr = &instrs[instrs.len() - 3];
                        if retaddr.memonic == Memonic::Push {
                            match retaddr.operands[0] {
                                Operand::Imm32s(v) => {
                                    let ret_target =
                                        (v as u32).wrapping_sub(pe.relocation_target) as usize;
                                    return Some(ret_target);
                                }
                                _ => panic!("unknown operand kind for retpoline push"),
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        None
    }

    fn is_unconditional_jump(memonic: Memonic) -> bool {
        matches!(memonic, Memonic::Jump | Memonic::Return)
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

    pub fn blocks(&self) -> impl Iterator<Item = &ByteCode> {
        self.blocks.iter()
    }

    pub fn build_memory_view(mut self, pe: &PortableExecutable) -> Vec<MemBlock> {
        let mut out = Vec::new();
        for (i, block) in self.blocks.drain(..).enumerate() {
            if i == 0 {
                out.push(MemBlock::Code(block));
            } else {
                let prev = &out[out.len() - 1];
                let prev_end: usize = match prev {
                    MemBlock::Code(bc) => bc.end_offset(),
                    MemBlock::Data { .. } => panic!("data should always abut next block"),
                };
                assert!(prev_end <= block.start_offset());
                if prev_end < block.start_offset() {
                    out.push(MemBlock::Data {
                        start_offset: prev_end,
                        // Safe because we try_from for code blocks and this is derived.
                        start_addr: (prev_end as u32).wrapping_add(pe.relocation_target),
                        data: pe.code[prev_end..block.start_offset].to_owned(),
                    });
                }
                out.push(MemBlock::Code(block));
            }
        }
        out
    }
}

#[derive(Clone, Debug)]
pub struct ByteCode {
    pub(crate) instrs: Vec<Instr>,

    // Initial offset within the code fragment passed to code.
    start_offset: usize,

    // Size in bytes of this code.
    size: usize,

    // Address when loaded into memory.
    start_addr: u32,
}

impl ByteCode {
    #[inline]
    pub fn size(&self) -> usize {
        self.size
    }

    #[inline]
    pub fn start_offset(&self) -> usize {
        self.start_offset
    }

    #[inline]
    pub fn end_offset(&self) -> usize {
        self.start_offset + self.size
    }

    #[inline]
    pub fn start_address(&self) -> u32 {
        self.start_addr
    }

    #[inline]
    pub fn end_address(&self) -> u32 {
        self.start_addr + self.size as u32
    }

    pub fn instrs(&self) -> impl Iterator<Item = &Instr> {
        self.instrs.iter()
    }

    pub fn merge(&mut self, mut other: ByteCode) {
        assert_eq!(self.start_offset + self.size, other.start_offset);
        self.size += other.size;
        self.instrs.append(&mut other.instrs);
    }

    pub fn show_relative(&self, base: usize) -> String {
        let mut pos = self.start_offset;
        let mut s = String::new();
        for instr in self.instrs.iter() {
            writeln!(
                s,
                "  @{:04X}|{:04X}               {}",
                pos,
                base + pos,
                instr.show_relative(base + pos)
            )
            .ok();
            pos += instr.size();
        }
        s
    }
}

impl fmt::Display for ByteCode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut pos = self.start_offset;
        for instr in self.instrs.iter() {
            writeln!(f, "  @{:04X}: {}", pos, instr)?;
            pos += instr.size();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lib::Libs;

    #[test]
    fn it_works() -> Result<()> {
        env_logger::init();

        let libs = Libs::for_testing()?;
        for (game, _palette, catalog) in libs.all() {
            for fid in catalog.find_with_extension("MC")? {
                let meta = catalog.stat(fid)?;
                println!(
                    "At: {:>7}:{:13} @ {}",
                    game.test_dir,
                    meta.name(),
                    meta.path()
                );

                let data = catalog.read(fid)?;
                let mut pe = peff::PortableExecutable::from_bytes(&data)?;
                pe.relocate(0xAA00_0000)?;

                let mut disasm = Disassembler::default();
                disasm.disassemble_at(0, &pe)?;
            }
        }

        Ok(())
    }
}
