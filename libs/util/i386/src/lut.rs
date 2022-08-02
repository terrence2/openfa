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
use lazy_static::lazy_static;
use std::collections::{HashMap, HashSet};

// Specifies where to find the operand.
#[derive(Clone, Debug)]
pub enum AddressingMethod {
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

    // The ModR/M byte may refer only to memory: mod != 11bin (BOUND, LEA, CALLF, JMPF, LES, LDS,
    // LSS, LFS, LGS, CMPXCHG8B, CMPXCHG16B, F20FF0 LDDQU).
    M,

    // The instruction has no ModR/M byte; the offset of the operand is coded as a word, double
    // word or quad word (depending on address size attribute) in the instruction. No base register,
    // index register, or scaling factor can be applied (only MOV  (A0, A1, A2, A3)).
    O,

    // Memory addressed by the DS:eSI or by RSI (only MOVS, CMPS, OUTS, and LODS). In 64-bit mode,
    // only 64-bit (RSI) and 32-bit (ESI) address sizes are supported. In non-64-bit modes, only
    // 32-bit (ESI) and 16-bit (SI) address sizes are supported.
    X,

    // Memory addressed by the ES:eDI or by RDI (only MOVS, CMPS, INS, STOS, and SCAS). In 64-bit
    // mode, only 64-bit (RDI) and 32-bit (EDI) address sizes are supported. In non-64-bit modes,
    // only 32-bit (EDI) and 16-bit (DI) address sizes are supported. The implicit ES segment
    // register cannot be overriden by a segment prefix.
    Y,

    // The instruction has no ModR/M byte; the three least-significant bits of the opcode byte
    // selects a general-purpose register
    Z,

    // Custom flag indicating that the Method/Type is an implicit register or constant in this
    // operand. The OperandType is the register or constant.
    Imp,
}

// Specifies what size the operand is.
#[derive(Clone, Copy, Debug)]
#[allow(non_camel_case_types, clippy::upper_case_acronyms)]
pub enum OperandType {
    // Byte, regardless of operand-size attribute.
    b,

    // Byte, sign-extended to the size of the destination operand.
    bs,

    // Word or doubleword, depending on operand-size attribute (for example, INC (40), PUSH (50)).
    v,

    // Word or doubleword sign extended to the size of the stack pointer (for example, PUSH (68)).
    vs,

    // Word, regardless of operand-size attribute (for example, ENTER).
    w,

    // Implicit register.
    eAX,
    eDX,
    AL,
    ES,
    SS,

    // Implicit values
    const1,
}

#[derive(Clone, Debug)]
pub struct OperandDef {
    pub method: AddressingMethod,
    pub ty: OperandType,
    // In theory we could also be stuffing more data about each operand into this struct.
    // For example:
    //
    // is_implicit: bool,
    // is_target: bool,
}

macro_rules! make_operand {
    ($meth0:ident / $type0:ident) => {
        OperandDef {
            method: AddressingMethod::$meth0,
            ty: OperandType::$type0,
        }
    };
}

