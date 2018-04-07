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
#[macro_use]
extern crate failure;
#[macro_use]
extern crate lazy_static;
extern crate reverse;

use failure::Error;
use reverse::bs2s;
use std::collections::{HashMap, HashSet};
use std::{fmt, mem};

#[derive(Debug, Fail)]
pub enum DisassemblyError {
    #[fail(display = "unknown opcode/ext: {:?}", op)]
    UnknownOpcode {
        ip: usize,
        op: (u8, u8),
    },
    #[fail(display = "disassembly stopped in middle of instruction")]
    TooShort {
        phase: &'static str,
    },
}

//pub struct Machine {
//    registers: [u32; 8],
//    memory_map: HashMap<usize, usize>,
//}
//
//impl Machine {
//    pub fn new() -> Self {
//        Machine {
//            registers: [0, 0, 0, 0, 0, 0, 0, 0],
//            memory_map: HashMap::new(),
//        }
//    }
//}

#[derive(Clone, Copy, Debug)]
enum FlagKind {
    ZF,
    CF,
    SF,
    OF,
}

#[derive(Clone, Copy, Debug)]
enum ConditionCode {
    Check(FlagKind, bool),
    CompareEq(FlagKind, FlagKind),
    CompareNEq(FlagKind, FlagKind),
}

#[derive(Clone, Copy, Debug)]
enum Memonic {
    //Adc,
    Add,
    //And,
    Call,
    Compare,
    Dec,
    Jump,
    Jcc(ConditionCode),
    //    JccAnd(ConditionCode, ConditionCode),
//    JccOr(ConditionCode, ConditionCode),
    Move,
    Neg,
    Or,
    Pop,
    Push,
    Return,
    Sar,
    Shl,
    //Sbb,
    Sub,
    Test,
}

/// Specifies where to find the operand.
#[derive(Clone)]
enum AddressingMethod {
    // A ModR/M byte follows the opcode and specifies the operand. The operand is either a
    // general-purpose register or a memory address. If it is a memory address, the address is
    // computed from a segment register and any of the following values: a base register, an index
    // register, a scaling factor, or a displacement.
    E,

    // The reg field of the ModR/M byte selects a general register (for example, AX (000)).
    G,

    // Immediate data. The operand value is encoded in subsequent bytes of the instruction.
    I,

    // The instruction contains a relative offset to be added to the instruction pointer register
    // (for example, JMP (E9), LOOP)).
    J,

    // The instruction has no ModR/M byte; the offset of the operand is coded as a word, double
    // word or quad word (depending on address size attribute) in the instruction. No base register,
    // index register, or scaling factor can be applied (only MOV  (A0, A1, A2, A3)).
    O,

    // The instruction has no ModR/M byte; the three least-significant bits of the opcode byte
    // selects a general-purpose register
    Z,

    // Custom flag indicating that the Method/Type is an implicit register or constant in this
    // operand. The OperandType is the register or constant.
    Imp,
}

/// Specifies what size the operand is.
#[derive(Clone)]
#[allow(non_camel_case_types)]
enum OperandType {
    // Byte, regardless of operand-size attribute.
    b,

    // Byte, sign-extended to the size of the destination operand.
    bs,

    // Word or doubleword, depending on operand-size attribute (for example, INC (40), PUSH (50)).
    v,

    // Word or doubleword sign extended to the size of the stack pointer (for example, PUSH (68)).
    vs,

    // Implicit register.
    eAX,

    // Implicit values
    const1,
}

#[derive(Debug)]
enum Reg {
    AL,
    BL,
    CL,
    DL,
    AH,
    BH,
    CH,
    DH,

    AX,
    CX,

    EAX,
    ECX,
    EDX,
    EBX,
    ESP,
    EBP,
    ESI,
    EDI,
}

#[derive(Debug)]
struct MemRef {
    displacement: i32,
    base: Option<Reg>,
    index: Option<Reg>,
    scale: u8,
}

impl MemRef {
    fn base(base: Reg) -> Self {
        MemRef {
            displacement: 0,
            base: Some(base),
            index: None,
            scale: 1,
        }
    }

    fn base_plus_displacement(base: Reg, displacement: i32) -> Self {
        MemRef {
            displacement,
            base: Some(base),
            index: None,
            scale: 1,
        }
    }

    fn displacement(displacement: i32) -> Self {
        MemRef {
            displacement,
            base: None,
            index: None,
            scale: 1,
        }
    }
}

struct OperandDecodeState {
    prefix: OpPrefix,
    op: u8,
    modrm: Option<u8>,
}

