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
use disassembler::{ByteCode, Memonic, Operand, Reg};
use failure::Error;
use lut::{ConditionCode, ConditionCode1, ConditionCode2, FlagKind};
use std::{collections::HashMap, mem};

#[derive(Eq, Ord, PartialOrd, PartialEq)]
struct MemMap<'a> {
    start: u32,
    end: u32,
    writable: bool,
    mem: &'a Vec<u8>,
}

impl<'a> MemMap<'a> {
    fn new(start: usize, end: usize, mem: &'a Vec<u8>) -> Self {
        assert!(start < end);
        //assert!(start % 4 == 0);
        //assert!((start - end) % 4 == 0);
        assert!(end <= u32::max_value() as usize);
        Self {
            start: start as u32,
            end: end as u32,
            writable: false,
            mem,
        }
    }
}

pub struct Interpreter<'a> {
    registers: Vec<u32>,
    cf: bool,
    of: bool,
    zf: bool,
    sf: bool,
    stack: Vec<u32>,
    memmap: Vec<MemMap<'a>>,
    bytecode: HashMap<u32, &'a ByteCode>,
    ports: HashMap<u32, Box<Fn() -> u32>>,
}

impl<'a> Interpreter<'a> {
    pub fn new() -> Self {
        let mut registers = Vec::new();
        registers.resize(Reg::num_registers(), 0);
        registers[Reg::ESP.to_offset()] = 0xFFFFFFFF;
        Self {
            registers,
            cf: false,
            of: false,
            zf: false,
            sf: false,
            stack: Vec::new(),
            memmap: Vec::new(),
            bytecode: HashMap::new(),
            ports: HashMap::new(),
        }
    }

    pub fn map_memory(&mut self, start: usize, data: &'a Vec<u8>) {
        let map = MemMap::new(start, start.checked_add(data.len()).unwrap(), data);
        self.memmap.push(map);
        self.memmap.sort();
    }

    pub fn add_entry_point(&mut self, offset: u32, bc: &'a ByteCode) {
        self.bytecode.insert(offset, bc);
    }

    pub fn interpret(&mut self, at: u32) -> Result<(), Error> {
        ensure!(
            self.bytecode.contains_key(&at),
            "attempting to interpret at {:08X}, which is not in bytecode",
            at
        );
        let bc = self.bytecode[&at];
        println!("About to interpret:\n{}", bc);
        self.registers[Reg::EIP.to_offset()] = at;
        for instr in bc.instrs.iter() {
            self.registers[Reg::EIP.to_offset()] += instr.size() as u32;
            match instr.memonic {
                Memonic::PushAll => self.do_pushall()?,
                Memonic::Move => self.do_move(&instr.operands[0], &instr.operands[1])?,
                Memonic::And => self.do_and(&instr.operands[0], &instr.operands[1])?,
                Memonic::Compare => self.do_compare(&instr.operands[0], &instr.operands[1])?,
                Memonic::Jcc(ref cc) => {
                    let offset = self.do_jcc(cc, &instr.operands[0])?;
                    let next_ip = self.registers[Reg::EIP.to_offset()] + offset as u32;
                    return self.interpret(next_ip);
                }
                _ => bail!("not implemented: {}", instr),
            }
        }
        return Ok(());
    }

    fn do_pushall(&mut self) -> Result<(), Error> {
        let tmp = self.registers[Reg::ESP.to_offset()];
        self.do_push(&Operand::Register(Reg::EAX))?;
        self.do_push(&Operand::Register(Reg::ECX))?;
        self.do_push(&Operand::Register(Reg::EDX))?;
        self.do_push(&Operand::Register(Reg::EBX))?;
        self.do_push(&Operand::Imm32(tmp))?;
        self.do_push(&Operand::Register(Reg::EBP))?;
        self.do_push(&Operand::Register(Reg::ESI))?;
        self.do_push(&Operand::Register(Reg::EDI))?;
        return Ok(());
    }

    fn do_push(&mut self, op: &Operand) -> Result<(), Error> {
        match op {
            Operand::Register(reg) => {
                assert!(!reg.is_reg16());
                assert!(!reg.is_low8());
                assert!(!reg.is_high8());
                let v = self.registers[reg.to_offset()];
                self.registers[Reg::ESP.to_offset()] -= 4;
                self.stack.push(v);
            }
            Operand::Imm32(v) => {
                self.stack.push(*v);
            }
            _ => bail!("do_push do not know how to push: {}", op),
        }
        return Ok(());
    }

    fn do_move(&mut self, op1: &Operand, op2: &Operand) -> Result<(), Error> {
        let v = self.get(op2)?;
        self.put(op1, v)
    }

    fn do_and(&mut self, op1: &Operand, op2: &Operand) -> Result<(), Error> {
        let a = self.get(op1)?;
        let b = self.get(op2)?;
        return self.put(op1, a & b);
    }

    fn do_compare(&mut self, op1: &Operand, op2: &Operand) -> Result<(), Error> {
        let a = self.get(op1)?;
        let b = self.get(op2)?;
        self.cf = (a as i32) < (b as i32);
        let rv = a.checked_add(b);
        let v = if rv.is_none() {
            self.of = true;
            a + b
        } else {
            self.of = false;
            rv.unwrap()
        };
        self.zf = v == 0;
        self.sf = (v as i32) < 0;
        println!(
            "compare {:08X}, {:08X} => cf:{}, of:{}, zf:{}, sf:{}",
            a, b, self.cf, self.of, self.zf, self.sf
        );
        return Ok(());
    }

