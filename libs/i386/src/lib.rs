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
extern crate reverse;

use reverse::bs2s;
use std::collections::HashMap;
use std::mem;

pub struct Machine {
    registers: [u32; 8],
    memory_map: HashMap<usize, usize>,
}

impl Machine {
    pub fn new() -> Self {
        Machine {
            registers: [0, 0, 0, 0, 0, 0, 0, 0],
            memory_map: HashMap::new(),
        }
    }
}

#[derive(Debug)]
enum Reg {
    EBX
}

#[derive(Debug)]
struct MemRef {
    displacement: u32
}

#[derive(Debug)]
enum Operand {
    Imm32(u32),
    Imm8(u8),
    // [reg * constant + reg + displacement]
    Memory(MemRef),
    Register(Reg)
}

#[derive(Debug)]
enum OpCode {
    Pop32(Operand),
    Push32(Operand),
    Compare(Operand, Operand),
    Jnz(Operand),
    Call(Operand),
    Return,
}

// Prefixes change the op lookup table.
enum OpTable {
    Plain1,
    Plain2,
}

struct OpPrefix {
    size: usize,
    op_table: OpTable,
    operand_size: usize,
}

impl OpPrefix {
    fn from_bytes(opcode: &[u8]) -> Self {
        match opcode[0] {
            0x66 => {
                return OpPrefix {
                    size: 1,
                    op_table: OpTable::Plain2,
                    operand_size: 2,
                };
            }
            _ => {}
        }
        return OpPrefix {
            size: 0,
            op_table: OpTable::Plain1,
            operand_size: 4,
        };
    }
}

#[derive(Debug)]
struct Instr {
    size: usize,
    op: OpCode,
}

impl Instr {
    fn disassemble(code: &[u8]) -> Vec<Instr> {
        println!("Disassembling: {}", bs2s(code));
        let mut instrs = Vec::new();
        let mut i = 0;
        while i < code.len() {
            let instr = Self::decode_one(&code[i..]);
            println!("Got {:?} at {}", instr, i + instr.size);
            i += instr.size;
            instrs.push(instr);
        }
        return instrs;
    }

    fn decode_one(code: &[u8]) -> Instr {
        let mut prefix = OpPrefix::from_bytes(code);

        let b = match code.get(prefix.size) {
            Some(b) => *b as usize,
            None => panic!("code too short"),
        };

        let mut i = prefix.size + 1;
        let op = match prefix.op_table {
            OpTable::Plain1 => match b {
                0x5B => OpCode::Pop32(Operand::Register(Reg::EBX)),
                0x68 => OpCode::Push32(Self::decode_op_imm32(&code[i..], &mut i)),
                0x75 => OpCode::Jnz(Self::decode_op_imm8(&code[i..], &mut i)),
                0xC3 => OpCode::Return,
                0xE8 => OpCode::Call(Self::decode_op_imm32(&code[i..], &mut i)),
                _ => {
                    panic!("unsupported bytecode: {:2X}", b)
                }
            }
            OpTable::Plain2 => match b {
                0x83 => {
                    OpCode::Compare(
                        Self::decode_op_rm(&code[i..], &mut i),
                        Self::decode_op_imm8(&code[i..], &mut i)
                    )
                }
                _ => unreachable!()
            }
            _ => unreachable!()
        };

        return Instr {
            size: i,
            op,
        };
    }

    fn modrm(b: u8) -> (u8, u8, u8) {
        return (b >> 6, (b >> 3) & 0b111, b & 0b111);
    }

    fn decode_op_rm(code: &[u8], offset: &mut usize) -> Operand {
        *offset += 1;
        let (mode, reg, rm) = Self::modrm(code[0]);
        println!("mod: {}, reg: {}, rm: {}", mode, reg, rm);
        return match mode {
            0b00 => {
                match rm {
                    0b101 => Operand::Memory(MemRef { displacement: Self::decode32(&code[1..], offset) }),
                    _ => unreachable!()
                }
            }
            _ => unreachable!()
        };
    }

    fn decode_op_imm32(code: &[u8], offset: &mut usize) -> Operand {
        return Operand::Imm32(Self::decode32(code, offset));
    }

    fn decode_op_imm8(code: &[u8], offset: &mut usize) -> Operand {
        return Operand::Imm8(Self::decode8(code, offset));
    }

    fn decode32(code: &[u8], offset: &mut usize) -> u32 {
        *offset += 4;
        let buf: &[u32] = unsafe { mem::transmute(code) };
        return buf[0];
    }

    fn decode8(code: &[u8], offset: &mut usize) -> u8 {
        *offset += 1;
        return code[0];
    }
}


//pub struct FakeX86 {
//}
//
//impl FakeX86 {
//    fn virtual_interpret(code: &[u8]) {
//        let mut state = FakeX86State::clean();
//        let mut ip = 0;
//        loop {
//            match code[ip] {
//                // PREFIX
//                0x66 => {
//                    state.prefix_operand_override = true;
//                    ip += 1;
//                },
//                // CMPW
//                0x83 => {
//                    // Check that this is set on all actual users.
//                    assert!(state.prefix_operand_override);
//                    let mods = code[ip + 1];
//                    let a1_ptr: &[u32] = unsafe { mem::transmute(&code[ip + 2..ip + 6]) };
//                    let a1: u32 = a1_ptr[0];
//                    let a2: u8 = code[ip + 7];
//                    ip += 2 + 4 + 1;
//                }
//                // JNE
//                0x75 => {
//
//                }
//                _ => {
//                    panic!("Unknown x86 opcode")
//                }
//            }
//        }
//    }
//}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::prelude::*;
    use super::*;

    #[test]
    fn it_works() {
        // f22.asm-10845.x86
        let paths = fs::read_dir("./test_data").unwrap();
        for i in paths {
            let entry = i.unwrap();
            let path = format!("{}", entry.path().display());
            println!("AT: {}", path);

            //if path == "./test_data/f22.asm-10845.x86" {
            if true {
                let mut fp = fs::File::open(entry.path()).unwrap();
                let mut data = Vec::new();
                fp.read_to_end(&mut data).unwrap();

                let bc = Instr::disassemble(&data);
                println!("DIS: {:?}", bc);
            }
        }
    }
}