impl OperandDecodeState {
    fn initial(prefix: OpPrefix, op: u8) -> Self {
        Self {
            prefix,
            op,
            modrm: None,
        }
    }

    fn read_modrm(&mut self, code: &[u8], ip: &mut usize) -> Result<(u8, u8, u8), Error> {
        if let Some(b) = self.modrm {
            return Ok(Operand::modrm(b));
        }
        ensure!(code.len() > *ip, DisassemblyError::TooShort {phase: "op read modrm"});
        let b = code[*ip];
        *ip += 1;
        let out = Operand::modrm(b);
        println!("modrm: {:2X} => mod: {}, reg: {}, rm: {}", b, out.0, out.1, out.2);
        self.modrm = Some(b);

        return Ok(out);
    }
}

#[derive(Debug)]
enum Operand {
    Imm32(u32),
    Imm32s(i32),
    Memory(MemRef),
    Register(Reg),
}

impl Operand {
    fn from_bytes(code: &[u8], ip: &mut usize, desc: &OperandDef, state: &mut OperandDecodeState) -> Result<Self, Error> {
        // Each of the two operands may use the singular regrm.

        Ok(match desc.method {
            AddressingMethod::E => {
                let (mode, _reg, rm) = state.read_modrm(code, ip)?;
                match mode {
                    0b00 => {
                        match rm {
                            0 | 1 | 2 | 3 => {
                                match desc.ty {
                                    OperandType::b => {
                                        Operand::Memory(MemRef::base(Self::register_low(rm)))
                                    }
                                    _ => unreachable!(),
                                }
                            }
                            0b101 => {
                                assert!(!state.prefix.toggle_address_size);
                                Operand::Memory(MemRef::displacement(Self::read4(code, ip)? as i32))
                            }
                            _ => unreachable!(),
                        }
                    },
                    0b01 => {
                        let base = Self::register(rm);
                        let disp8 = Self::read1(code, ip)?;
                        Operand::Memory(MemRef::base_plus_displacement(base, disp8 as i8 as i32))
                    }
                    0b10 => {
                        let base = Self::register(rm);
                        let disp32 = Self::read4(code, ip)?;
                        Operand::Memory(MemRef::base_plus_displacement(base, disp32 as i32))
                    }
                    0b11 => {
                        match desc.ty {
                            OperandType::b => Operand::Register(Self::register_low(rm)),
                            OperandType::v => Operand::Register(Self::maybe_toggle_reg_size(Self::register(rm), state.prefix.toggle_operand_size)),
                            _ => unreachable!()
                        }
                    }
                    _ => unreachable!(),
                }
            }
            AddressingMethod::G => {
                let (_mod, reg, _rm) = state.read_modrm(code, ip)?;
                match desc.ty {
                    OperandType::b => Operand::Register(Self::register_low(reg)),
                    OperandType::v => Operand::Register(Self::maybe_toggle_reg_size(Self::register(reg), state.prefix.toggle_operand_size)),
                    _ => unreachable!()
                }
            }
            AddressingMethod::I => {
                match desc.ty {
                    OperandType::b => {
                        Operand::Imm32(Self::read1(code, ip)? as u32)
                    }
                    OperandType::bs => {
                        Operand::Imm32s(Self::read1(code, ip)? as i8 as i32)
                    }
                    OperandType::v => {
                        Self::read_n_32(code, ip, state.prefix.toggle_operand_size, false)?
                    }
                    OperandType::vs => {
                        Self::read_n_32(code, ip, state.prefix.toggle_operand_size, true)?
                    }
                    _ => unreachable!()
                }
            }
            AddressingMethod::J => {
                match desc.ty {
                    OperandType::bs => {
                        Operand::Imm32s(Self::read1(code, ip)? as i8 as i32)
                    }
                    OperandType::v => {
                        Self::read_n_32(code, ip, state.prefix.toggle_operand_size, false)?
                    }
                    _ => unreachable!()
                }
            }
            AddressingMethod::O => {
                match desc.ty {
                    OperandType::v => {
                        Operand::Memory(MemRef::displacement(Self::read4(code, ip)? as i32))
                    }
                    _ => unreachable!()
                }
            }
            AddressingMethod::Z => {
                Operand::Register(Self::maybe_toggle_reg_size(Self::register(state.op & 0b111), state.prefix.toggle_operand_size))
            }
            AddressingMethod::Imp => {
                match desc.ty {
                    OperandType::eAX => Operand::Register(Self::maybe_toggle_reg_size(Reg::EAX, state.prefix.toggle_operand_size)),
                    OperandType::const1 => Operand::Imm32(1),
                    _ => unreachable!()
                }
            }
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
            _ => unreachable!()
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
            _ => unreachable!()
        }
    }

    fn maybe_toggle_reg_size(reg: Reg, toggle_operand_size: bool) -> Reg {
        if toggle_operand_size {
            match reg {
                Reg::EAX => Reg::AX,
                Reg::ECX => Reg::CX,
                _ => unreachable!()
            }
        } else {
            reg
        }
    }

    fn read_n_32(code: &[u8], ip: &mut usize, toggle_size: bool, sign_extend: bool) -> Result<Operand, Error> {
        Ok(if toggle_size {
            let uw = Self::read2(code, ip)?;
            if sign_extend {
                Operand::Imm32s(uw as i16 as i32)
            } else {
                Operand::Imm32(uw as u32)
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

    fn read1(code: &[u8], ip: &mut usize) -> Result<u8, Error> {
        ensure!(code.len() > *ip, DisassemblyError::TooShort {phase: "op read 1"});
        let b = code[*ip];
        *ip += 1;
        return Ok(b);
    }

    fn read2(code: &[u8], ip: &mut usize) -> Result<u16, Error> {
        ensure!(code.len() > *ip + 1, DisassemblyError::TooShort {phase: "op read 2"});
        let r: &[u16] = unsafe { mem::transmute(&code[*ip..]) };
        let w = r[0];
        *ip += 2;
        return Ok(w);
    }

    fn read4(code: &[u8], ip: &mut usize) -> Result<u32, Error> {
        ensure!(code.len() > *ip + 3, DisassemblyError::TooShort {phase: "op read 4"});
        let r: &[u32] = unsafe { mem::transmute(&code[*ip..]) };
        let dw = r[0];
        *ip += 4;
        return Ok(dw);
    }

    fn modrm(b: u8) -> (u8, u8, u8) {
        return (b >> 6, (b >> 3) & 0b111, b & 0b111);
    }
}

impl fmt::Display for Operand {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            &Operand::Register(ref r) => write!(f, "{:?}", r),
            &Operand::Imm32(x) => write!(f, "0x{:X}", x),
            &Operand::Imm32s(x) => write!(f, "0x{:X}", x),
            &Operand::Memory(ref mr) => {
                match (&mr.base, &mr.index) {
                    (&Some(ref base), &Some(ref index)) =>
                        write!(f, "[{:?}+{:?}*{}+0x{:X}]", base, index, mr.scale, mr.displacement),
                    (&Some(ref base), &None) =>
                        write!(f, "[{:?}+0x{:X}]", base, mr.displacement),
                    (&None, &Some(ref index)) =>
                        write!(f, "[{:?}*{}+0x{:X}]", index, mr.scale, mr.displacement),
                    (&None, &None) =>
                        write!(f, "[0x{:X}]", mr.displacement),
                }
            }
        }
    }
}

#[derive(Clone)]
struct OperandDef {
    // is_implicit: bool,
    // is_target: bool,
    method: AddressingMethod,
    ty: OperandType,
}

#[derive(Clone)]
struct OpCodeDef {
    memonic: Memonic,
    operands: Vec<OperandDef>,
}

macro_rules! make_operand {
    ($meth0:ident / $type0:ident) => {
        OperandDef {
            method: AddressingMethod::$meth0,
            ty: OperandType::$type0
        }
    }
}

macro_rules! make_op {
    ($meme:ident: $( $meth0:ident / $type0:ident ),* ) => {
        OpCodeDef {
            memonic: Memonic::$meme,
            operands: vec![
                $( make_operand!($meth0/$type0) ),*
            ]
        }
    };

    (J|$flag:ident=$value:tt: $( $meth0:ident / $type0:ident ),* ) => {
        OpCodeDef {
            memonic: Memonic::Jcc(ConditionCode::Check(FlagKind::$flag, $value == 1)),
            operands: vec![
                $( make_operand!($meth0/$type0) ),*
            ]
        }
    };

    (J|$flag0:ident==$flag1:ident: $( $meth0:ident / $type0:ident ),* ) => {
        OpCodeDef {
            memonic: Memonic::Jcc(ConditionCode::CompareEq(FlagKind::$flag0, FlagKind::$flag1)),
            operands: vec![
                $( make_operand!($meth0/$type0) ),*
            ]
        }
    };

    (J|$flag0:ident!=$flag1:ident: $( $meth0:ident / $type0:ident ),* ) => {
        OpCodeDef {
            memonic: Memonic::Jcc(ConditionCode::CompareNEq(FlagKind::$flag0, FlagKind::$flag1)),
            operands: vec![
                $( make_operand!($meth0/$type0) ),*
            ]
        }
    }
}

lazy_static! {
    static ref PREFIX_CODES: HashSet<u8> = {
        [0x26u8,
         0x2Eu8,
         0x36u8,
         0x3Eu8,
         0x64u8,
         0x65u8,
         0x66u8,
         0x67u8,
         0x9Bu8,
         0xF0u8,
         0xF2u8,
         0xF3u8]
            .iter()
            .map(|&n| n)
            .collect()
    };

    static ref USE_REG_OPCODES: HashSet<u8> = {
        [0x80u8,
         0x81u8,
         0x82u8,
         0x83u8,
         0x8Fu8,
         0xC0u8,
         0xC1u8,
         0xC6u8,
         0xC7u8,
         0xD0u8,
         0xD1u8,
         0xD2u8,
         0xD3u8,
         0xD8u8,
         0xD9u8,
         0xDAu8,
         0xDBu8,
         0xDCu8,
         0xDDu8,
         0xDEu8,
         0xDFu8,
         0xF6u8,
         0xF7u8,
         0xFEu8,
         0xFFu8]
            .iter()
            .map(|&n| n)
            .collect()
    };

    static ref HAS_INLINE_REG: HashSet<u8> = {
        [0x50, 0x58, 0xB8].iter().map(|&n| n).collect()
    };

    static ref OPCODE_TABLE: HashMap<(u8, u8), OpCodeDef> = {
        let mut out: HashMap<(u8, u8), OpCodeDef> = HashMap::new();
        let ops = [
            (0x00, 0, make_op!(Add:     E/b, G/b)),
            (0x05, 0, make_op!(Add:     Imp/eAX, I/v)),
            (0x0B, 0, make_op!(Or:      G/v, E/v)),
            (0x2B, 0, make_op!(Sub:     G/v, E/v)),
            (0x2D, 0, make_op!(Sub:     Imp/eAX, I/v)),
            (0x3D, 0, make_op!(Compare: Imp/eAX, I/v)),
            (0x48, 0, make_op!(Dec:     Z/v)),
            (0x50, 0, make_op!(Push:    Z/v)),
            (0x58, 0, make_op!(Pop:     Z/v)),
            (0x68, 0, make_op!(Push:    I/vs)),
            (0x72, 0, make_op!(J|CF=1:  J/bs)),
            (0x73, 0, make_op!(J|CF=0:  J/bs)),
            (0x74, 0, make_op!(J|ZF=1:  J/bs)),
            (0x75, 0, make_op!(J|ZF=0:  J/bs)),
            (0x7C, 0, make_op!(J|SF!=OF: J/bs)),
            (0x7D, 0, make_op!(J|SF==OF: J/bs)),
            (0x80, 7, make_op!(Compare: E/b, I/b)),
            (0x81, 0, make_op!(Add:     E/v, I/v)),
            (0x81, 1, make_op!(Or:      E/v, I/v)),
            //(0x81, 4, make_op!(And:     E/v, I/v)),
            (0x83, 1, make_op!(Or:      E/v, I/bs)),
            //(0x83, 2, make_op!(Adc:     E/v, I/bs)),
            (0x83, 7, make_op!(Compare: E/v, I/bs)),
            //(0x83, 3, make_op!(Sbb:     E/v, I/bs)),
            (0x89, 0, make_op!(Move:    E/v, G/v)),
            (0x8B, 0, make_op!(Move:    G/v, E/v)),
            (0xA1, 0, make_op!(Move:    Imp/eAX, O/v)),
            (0xB8, 0, make_op!(Move:    Z/v, I/v)),
            (0xC1, 4, make_op!(Shl:     E/v, I/b)),
            (0xC1, 7, make_op!(Sar:     E/v, I/b)),
            (0xC3, 0, make_op!(Return:)),
            (0xD1, 7, make_op!(Sar:     E/v, Imp/const1)),
            (0xE8, 0, make_op!(Call:    J/v)),
            (0xEB, 0, make_op!(Jump:    J/bs)),
            (0xF7, 0, make_op!(Test:    E/v, I/v)),
            (0xF7, 3, make_op!(Neg:     E/v)),
        ];
        for &(ref op, ref ext, ref def) in ops.iter() {
            out.insert((*op, *ext), (*def).clone());
        }
        return out;
    };
}

struct OpPrefix {
    toggle_address_size: bool,
    toggle_operand_size: bool,
}

impl OpPrefix {
    fn default() -> Self {
        OpPrefix {
            toggle_address_size: false,
            toggle_operand_size: false,
        }
    }

    fn apply(mut self, b: u8) -> Self {
        match b {
            0x66 => self.toggle_operand_size = true,
            0x67 => self.toggle_address_size = true,
            _ => unreachable!()
        }
        return self;
    }

    fn from_bytes(code: &[u8], ip: &mut usize) -> Self {
        let mut prefix = Self::default();
        while *ip < code.len() && PREFIX_CODES.contains(&code[*ip]) {
            prefix = prefix.apply(code[*ip]);
            *ip += 1;
        }
        return prefix;
    }
}

#[derive(Debug)]
pub struct Instr {
    memonic: Memonic,
    operands: Vec<Operand>,
    raw: Vec<u8>,
}

impl Instr {
    pub fn disassemble(code: &[u8], verbose: bool) -> Result<Vec<Instr>, Error> {
        if verbose {
            println!("Disassembling: {}", bs2s(code));
        }
        let mut instrs = Vec::new();
        let mut ip = 0usize;
        while ip < code.len() {
            let instr = Self::decode_one(code, &mut ip)?;
            if verbose {
                println!("  @{}: {}", ip, instr);
            }
            instrs.push(instr);
        }
        return Ok(instrs);
    }

    fn read_op(code: &[u8], ip: &mut usize) -> Result<(u8, u8), Error> {
        ensure!(code.len() > *ip, DisassemblyError::TooShort{phase: "read_op"});
        let op = code[*ip];
        *ip += 1;
        let op_ext =
            if USE_REG_OPCODES.contains(&op) {
                ensure!(code.len() > *ip, DisassemblyError::TooShort{phase: "decode_op_ext"});
                let (_, ext, _) = Operand::modrm(code[*ip]);
                ext
            } else {
                0
            };
        return Ok((op, op_ext));
    }

    fn lookup_op<'a>(op: &(u8, u8), ip: &mut usize) -> Result<&'a OpCodeDef, Error> {
        if OPCODE_TABLE.contains_key(&op) {
            return Ok(&OPCODE_TABLE[&op]);
        }

        // If there is no exact match, then this may be an opcode with the reg embedded in
        // the low bits, so retry with those masked off.
        let base_op = (op.0 & !0b111, 0);
        if HAS_INLINE_REG.contains(&base_op.0) {
            if OPCODE_TABLE.contains_key(&base_op) {
                return Ok(&OPCODE_TABLE[&base_op]);
            }
        }

        return Err(DisassemblyError::UnknownOpcode { ip: *ip, op: *op }.into());
    }

    pub fn decode_one(code: &[u8], ip: &mut usize) -> Result<Instr, Error> {
        let initial_ip = *ip;

        let prefix = OpPrefix::from_bytes(code, ip);

        let op = Self::read_op(code, ip)?;

        let opcode_desc = Self::lookup_op(&op, ip)?;

        let mut operands = Vec::new();
        let mut decode_state = OperandDecodeState::initial(prefix, op.0);
        for operand_desc in opcode_desc.operands.iter() {
            operands.push(Operand::from_bytes(code, ip, operand_desc, &mut decode_state)?);
        }
        return Ok(Instr { memonic: opcode_desc.memonic, operands, raw: code[initial_ip..*ip].to_vec() });
    }
}

impl fmt::Display for Instr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:23} {:?}(", bs2s(&self.raw), self.memonic)?;
        for (i, op) in self.operands.iter().enumerate() {
            if i != 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", op)?;
        }
        write!(f, ")")?;
        return Ok(());
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::prelude::*;
    use super::*;

    #[test]
    fn it_works() {
        let paths = fs::read_dir("./test_data").unwrap();
        for i in paths {
            let entry = i.unwrap();
            let path = format!("{}", entry.path().display());
            println!("AT: {}", path);

            let mut fp = fs::File::open(entry.path()).unwrap();
            let mut data = Vec::new();
            fp.read_to_end(&mut data).unwrap();

            let bc = Instr::disassemble(&data, true);
            if let Err(ref e) = bc {
                if let Some(&DisassemblyError::UnknownOpcode { ip: ip, op: (op, ext) }) = e.downcast_ref::<DisassemblyError>() {
                    println!("Unknown OpCode: {:2X} /{}", op, ext);
                    let line1 = bs2s(&data);
                    let mut line2 = String::new();
                    for i in 0..(ip - 1) * 3 {
                        line2 += "-";
                    }
                    line2 += "^";
                    println!("{}\n{}", line1, line2);
                }
                println!("Error: {}", e);
            }
            bc.unwrap();
        }
    }
}
