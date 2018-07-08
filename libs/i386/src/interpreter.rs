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
use disassembler::{ByteCode, MemRef, Memonic, Operand, Reg};
use failure::Error;
use lut::{ConditionCode, ConditionCode1, ConditionCode2, FlagKind};
use std::{collections::HashMap, mem};

pub enum ExitInfo {
    OutOfInstructions,
    Trampoline(String, Vec<u32>),
}

#[derive(Eq, Ord, PartialOrd, PartialEq)]
struct MemMapR<'a> {
    start: u32,
    end: u32,
    mem: &'a Vec<u8>,
}

impl<'a> MemMapR<'a> {
    fn new(start: u32, end: u32, mem: &'a Vec<u8>) -> Self {
        assert!(start < end);
        Self { start, end, mem }
    }
}

#[derive(Eq, Ord, PartialOrd, PartialEq)]
struct MemMapW {
    start: u32,
    end: u32,
    mem: Vec<u8>,
}

impl MemMapW {
    fn new(start: u32, end: u32, mem: Vec<u8>) -> Self {
        assert!(start < end);
        Self { start, end, mem }
    }
}

pub struct Interpreter<'a> {
    registers: Vec<u32>,
    cf: bool,
    of: bool,
    zf: bool,
    sf: bool,
    stack: Vec<u32>,
    memmap_w: Vec<MemMapW>,
    memmap_r: Vec<MemMapR<'a>>,
    bytecode: Vec<&'a ByteCode>,
    ports_r: HashMap<u32, Box<Fn() -> u32>>,
    ports_w: HashMap<u32, Box<Fn(u32)>>,
    trampolines: HashMap<u32, (String, usize)>,
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
            memmap_r: Vec::new(),
            memmap_w: Vec::new(),
            bytecode: Vec::new(),
            ports_r: HashMap::new(),
            ports_w: HashMap::new(),
            trampolines: HashMap::new(),
        }
    }

    pub fn add_trampoline(&mut self, addr: u32, name: &str, arg_count: usize) {
        self.trampolines.insert(addr, (name.to_owned(), arg_count));
    }

    pub fn add_read_port(&mut self, addr: u32, func: Box<Fn() -> u32>) {
        self.ports_r.insert(addr, func);
    }

    pub fn add_write_port(&mut self, addr: u32, func: Box<Fn(u32)>) {
        self.ports_w.insert(addr, func);
    }

    pub fn map_readonly(&mut self, start: u32, data: &'a Vec<u8>) -> Result<(), Error> {
        ensure!(
            data.len() < u32::max_value() as usize,
            "readonly data segment too large"
        );
        let end = start.checked_add(data.len() as u32);
        ensure!(
            end.is_some(),
            "readonly data segment overflowed when mapped at {:08X}",
            start
        );
        let map = MemMapR::new(start, end.unwrap(), data);
        self.memmap_r.push(map);
        self.memmap_r.sort();
        return Ok(());
    }

    pub fn map_writable(&mut self, start: u32, data: Vec<u8>) -> Result<(), Error> {
        ensure!(
            data.len() < u32::max_value() as usize,
            "readonly data segment too large"
        );
        let end = start.checked_add(data.len() as u32);
        ensure!(
            end.is_some(),
            "readonly data segment overflowed when mapped at {:08X}",
            start
        );
        let map = MemMapW::new(start, end.unwrap(), data);
        self.memmap_w.push(map);
        self.memmap_w.sort();
        return Ok(());
    }

    pub fn add_code(&mut self, bc: &'a ByteCode) {
        self.bytecode.push(bc);
    }

    fn find_instr(&self) -> Result<(&'a ByteCode, usize), Error> {
        trace!("searching for instr at ip: {:08X}", self.eip());
        for bc in self.bytecode.iter() {
            if self.eip() >= bc.start_addr && self.eip() < bc.start_addr + bc.size {
                trace!("in bc at {:08X}", bc.start_addr);
                let mut pos = bc.start_addr;
                let mut offset = 0;
                for instr in bc.instrs.iter() {
                    trace!("checking {}: {:08X} of {:08X}", offset, pos, self.eip());
                    if pos == self.eip() {
                        return Ok((bc, offset));
                    }
                    pos += instr.size() as u32;
                    offset += 1;
                }
                bail!(
                    "attempted to jump to {:08X}, which is not aligned to any instruction in {}",
                    self.eip(),
                    bc
                );
            }
        }
        bail!(
            "attempted to jump to {:08X}, which is not in any code section",
            self.eip()
        );
    }

    pub fn interpret(&mut self, at: u32) -> Result<ExitInfo, Error> {
        *self.eip_mut() = at;
        let (bc, mut offset) = self.find_instr()?;
        while offset < bc.instrs.len() {
            let instr = &bc.instrs[offset];
            println!("{:3}:{:04X}: {}", offset, self.eip(), instr);
            offset += 1;
            *self.eip_mut() += instr.size() as u32;
            match instr.memonic {
                Memonic::PushAll => self.do_pushall()?,
                Memonic::PopAll => self.do_popall()?,
                Memonic::Push => self.do_push(instr.op(0))?,
                Memonic::Pop => self.do_pop(instr.op(0))?,
                Memonic::Move => self.do_move(instr.op(0), instr.op(1))?,
                Memonic::MoveStr => self.do_move_str(instr.op(0), instr.op(1))?,
                Memonic::Dec => self.do_dec(instr.op(0))?,
                Memonic::Inc => self.do_inc(instr.op(0))?,
                Memonic::Neg => self.do_neg(instr.op(0))?,
                Memonic::Add => self.do_add(instr.op(0), instr.op(1))?,
                Memonic::Adc => self.do_adc(instr.op(0), instr.op(1))?,
                Memonic::Sub => self.do_sub(instr.op(0), instr.op(1))?,
                Memonic::IDiv => self.do_idiv(instr.op(0), instr.op(1), instr.op(2))?,
                Memonic::IMul => self.do_imul(instr.op(0), instr.op(1), instr.op(2))?,
                Memonic::IMul2 => self.do_imul2(instr.op(0), instr.op(1))?,
                Memonic::And => self.do_and(instr.op(0), instr.op(1))?,
                Memonic::Or => self.do_or(instr.op(0), instr.op(1))?,
                Memonic::Xor => self.do_xor(instr.op(0), instr.op(1))?,
                Memonic::RotCR => self.do_rcr(instr.op(0), instr.op(1))?,
                Memonic::ShiftL => self.do_shl(instr.op(0), instr.op(1))?,
                Memonic::ShiftR => self.do_shr(instr.op(0), instr.op(1))?,
                Memonic::Sar => self.do_sar(instr.op(0), instr.op(1))?,
                Memonic::LEA => self.do_lea(instr.op(0), instr.op(1))?,
                Memonic::Compare => self.do_compare(instr.op(0), instr.op(1))?,
                Memonic::Test => self.do_test(instr.op(0), instr.op(1))?,
                Memonic::Jump => {
                    let offset = self.do_jump(instr.op(0))?;
                    if offset != 0 {
                        return self.jump(offset);
                    }
                }
                Memonic::Jcc(ref cc) => {
                    let offset = self.do_jcc(cc, instr.op(0))?;
                    if offset != 0 {
                        return self.jump(offset);
                    }
                }
                Memonic::Call => {
                    let offset = self.do_call(instr.op(0))?;
                    if offset != 0 {
                        return self.jump(offset);
                    }
                }
                Memonic::Return => {
                    let absolute = self.do_return()?;
                    // If returning to a trampoline, do actual return with data
                    // about what we would be returning from.
                    println!("checking {:08X} against {:?}", absolute, self.trampolines);
                    if self.trampolines.contains_key(&absolute) {
                        let (ref name, ref arg_count) = self.trampolines[&absolute];
                        let mut args = self.stack[self.stack.len() - *arg_count..].to_owned();
                        args.reverse();
                        return Ok(ExitInfo::Trampoline(name.to_owned(), args));
                    }
                    return self.interpret(absolute);
                }
                _ => bail!("not implemented: {}", instr),
            }
        }
        return Ok(ExitInfo::OutOfInstructions);
    }

    fn jump(&mut self, offset: i32) -> Result<ExitInfo, Error> {
        let next_ip = if offset >= 0 {
            self.eip() + offset as u32
        } else {
            self.eip() - (-offset as u32)
        };
        return self.interpret(next_ip);
    }

    fn eip(&self) -> u32 {
        self.registers[Reg::EIP.to_offset()]
    }

    fn eip_mut(&mut self) -> &mut u32 {
        &mut self.registers[Reg::EIP.to_offset()]
    }

    fn esp(&self) -> u32 {
        self.registers[Reg::ESP.to_offset()]
    }

    fn esp_mut(&mut self) -> &mut u32 {
        &mut self.registers[Reg::ESP.to_offset()]
    }

    fn ecx(&self) -> u32 {
        self.registers[Reg::ECX.to_offset()]
    }

    fn ecx_mut(&mut self) -> &mut u32 {
        &mut self.registers[Reg::ECX.to_offset()]
    }

    fn esi_mut(&mut self) -> &mut u32 {
        &mut self.registers[Reg::ESI.to_offset()]
    }

    fn edi_mut(&mut self) -> &mut u32 {
        &mut self.registers[Reg::EDI.to_offset()]
    }

    fn do_pushall(&mut self) -> Result<(), Error> {
        let tmp = self.esp();
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

    fn do_popall(&mut self) -> Result<(), Error> {
        self.do_pop(&Operand::Register(Reg::EDI))?;
        self.do_pop(&Operand::Register(Reg::ESI))?;
        self.do_pop(&Operand::Register(Reg::EBP))?;
        self.stack.pop().unwrap(); // don't pop into esp; do it manually so the stack stays correct.
        *self.esp_mut() += 4;
        self.do_pop(&Operand::Register(Reg::EBX))?;
        self.do_pop(&Operand::Register(Reg::EDX))?;
        self.do_pop(&Operand::Register(Reg::ECX))?;
        self.do_pop(&Operand::Register(Reg::EAX))?;
        return Ok(());
    }

    fn do_push(&mut self, op: &Operand) -> Result<(), Error> {
        let v = self.get(op)?;
        self.stack.push(v);
        *self.esp_mut() -= 4;
        println!("        push {:08X}: sp at {:08X}", v, self.esp());
        return Ok(());
    }

    fn do_pop(&mut self, op: &Operand) -> Result<(), Error> {
        ensure!(self.stack.len() > 0, "pop with empty stack");
        let v = self.stack.pop().unwrap();
        self.put(op, v)?;
        *self.esp_mut() += 4;
        println!("        pop: sp at {:08X}", self.esp());
        return Ok(());
    }

    fn do_move(&mut self, op1: &Operand, op2: &Operand) -> Result<(), Error> {
        let v = self.get(op2)?;
        self.put(op1, v)
    }

    fn do_move_str(&mut self, op1: &Operand, op2: &Operand) -> Result<(), Error> {
        while self.ecx() != 0 {
            let v = self.get(op2)?;
            self.put(op1, v)?;
            *self.ecx_mut() -= 1;
            *self.esi_mut() += 1;
            *self.edi_mut() += 1;
        }
        return Ok(());
    }

    fn do_dec(&mut self, op: &Operand) -> Result<(), Error> {
        let v = self.get(op)? - 1;
        // FIXME: set flags
        return self.put(op, v);
    }

    fn do_inc(&mut self, op: &Operand) -> Result<(), Error> {
        let v = self.get(op)? + 1;
        // FIXME: set flags
        return self.put(op, v);
    }

    fn do_neg(&mut self, op: &Operand) -> Result<(), Error> {
        let v = -(self.get(op)? as i32);
        // FIXME: set flags
        return self.put(op, v as u32);
    }

    fn do_add(&mut self, op1: &Operand, op2: &Operand) -> Result<(), Error> {
        let a = self.get(op1)?;
        let b = self.get(op2)?;
        return self.put(op1, a + b);
    }

    fn do_adc(&mut self, op1: &Operand, op2: &Operand) -> Result<(), Error> {
        let a = self.get(op1)?;
        let b = self.get(op2)?;
        let carry = if self.cf { 1 } else { 0 };
        return self.put(op1, a + b + carry);
    }

    fn do_sub(&mut self, op1: &Operand, op2: &Operand) -> Result<(), Error> {
        let a = self.get(op1)? as i32;
        let b = self.get(op2)? as i32;
        return self.put(op1, (a - b) as u32);
    }

    fn do_imul(&mut self, op_dx: &Operand, op_ax: &Operand, op3: &Operand) -> Result<(), Error> {
        // dx:ax = ax * reg
        let a = self.get(op_ax)? as i32 as i64;
        let b = self.get(op3)? as i32 as i64;
        let v = a * b;
        let va = (v & 0xFFFFFFFF) as u32;
        let vd = (v >> 32) as u32;
        self.cf = vd != 0;
        self.of = self.cf;
        self.put(op_ax, va)?;
        return self.put(op_dx, vd);
    }

    fn do_imul2(&mut self, op1: &Operand, op2: &Operand) -> Result<(), Error> {
        // IMUL r16,r/m16 	word register = word register * r/m word.
        let a = self.get(op1)? as i32 as i64;
        let b = self.get(op2)? as i32 as i64;
        let v = a * b;
        self.cf = v & 0xFFFFFFFF == v;
        self.of = self.cf;
        return Ok(());
    }

    fn do_idiv(&mut self, op_dx: &Operand, op_ax: &Operand, op3: &Operand) -> Result<(), Error> {
        // ax, dx = dx:ax / cx, dx:ax % cx
        let dx = self.get(op_dx)? as i32 as i64 as u64; // make sure to sign-extend
        let ax = self.get(op_ax)? as i32 as i64 as u64;
        let cx = self.get(op3)? as i32 as i64;
        let tmp = ((dx << 32) | ax) as i64;
        let q = (tmp / cx) as i32 as u32;
        let r = (tmp % cx) as i32 as u32;
        self.put(op_ax, q)?;
        return self.put(op_dx, r);
    }

    fn do_and(&mut self, op1: &Operand, op2: &Operand) -> Result<(), Error> {
        let a = self.get(op1)?;
        let b = self.get(op2)?;
        return self.put(op1, a & b);
    }

    fn do_or(&mut self, op1: &Operand, op2: &Operand) -> Result<(), Error> {
        let a = self.get(op1)?;
        let b = self.get(op2)?;
        return self.put(op1, a | b);
    }

    fn do_xor(&mut self, op1: &Operand, op2: &Operand) -> Result<(), Error> {
        let a = self.get(op1)?;
        let b = self.get(op2)?;
        return self.put(op1, a ^ b);
    }

    // rotate right including carry.
    fn do_rcr(&mut self, op1: &Operand, op2: &Operand) -> Result<(), Error> {
        let arg = self.get(op1)?;
        let cnt = self.get(op2)?;
        assert!(!self.cf, "carry flag set in rotate right");
        // FIXME: this needs to be size aware; put a func to get it on Operand.
        let v = arg.rotate_right(cnt);
        // FIXME: this needs to set CF
        return self.put(op1, v);
    }

    fn msb(v: u32, size: u8) -> bool {
        match size {
            1 => (v >> 7) & 1 == 1,
            2 => (v >> 15) & 1 == 1,
            4 => (v >> 31) & 1 == 1,
            _ => panic!("invalid size"),
        }
    }

    fn do_shl(&mut self, op1: &Operand, op2: &Operand) -> Result<(), Error> {
        let count = self.get(op2)? & 0x1F;
        let mut arg = self.get(op1)?;
        let mut cnt = count;
        while cnt != 0 {
            self.cf = Self::msb(arg, op1.size());
            arg <<= 1;
            cnt -= 1;
        }
        if count == 1 {
            self.of = Self::msb(arg, op1.size()) ^ self.cf;
        }
        return self.put(op1, arg);
    }

    fn do_shr(&mut self, op1: &Operand, op2: &Operand) -> Result<(), Error> {
        let tmp_dest = self.get(op1)?;
        let count = self.get(op2)? & 0x1F;
        let mut arg = tmp_dest;
        let mut cnt = count;
        while cnt != 0 {
            self.cf = (arg & 1) == 1;
            arg /= 2;
            cnt -= 1;
        }
        if count == 1 {
            self.of = Self::msb(tmp_dest, op1.size())
        }
        return self.put(op1, arg);
    }

    fn do_sar(&mut self, op1: &Operand, op2: &Operand) -> Result<(), Error> {
        let mut arg = self.get(op1)? as i32;
        let mut cnt = self.get(op2)? & 0x1F;
        while cnt != 0 {
            self.cf = (arg & 1) == 1;
            arg /= 2;
            cnt -= 1;
        }
        self.of = false;
        return self.put(op1, arg as u32);
    }

    fn do_compare(&mut self, op1: &Operand, op2: &Operand) -> Result<(), Error> {
        let a = self.get(op1)?;
        let b = self.get(op2)?;
        self.cf = (a as i32) < (b as i32);
        let rv = a.checked_sub(b);
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
            "    compare {:08X}, {:08X} -> {:04X} => cf:{}, of:{}, zf:{}, sf:{}",
            a, b, v, self.cf, self.of, self.zf, self.sf
        );
        return Ok(());
    }

    fn do_test(&mut self, op1: &Operand, op2: &Operand) -> Result<(), Error> {
        let a = self.get(op1)?;
        let b = self.get(op2)?;
        let tmp = a & b;
        self.sf = (tmp >> 31) & 1 != 0;
        self.zf = tmp == 0;
        self.cf = false;
        self.of = false;
        return Ok(());
    }

    fn do_jcc(&self, cc: &ConditionCode, op: &Operand) -> Result<i32, Error> {
        let should_jump = match cc {
            ConditionCode::Unary(ref cc1) => self.check_cc1(cc1),
            ConditionCode::Binary(ref cc2) => match cc2 {
                ConditionCode2::And(cc1a, cc1b) => self.check_cc1(cc1a) && self.check_cc1(cc1b),
                ConditionCode2::Or(cc1a, cc1b) => self.check_cc1(cc1a) || self.check_cc1(cc1b),
            },
        };
        if !should_jump {
            return Ok(0);
        }
        let offset = self.get(op)? as i32;
        println!("    jcc -> {:04X}", offset);
        return Ok(offset);
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

    fn do_jump(&self, op: &Operand) -> Result<i32, Error> {
        let offset = self.get(op)? as i32;
        println!("    jump -> {:04X}", offset);
        return Ok(offset);
    }

    fn do_call(&mut self, op: &Operand) -> Result<i32, Error> {
        let offset = self.get(op)? as i32;
        let ip = self.eip();
        self.stack.push(ip);
        *self.esp_mut() -= 4;
        println!("    call -> {:04X}", offset);
        return Ok(offset);
    }

    fn do_return(&mut self) -> Result<u32, Error> {
        ensure!(self.stack.len() > 0, "return with empty stack");
        let absolute = self.stack.pop().unwrap();
        *self.esp_mut() += 4;
        println!("    ret -> {:04X}", absolute);
        return Ok(absolute);
    }

    fn do_lea(&mut self, op1: &Operand, op2: &Operand) -> Result<(), Error> {
        let v = self.lea(Self::op_as_mem(op2)?)?;
        return self.put(op1, v);
    }

    // Some instructions force a certain operand type.
    fn op_as_mem(op: &Operand) -> Result<&MemRef, Error> {
        match op {
            Operand::Memory(mem) => return Ok(mem),
            _ => bail!("op_as_mem on non memory operand"),
        }
    }

    // [base + index*scale + disp]
    fn lea(&self, mem: &MemRef) -> Result<u32, Error> {
        if let Some(ref seg_reg) = mem.segment {
            ensure!(
                self.registers[seg_reg.to_offset()] == 0,
                "non-zero segment register in mem ref"
            );
        }
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
        return Ok(addr);
    }

    fn get(&self, op: &Operand) -> Result<u32, Error> {
        Ok(match op {
            Operand::Imm32(u) => *u,
            Operand::Imm32s(i) => *i as u32,
            Operand::Register(r) => {
                let base = self.registers[r.to_offset()];
                if r.is_reg16() {
                    println!("    read_reg {} -> {:04X}", r, base & 0xFFFF);
                    base & 0xFFFF
                } else if r.is_low8() {
                    println!("    read_reg {} -> {:02X}", r, base & 0xFF);
                    base & 0xFF
                } else if r.is_high8() {
                    println!("    read_reg {} -> {:02X}", r, (base >> 8) & 0xFF);
                    (base >> 8) & 0xFF
                } else {
                    println!("    read_reg {} -> {:08X}", r, base);
                    base
                }
            }
            Operand::Memory(mem) => {
                let addr = self.lea(mem)?;
                self.mem_lookup(addr, mem.size)?
            }
        })
    }

    fn mem_lookup(&self, addr: u32, size: u8) -> Result<u32, Error> {
        if self.ports_r.contains_key(&addr) {
            let v = self.ports_r[&addr]();
            println!("    read_port {:08X} -> {:08X}", addr, v);
            return Ok(v);
        }
        for map in self.memmap_w.iter() {
            if addr >= map.start && addr < map.end {
                let v = self.mem_peek((addr - map.start) as usize, &map.mem, size)?;
                println!("    read_rw  {} @ {:08X} -> {:08X}", size, addr, v);
                return Ok(v);
            }
        }
        for map in self.memmap_r.iter() {
            if addr >= map.start && addr < map.end {
                let v = self.mem_peek((addr - map.start) as usize, &map.mem, size)?;
                println!("    read_ro  {} @ {:08X} -> {:08X}", size, addr, v);
                return Ok(v);
            }
        }
        bail!(
            "no memory or port for address: {:08X} at ip {:08X}",
            addr,
            self.eip()
        );
    }

    fn mem_peek(&self, rel: usize, v: &'a Vec<u8>, size: u8) -> Result<u32, Error> {
        Ok(match size {
            1 => v[rel] as u32,
            2 => {
                let vp: &[u16] = unsafe { mem::transmute(&v[rel..rel + 2]) };
                vp[0] as u32
            }
            4 => {
                let vp: &[u32] = unsafe { mem::transmute(&v[rel..rel + 4]) };
                vp[0]
            }
            _ => bail!("don't know how to handle read size {}", size),
        })
    }

    fn put(&mut self, op: &Operand, v: u32) -> Result<(), Error> {
        match op {
            Operand::Register(r) => {
                if r.is_reg16() {
                    println!("    write_reg {} <- {:04X}", r, v & 0xFFFF);
                    self.registers[r.to_offset()] &= !0xFFFF;
                    self.registers[r.to_offset()] |= v & 0xFFFF;
                } else if r.is_low8() {
                    println!("    write_reg {} <- {:02X}", r, v & 0xFF);
                    self.registers[r.to_offset()] &= !0xFF;
                    self.registers[r.to_offset()] |= v & 0xFF;
                } else if r.is_high8() {
                    println!("    write_reg {} <- {:04X}", r, (v & 0xFF) << 8);
                    self.registers[r.to_offset()] &= !0xFF00;
                    self.registers[r.to_offset()] |= (v & 0xFF) << 8;
                } else {
                    println!("    write_reg {} <- {:08X}", r, v);
                    self.registers[r.to_offset()] = v;
                }
            }
            Operand::Memory(mem) => {
                let addr = self.lea(mem)?;
                self.mem_write(addr, v, mem.size)?
            }
            _ => bail!("attempted put to {}", op),
        }
        return Ok(());
    }

    fn mem_write(&mut self, addr: u32, v: u32, size: u8) -> Result<(), Error> {
        if self.ports_w.contains_key(&addr) {
            println!("    write_port {:08X} <- {:08X}", addr, v);
            self.ports_w[&addr](v);
            return Ok(());
        }
        for map in self.memmap_w.iter_mut() {
            if addr >= map.start && addr < map.end {
                let rel = (addr - map.start) as usize;
                match size {
                    1 => map.mem[rel] = v as u8,
                    2 => {
                        let vp: &mut [u16] = unsafe { mem::transmute(&mut map.mem[rel..rel + 2]) };
                        vp[0] = v as u16;
                    }
                    4 => {
                        let vp: &mut [u32] = unsafe { mem::transmute(&mut map.mem[rel..rel + 4]) };
                        vp[0] = v;
                    }
                    _ => bail!("don't know how to handle write size {}", size),
                }
                println!("    write_rw {}@ {:08X} <- {:08X}", size, addr, v);
                return Ok(());
            }
        }
        bail!("no writable memory for address: {:08X}", addr);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::prelude::*;

    #[test]
    fn it_works() {
        // //let path = "./test_data/sh/EXP.SH";
        // let path = "./test_data/x86/exp.asm-1035.x86";
        // let mut fp = fs::File::open(path).unwrap();
        // let mut data = Vec::new();
        // fp.read_to_end(&mut data).unwrap();

        // let bc = ByteCode::disassemble(&data, true).unwrap();

        // let mut interp = Interpreter::new();
        // //interp.map_memory(0xAA000000, data);
        // interp.interpret(&bc).unwrap();
    }
}