    fn do_jcc(&mut self, cc: &ConditionCode, op: &Operand) -> Result<usize, Error> {
        let should_jump = match cc {
            ConditionCode::Unary(ref cc1) => self.check_cc1(cc1),
            ConditionCode::Binary(ref cc2) => match cc2 {
                ConditionCode2::And(cc1a, cc1b) => self.check_cc1(cc1a) && self.check_cc1(cc1b),
                ConditionCode2::Or(cc1a, cc1b) => self.check_cc1(cc1a) || self.check_cc1(cc1b),
            },
        };
        if should_jump {
            let offset = self.get(op)?;
            println!("jcc -> {}", offset);
            return Ok(offset as usize);
        }
        return Ok(0);
    }

    fn check_cc1(&self, cc1: &ConditionCode1) -> bool {
        match cc1 {
            ConditionCode1::Check(flag, expect) => self.get_flag(flag) == *expect,
            ConditionCode1::Eq(flag1, flag2) => self.get_flag(flag1) == self.get_flag(flag2),
            ConditionCode1::NotEq(flag1, flag2) => self.get_flag(flag1) != self.get_flag(flag2),
        }
    }

    fn get_flag(&self, flag: &FlagKind) -> bool {
        match flag {
            FlagKind::CF => self.cf,
            FlagKind::OF => self.of,
            FlagKind::ZF => self.zf,
            FlagKind::SF => self.sf,
            FlagKind::PF => panic!("attempted read of parity flag"),
        }
    }

    fn get(&self, op: &Operand) -> Result<u32, Error> {
        Ok(match op {
            Operand::Imm32(u) => *u,
            Operand::Imm32s(i) => *i as u32,
            Operand::Register(r) => {
                let base = self.registers[r.to_offset()];
                if r.is_reg16() {
                    base & 0xFFFF
                } else if r.is_low8() {
                    base & 0xFF
                } else if r.is_high8() {
                    (base >> 8) & 0xFF
                } else {
                    base
                }
            }
            Operand::Memory(mem) => {
                // [base + index*scale + disp]
                ensure!(
                    mem.segment.is_none(),
                    "don't know how to handle segment in mem read"
                );
                ensure!(
                    mem.index.is_none(),
                    "don't know how to handle index in mem read"
                );
                let base = if let Some(ref r) = mem.base {
                    self.registers[r.to_offset()]
                } else {
                    0
                };
                //let base = mem.base.map(|r| self.registers[r.to_offset()]).unwrap_or(0);
                //let base = mem.base.map_or(0, |ref r| self.registers[r.to_offset()]);
                let addr = if mem.displacement >= 0 {
                    base + mem.displacement as u32
                } else {
                    if base > -mem.displacement as u32 {
                        base - (-mem.displacement) as u32
                    } else {
                        (base as i32 + mem.displacement) as u32
                    }
                };
                self.mem_lookup(addr)?
            }
        })
    }

    fn mem_lookup(&self, addr: u32) -> Result<u32, Error> {
        if self.ports.contains_key(&addr) {
            let v = self.ports[&addr]();
            println!("port_read: {:08X} -> {:08X}", addr, v);
            return Ok(v);
        }
        for map in self.memmap.iter() {
            println!(
                "Looking for {:08X} in {:08X}-{:08X}",
                addr, map.start, map.end
            );
            if addr >= map.start && addr < map.end {
                let rel = (addr - map.start) as usize;
                let vp: &[u32] = unsafe { mem::transmute(&map.mem[rel..rel + 4]) };
                println!("mem_read: {:08X} -> {:08X}", addr, vp[0]);
                return Ok(vp[0]);
            }
        }
        bail!("no memory or port for address: {:08X}", addr);
    }

    fn put(&mut self, op: &Operand, v: u32) -> Result<(), Error> {
        match op {
            Operand::Register(r) => {
                if r.is_reg16() {
                    println!("write_reg: {} <- {:04X}", r, v & 0xFFFF);
                    self.registers[r.to_offset()] |= v & 0xFFFF;
                } else if r.is_low8() {
                    println!("write_reg: {} <- {:02X}", r, v & 0xFF);
                    self.registers[r.to_offset()] |= v & 0xFF;
                } else if r.is_high8() {
                    println!("write_reg: {} <- {:04X}", r, (v & 0xFF) << 8);
                    self.registers[r.to_offset()] |= (v & 0xFF) << 8;
                } else {
                    println!("write_reg: {} <- {:08X}", r, v);
                    self.registers[r.to_offset()] = v;
                }
            }
            Operand::Memory(_mem) => bail!("implement memory put"),
            _ => bail!("attempted put to {}", op),
        }
        return Ok(());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::prelude::*;

    #[test]
    fn it_works() {
        //let path = "./test_data/sh/EXP.SH";
        let path = "./test_data/x86/exp.asm-1035.x86";
        let mut fp = fs::File::open(path).unwrap();
        let mut data = Vec::new();
        fp.read_to_end(&mut data).unwrap();

        let bc = ByteCode::disassemble(&data, true).unwrap();

        let mut interp = Interpreter::new();
        //interp.map_memory(0xAA000000, data);
        interp.interpret(&bc).unwrap();

        // let paths = fs::read_dir("./test_data").unwrap();
        // for i in paths {
        //     let entry = i.unwrap();
        //     let path = format!("{}", entry.path().display());
        //     println!("AT: {}", path);

        //     let mut fp = fs::File::open(entry.path()).unwrap();
        //     let mut data = Vec::new();
        //     fp.read_to_end(&mut data).unwrap();

        //     let bc = ByteCode::disassemble(&data, true);
        //     if let Err(ref e) = bc {
        //         if !DisassemblyError::maybe_show(e, &data) {
        //             println!("Error: {}", e);
        //         }
        //     }
        //     bc.unwrap();
        // }
    }
}