#[allow(clippy::upper_case_acronyms)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FlagKind {
    ZF,
    CF,
    SF,
    OF,
    PF,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConditionCode1 {
    Check(FlagKind, bool),
    Eq(FlagKind, FlagKind),
    NotEq(FlagKind, FlagKind),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConditionCode2 {
    Or(ConditionCode1, ConditionCode1),
    And(ConditionCode1, ConditionCode1),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConditionCode {
    Unary(ConditionCode1),
    Binary(ConditionCode2),
}

macro_rules! make_cc1 {
    ($flag:ident = $value:tt) => {
        ConditionCode1::Check(FlagKind::$flag, $value == 1)
    };

    ($flag0:ident == $flag1:ident) => {
        ConditionCode1::Eq(FlagKind::$flag0, FlagKind::$flag1)
    };

    ($flag0:ident != $flag1:ident) => {
        ConditionCode1::NotEq(FlagKind::$flag0, FlagKind::$flag1)
    };
}

macro_rules! make_cc2 {
    ($a:tt $op0:tt $b:tt || $c:tt $op1:tt $d:tt) => {
        ConditionCode2::Or(make_cc1!($a $op0 $b), make_cc1!($c $op1 $d))
    };

    ($a:tt $op0:tt $b:tt && $c:tt $op1:tt $d:tt) => {
        ConditionCode2::And(make_cc1!($a $op0 $b), make_cc1!($c $op1 $d))
    };
}

#[allow(clippy::upper_case_acronyms)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Memonic {
    Adc,
    Add,
    And,
    Call,
    ClearDF,
    Compare,
    Debugger,
    Dec,
    Div,
    Jump,
    Jcc(ConditionCode),
    Inc,
    IDiv,
    IMul3,
    IMul2,
    LEA,
    Move,
    MoveStr,
    MoveZX,
    Mul,
    Neg,
    Or,
    Pop,
    PopAll,
    Push,
    PushAll,
    Return,
    RotCR,
    ShiftR,
    Sar,
    ShiftL,
    Sub,
    Test,
    Xor,
}

#[derive(Clone, Debug)]
pub struct OpCodeDef {
    pub memonic: Memonic,
    pub operands: Vec<OperandDef>,
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

    (J|$a:tt $op:tt $b:tt: $( $meth0:ident / $type0:ident ),* ) => {
        OpCodeDef {
            memonic: Memonic::Jcc(ConditionCode::Unary(make_cc1!($a $op $b))),
            operands: vec![
                $( make_operand!($meth0/$type0) ),*
            ]
        }
    };

    (J|$a:tt $op0:tt $b:tt $cmp:tt $c:tt $op1:tt $d:tt: $( $meth0:ident / $type0:ident ),* ) => {
        OpCodeDef {
            memonic: Memonic::Jcc(ConditionCode::Binary(make_cc2!($a $op0 $b $cmp $c $op1 $d))),
            operands: vec![
                $( make_operand!($meth0/$type0) ),*
            ]
        }
    };
}

lazy_static! {
    pub static ref PREFIX_CODES: HashSet<u8> = {
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
            .cloned()
            .collect()
    };

    pub static ref USE_REG_OPCODES: HashSet<u16> = {
        [0x80,
         0x81,
         0x82,
         0x83,
         0x8F,
         0xC0,
         0xC1,
         0xC6,
         0xC7,
         0xD0,
         0xD1,
         0xD2,
         0xD3,
         0xD8,
         0xD9,
         0xDA,
         0xDB,
         0xDC,
         0xDD,
         0xDE,
         0xDF,
         0xF6,
         0xF7,
         0xFE,
         0xFF]
            .iter()
            .cloned()
            .collect()
    };

    pub static ref HAS_INLINE_REG: HashSet<u16> = {
        [0x50, 0x58, 0xB8].iter().cloned().collect()
    };

    #[cfg_attr(rustfmt, rustfmt_skip)]
    pub static ref OPCODES: HashMap<(u16, u8), OpCodeDef> = {
        let mut out: HashMap<(u16, u8), OpCodeDef> = HashMap::new();
        let ops = [
            (0x00, 0, make_op!(Add:     E/b, G/b)),
            (0x02, 0, make_op!(Add:     G/b, E/b)),
            (0x03, 0, make_op!(Add:     G/v, E/v)),
            (0x05, 0, make_op!(Add:     Imp/eAX, I/v)),
            (0x07, 0, make_op!(Pop:     Imp/ES)),
            (0x0B, 0, make_op!(Or:      G/v, E/v)),
            (0x0D, 0, make_op!(Or:      Imp/eAX, I/v)),
            (0x16, 0, make_op!(Push:    Imp/SS)),
            (0x22, 0, make_op!(And:     G/b, E/b)),
            (0x25, 0, make_op!(And:     Imp/eAX, I/v)),
            (0x2A, 0, make_op!(Sub:     G/b, E/b)),
            (0x2B, 0, make_op!(Sub:     G/v, E/v)),
            (0x2D, 0, make_op!(Sub:     Imp/eAX, I/v)),
            (0x32, 0, make_op!(Xor:     G/b, E/b)),
            (0x33, 0, make_op!(Xor:     G/v, E/v)),
            (0x3A, 0, make_op!(Compare: G/b, E/b)),
            (0x3B, 0, make_op!(Compare: G/v, E/v)),
            (0x3C, 0, make_op!(Compare: Imp/AL, I/b)),
            (0x3D, 0, make_op!(Compare: Imp/eAX, I/v)),
            (0x40, 0, make_op!(Inc:     Z/v)),
            (0x41, 0, make_op!(Inc:     Z/v)),
            (0x42, 0, make_op!(Inc:     Z/v)),
            (0x43, 0, make_op!(Inc:     Z/v)),
            (0x44, 0, make_op!(Inc:     Z/v)),
            (0x45, 0, make_op!(Inc:     Z/v)),
            (0x46, 0, make_op!(Inc:     Z/v)),
            (0x47, 0, make_op!(Inc:     Z/v)),
            (0x48, 0, make_op!(Dec:     Z/v)),
            (0x49, 0, make_op!(Dec:     Z/v)),
            (0x4A, 0, make_op!(Dec:     Z/v)),
            (0x4B, 0, make_op!(Dec:     Z/v)),
            (0x4C, 0, make_op!(Dec:     Z/v)),
            (0x4D, 0, make_op!(Dec:     Z/v)),
            (0x4E, 0, make_op!(Dec:     Z/v)),
            (0x4F, 0, make_op!(Dec:     Z/v)),
            (0x50, 0, make_op!(Push:    Z/v)),
            (0x58, 0, make_op!(Pop:     Z/v)),
            (0x60, 0, make_op!(PushAll:)),
            (0x61, 0, make_op!(PopAll:)),
            (0x68, 0, make_op!(Push:    I/vs)),
            (0x6B, 0, make_op!(IMul3:   G/v, E/v, I/bs)),
            (0x70, 0, make_op!(J|OF=1:  J/bs)),
            (0x71, 0, make_op!(J|OF=0:  J/bs)),
            (0x72, 0, make_op!(J|CF=1:  J/bs)),
            (0x73, 0, make_op!(J|CF=0:  J/bs)),
            (0x74, 0, make_op!(J|ZF=1:  J/bs)),
            (0x75, 0, make_op!(J|ZF=0:  J/bs)),
            (0x76, 0, make_op!(J|CF=1||ZF=1: J/bs)),
            (0x77, 0, make_op!(J|CF=0&&ZF=0: J/bs)),
            (0x78, 0, make_op!(J|SF=1:  J/bs)),
            (0x79, 0, make_op!(J|SF=0:  J/bs)),
            (0x7A, 0, make_op!(J|PF=1:  J/bs)),
            (0x7B, 0, make_op!(J|PF=0:  J/bs)),
            (0x7C, 0, make_op!(J|SF!=OF:J/bs)),
            (0x7D, 0, make_op!(J|SF==OF:J/bs)),
            (0x7E, 0, make_op!(J|ZF=1||SF!=OF: J/bs)),
            (0x7F, 0, make_op!(J|ZF=0&&SF==OF: J/bs)),
            (0x80, 0, make_op!(Add:     E/b, I/b)),
            (0x80, 2, make_op!(Adc:     E/b, I/b)),
            (0x80, 7, make_op!(Compare: E/b, I/b)),
            (0x81, 0, make_op!(Add:     E/v, I/v)),
            (0x81, 1, make_op!(Or:      E/v, I/v)),
            (0x81, 4, make_op!(And:     E/v, I/v)),
            (0x81, 7, make_op!(Compare: E/v, I/v)),
            (0x82, 0, make_op!(Add:     E/b, I/b)),
            (0x83, 0, make_op!(Add:     E/v, I/bs)),
            (0x83, 1, make_op!(Or:      E/v, I/bs)),
            (0x83, 4, make_op!(And:     E/v, I/bs)),
            (0x83, 5, make_op!(Sub:     E/v, I/bs)),
            (0x83, 7, make_op!(Compare: E/v, I/bs)),
            (0x88, 0, make_op!(Move:    E/b, G/b)),
            (0x89, 0, make_op!(Move:    E/v, G/v)),
            (0x8A, 0, make_op!(Move:    G/b, E/b)),
            (0x8B, 0, make_op!(Move:    G/v, E/v)),
            (0x8D, 0, make_op!(LEA:     G/v, M/v)),
            (0xA1, 0, make_op!(Move:    Imp/eAX, O/v)),
            (0xA3, 0, make_op!(Move:    O/v, Imp/eAX)),
            (0xA4, 0, make_op!(MoveStr: Y/b, X/b)),
            (0xB8, 0, make_op!(Move:    Z/v, I/v)),
            (0xC1, 4, make_op!(ShiftL:  E/v, I/b)),
            (0xC1, 5, make_op!(ShiftR:  E/v, I/b)),
            (0xC1, 7, make_op!(Sar:     E/v, I/b)),
            (0xC3, 0, make_op!(Return:)),
            (0xC6, 0, make_op!(Move:    E/b, I/b)),
            (0xC7, 0, make_op!(Move:    E/v, I/v)),
            (0xCC, 0, make_op!(Debugger:)),
            (0xD1, 1, make_op!(RotCR:   E/v, Imp/const1)),
            (0xD1, 4, make_op!(ShiftL:  E/v)),
            (0xD1, 7, make_op!(Sar:     E/v, Imp/const1)),
            (0xE8, 0, make_op!(Call:    J/v)),
            (0xE9, 0, make_op!(Jump:    J/v)),
            (0xEB, 0, make_op!(Jump:    J/bs)),
            (0xF7, 0, make_op!(Test:    E/v, I/v)),
            (0xF7, 3, make_op!(Neg:     E/v)),
            (0xF7, 4, make_op!(Mul:     Imp/eDX, Imp/eAX, E/v)),
            (0xF7, 5, make_op!(IMul3:   Imp/eDX, Imp/eAX, E/v)),
            (0xF7, 6, make_op!(Div:     Imp/eDX, Imp/eAX, E/v)),
            (0xF7, 7, make_op!(IDiv:    Imp/eDX, Imp/eAX, E/v)),
            (0xFC, 0, make_op!(ClearDF:)),
            (0xFF, 1, make_op!(Dec:     E/v)),
            (0xFF, 4, make_op!(Jump:    E/v)),

            (0x0F83, 0, make_op!(J|CF=0: J/v)),
            (0x0F84, 0, make_op!(J|ZF=1: J/v)),
            (0x0F85, 0, make_op!(J|ZF=0: J/v)),
            (0x0FAF, 0, make_op!(IMul2:  G/v, E/v)),
            (0x0FB6, 0, make_op!(MoveZX: G/v, E/b)), // move byte to word or doubleword with zero extension
            (0x0FB7, 0, make_op!(MoveZX: G/v, E/w)), // move word to doubleword with zero extension
        ];
        for &(ref op, ref ext, ref def) in ops.iter() {
            out.insert((*op, *ext), (*def).clone());
        }
        out
    };
}
