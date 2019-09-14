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
#![allow(clippy::new_without_default, clippy::transmute_ptr_to_ptr)]

use crate::{
    disassembler::{ByteCode, MemRef, Memonic, Operand, Reg},
    lut::{ConditionCode, ConditionCode1, ConditionCode2, FlagKind},
};
use failure::{bail, ensure, Fallible};
use log::trace;
use std::{cell::RefCell, collections::HashMap, mem, rc::Rc};

#[derive(Debug)]
pub enum ExitInfo {
    OutOfInstructions,
    Trampoline(String, Vec<u32>),
}

impl ExitInfo {
    pub fn ok_trampoline(self) -> Fallible<(String, Vec<u32>)> {
        Ok(match self {
            ExitInfo::Trampoline(name, args) => (name, args),
            _ => bail!("exit info is not a trampoline"),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum MapProtection {
    // Read,
    Write,
}

#[derive(Clone, Debug, Eq, Ord, PartialOrd, PartialEq)]
struct MemMap {
    mem: Vec<u8>,
    protection: MapProtection,
    start: u32,
}

impl MemMap {
    fn new(start: u32 /*, end: u32*/, mem: Vec<u8>, protection: MapProtection) -> Self {
        //assert!(start < end);
        Self {
            start,
            mem,
            protection,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Interpreter {
    registers: Vec<u32>,
    cf: bool,
    of: bool,
    zf: bool,
    sf: bool,
    stack: Vec<u32>,
    mem_maps: Vec<MemMap>,
    value_maps: HashMap<u32, u32>,
    bytecode: Vec<Rc<RefCell<ByteCode>>>,
    trampolines: HashMap<u32, (String, usize)>,
}

impl Interpreter {
    pub fn new() -> Self {
        let mut registers = Vec::new();
        registers.resize(Reg::num_registers(), 0);
        registers[Reg::ESP.to_offset()] = 0xFFFF_FFFF;
        Self {
            registers,
            cf: false,
            of: false,
            zf: false,
            sf: false,
            stack: Vec::new(),
            mem_maps: Vec::new(),
            bytecode: Vec::new(),
            value_maps: HashMap::new(),
            trampolines: HashMap::new(),
        }
    }

    pub fn push_stack_value(&mut self, value: u32) {
        self.registers[Reg::ESP.to_offset()] -= 4;
        self.stack.push(value);
    }

    pub fn set_register_value(&mut self, reg: Reg, value: u32) {
        self.registers[reg.to_offset()] = value;
    }

    pub fn add_trampoline(&mut self, addr: u32, name: &str, arg_count: usize) {
        self.trampolines.insert(addr, (name.to_owned(), arg_count));
    }

    pub fn map_value(&mut self, addr: u32, value: u32) {
        self.value_maps.insert(addr, value);
    }

    pub fn unmap_value(&mut self, addr: u32) -> u32 {
        self.value_maps.remove(&addr).unwrap()
    }

    pub fn map_writable(&mut self, start: u32, data: Vec<u8>) -> Fallible<()> {
        ensure!(
            data.len() < u32::max_value() as usize,
            "readonly data segment too large"
        );
        let map = MemMap::new(start, data, MapProtection::Write);
        self.mem_maps.push(map);
        self.mem_maps.sort();
        Ok(())
    }

    pub fn unmap_writable(&mut self, start: u32) -> Fallible<Vec<u8>> {
        if let Ok(offset) = self.mem_maps.binary_search_by(|v| v.start.cmp(&start)) {
            let map = self.mem_maps.remove(offset);
            return Ok(map.mem);
        }
        bail!(
            "the address {:08X} is not mapped to a writable block",
            start
        )
    }

    pub fn add_code(&mut self, bc: Rc<RefCell<ByteCode>>) {
        self.bytecode.push(bc);
    }

    pub fn clear_code(&mut self) {
        self.bytecode.clear();
    }

    fn find_instr(&self) -> Fallible<(Rc<RefCell<ByteCode>>, usize)> {
        trace!("searching for instr at ip: {:08X}", self.eip());
        for bc_ref in self.bytecode.iter() {
            let bc = bc_ref.borrow();
            if self.eip() >= bc.start_addr && self.eip() < bc.start_addr + bc.size {
                trace!("in bc at {:08X}", bc.start_addr);
                let mut pos = bc.start_addr;
                for (offset, instr) in bc.instrs.iter().enumerate() {
                    trace!("checking {}: {:08X} of {:08X}", offset, pos, self.eip());
                    if pos == self.eip() {
                        return Ok((bc_ref.clone(), offset));
                    }
                    pos += instr.size() as u32;
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
        )
    }

    pub fn interpret(&mut self, at: u32) -> Fallible<ExitInfo> {
        *self.eip_mut() = at;
        let (bc_ref, mut offset) = self.find_instr()?;
        let bc = bc_ref.borrow();
        while offset < bc.instrs.len() {
            let instr = &bc.instrs[offset];
            trace!("{:3}:{:04X}: {}", offset, self.eip(), instr);
            offset += 1;
            *self.eip_mut() += instr.size() as u32;
            match instr.memonic {
                Memonic::PushAll => self.do_pushall()?,
                Memonic::PopAll => self.do_popall()?,
                Memonic::Push => self.do_push(instr.op(0))?,
                Memonic::Pop => self.do_pop(instr.op(0))?,
                Memonic::Move => self.do_move(instr.op(0), instr.op(1))?,
                Memonic::MoveStr => self.do_move_str(instr.op(0), instr.op(1))?,
                Memonic::MoveZX => self.do_move_zx(instr.op(0), instr.op(1))?,
                Memonic::Dec => self.do_dec(instr.op(0))?,
                Memonic::Inc => self.do_inc(instr.op(0))?,
                Memonic::Neg => self.do_neg(instr.op(0))?,
                Memonic::Add => self.do_add(instr.op(0), instr.op(1))?,
                Memonic::Adc => self.do_adc(instr.op(0), instr.op(1))?,
                Memonic::Sub => self.do_sub(instr.op(0), instr.op(1))?,
                Memonic::IDiv => self.do_idiv(instr.op(0), instr.op(1), instr.op(2))?,
                Memonic::IMul3 => self.do_imul3(instr.op(0), instr.op(1), instr.op(2))?,
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
                Memonic::Jcc(cc) => {
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
                    trace!("checking {:08X} against {:?}", absolute, self.trampolines);
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
        Ok(ExitInfo::OutOfInstructions)
    }

    fn jump(&mut self, offset: i32) -> Fallible<ExitInfo> {
        let next_ip = if offset >= 0 {
            self.eip() + offset as u32
        } else {
            self.eip() - (-offset as u32)
        };
        self.interpret(next_ip)
    }

    pub fn eip(&self) -> u32 {
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

    fn do_pushall(&mut self) -> Fallible<()> {
        let tmp = self.esp();
        self.do_push(&Operand::Register(Reg::EAX))?;
        self.do_push(&Operand::Register(Reg::ECX))?;
        self.do_push(&Operand::Register(Reg::EDX))?;
        self.do_push(&Operand::Register(Reg::EBX))?;
        self.do_push(&Operand::Imm32(tmp))?;
        self.do_push(&Operand::Register(Reg::EBP))?;
        self.do_push(&Operand::Register(Reg::ESI))?;
        self.do_push(&Operand::Register(Reg::EDI))?;
        Ok(())
    }

    fn do_popall(&mut self) -> Fallible<()> {
        self.do_pop(&Operand::Register(Reg::EDI))?;
        self.do_pop(&Operand::Register(Reg::ESI))?;
        self.do_pop(&Operand::Register(Reg::EBP))?;
        self.stack.pop().unwrap(); // don't pop into esp; do it manually so the stack stays correct.
        *self.esp_mut() += 4;
        self.do_pop(&Operand::Register(Reg::EBX))?;
        self.do_pop(&Operand::Register(Reg::EDX))?;
        self.do_pop(&Operand::Register(Reg::ECX))?;
        self.do_pop(&Operand::Register(Reg::EAX))?;
        Ok(())
    }

    fn do_push(&mut self, op: &Operand) -> Fallible<()> {
        let v = self.get(op)?;
        self.stack.push(v);
        *self.esp_mut() -= 4;
        trace!("        push {:08X}: sp at {:08X}", v, self.esp());
        Ok(())
    }

    fn do_pop(&mut self, op: &Operand) -> Fallible<()> {
        ensure!(!self.stack.is_empty(), "pop with empty stack");
        let v = self.stack.pop().unwrap();
        self.put(op, v)?;
        *self.esp_mut() += 4;
        trace!("        pop: sp at {:08X}", self.esp());
        Ok(())
    }

    fn do_move(&mut self, op1: &Operand, op2: &Operand) -> Fallible<()> {
        let v = self.get(op2)?;
        self.put(op1, v)
    }

    fn do_move_str(&mut self, op1: &Operand, op2: &Operand) -> Fallible<()> {
        while self.ecx() != 0 {
            let v = self.get(op2)?;
            self.put(op1, v)?;
            *self.ecx_mut() -= 1;
            *self.esi_mut() += 1;
            *self.edi_mut() += 1;
        }
        Ok(())
    }

    fn do_move_zx(&mut self, op1: &Operand, op2: &Operand) -> Fallible<()> {
        let v = self.get(op2)?;
        self.put(op1, v)
    }

    fn do_dec(&mut self, op: &Operand) -> Fallible<()> {
        let v = self.get(op)? - 1;
        // FIXME: set flags
        self.put(op, v)
    }

    fn do_inc(&mut self, op: &Operand) -> Fallible<()> {
        let v = self.get(op)? + 1;
        // FIXME: set flags
        self.put(op, v)
    }

    fn do_neg(&mut self, op: &Operand) -> Fallible<()> {
        let v = -(self.get(op)? as i32);
        // FIXME: set flags
        self.put(op, v as u32)
    }

    fn do_add(&mut self, op1: &Operand, op2: &Operand) -> Fallible<()> {
        let a = self.get(op1)?;
        let b = self.get(op2)?;
        self.put(op1, a + b)
    }

    fn do_adc(&mut self, op1: &Operand, op2: &Operand) -> Fallible<()> {
        let a = self.get(op1)?;
        let b = self.get(op2)?;
        let carry = if self.cf { 1 } else { 0 };
        self.put(op1, a + b + carry)
    }

    fn do_sub(&mut self, op1: &Operand, op2: &Operand) -> Fallible<()> {
        let a = self.get(op1)? as i32;
        let b = self.get(op2)? as i32;
        self.put(op1, (a - b) as u32)
    }

    fn do_imul3(&mut self, dst: &Operand, src1: &Operand, src2: &Operand) -> Fallible<()> {
        // dx:ax = ax * reg
        let a = i64::from(self.get(src1)? as i32);
        let b = i64::from(self.get(src2)? as i32);
        let v = a * b;
        self.cf = v & 0xFFFF_FFFF == v;
        self.of = self.cf;
        self.put(dst, (v & 0xFFFF_FFFF) as u32)
    }

    fn do_imul2(&mut self, op1: &Operand, op2: &Operand) -> Fallible<()> {
        // IMUL r16,r/m16 	word register = word register * r/m word.
        let a = i64::from(self.get(op1)? as i32);
        let b = i64::from(self.get(op2)? as i32);
        let v = a * b;
        self.cf = v & 0xFFFF_FFFF == v;
        self.of = self.cf;
        self.put(op1, (v & 0xFFFF_FFFF) as u32)
    }

    fn do_idiv(&mut self, op_dx: &Operand, op_ax: &Operand, op3: &Operand) -> Fallible<()> {
        // ax, dx = dx:ax / cx, dx:ax % cx
        let dx = i64::from(self.get(op_dx)? as i32) as u64; // make sure to sign-extend
        let ax = i64::from(self.get(op_ax)? as i32) as u64;
        let cx = i64::from(self.get(op3)? as i32);
        let tmp = ((dx << 32) | ax) as i64;
        let q = (tmp / cx) as i32 as u32;
        let r = (tmp % cx) as i32 as u32;
        self.put(op_ax, q)?;
        self.put(op_dx, r)
    }

    fn do_and(&mut self, op1: &Operand, op2: &Operand) -> Fallible<()> {
        let a = self.get(op1)?;
        let b = self.get(op2)?;
        let v = a & b;
        self.zf = v == 0;
        self.put(op1, v)
    }

    fn do_or(&mut self, op1: &Operand, op2: &Operand) -> Fallible<()> {
        let a = self.get(op1)?;
        let b = self.get(op2)?;
        let v = a | b;
        self.zf = v == 0;
        self.put(op1, v)
    }

    fn do_xor(&mut self, op1: &Operand, op2: &Operand) -> Fallible<()> {
        let a = self.get(op1)?;
        let b = self.get(op2)?;
        self.put(op1, a ^ b)
    }

    // rotate right including carry.
    fn do_rcr(&mut self, op1: &Operand, op2: &Operand) -> Fallible<()> {
        let arg = self.get(op1)?;
        let cnt = self.get(op2)?;
        assert!(!self.cf, "carry flag set in rotate right");
        // FIXME: this needs to be size aware; put a func to get it on Operand.
        let v = arg.rotate_right(cnt);
        // FIXME: this needs to set CF
        self.put(op1, v)
    }

    fn msb(v: u32, size: u8) -> bool {
        match size {
            1 => (v >> 7) & 1 == 1,
            2 => (v >> 15) & 1 == 1,
            4 => (v >> 31) & 1 == 1,
            _ => panic!("invalid size"),
        }
    }

    fn do_shl(&mut self, op1: &Operand, op2: &Operand) -> Fallible<()> {
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
        self.put(op1, arg)
    }

    fn do_shr(&mut self, op1: &Operand, op2: &Operand) -> Fallible<()> {
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
        self.put(op1, arg)
    }

    fn do_sar(&mut self, op1: &Operand, op2: &Operand) -> Fallible<()> {
        let mut arg = self.get(op1)? as i32;
        let mut cnt = self.get(op2)? & 0x1F;
        while cnt != 0 {
            self.cf = (arg & 1) == 1;
            arg /= 2;
            cnt -= 1;
        }
        self.of = false;
        self.put(op1, arg as u32)
    }

    fn do_compare(&mut self, op1: &Operand, op2: &Operand) -> Fallible<()> {
        let a = self.get(op1)? as i32;
        let b = self.get(op2)? as i32;
        self.cf = a < b;
        let rv = a.checked_sub(b);
        let v = if rv.is_none() {
            self.of = true;
            a.wrapping_add(b)
        } else {
            self.of = false;
            rv.unwrap()
        };
        self.zf = v == 0;
        self.sf = (v as i32) < 0;
        trace!(
            "    compare {:08X}, {:08X} -> {:04X} => cf:{}, of:{}, zf:{}, sf:{}",
            a,
            b,
            v,
            self.cf,
            self.of,
            self.zf,
            self.sf
        );
        Ok(())
    }

    fn do_test(&mut self, op1: &Operand, op2: &Operand) -> Fallible<()> {
        let a = self.get(op1)?;
        let b = self.get(op2)?;
        let tmp = a & b;
        self.sf = (tmp >> 31) & 1 != 0;
        self.zf = tmp == 0;
        self.cf = false;
        self.of = false;
        Ok(())
    }

    fn do_jcc(&self, cc: ConditionCode, op: &Operand) -> Fallible<i32> {
        let should_jump = match cc {
            ConditionCode::Unary(cc1) => self.check_cc1(cc1),
            ConditionCode::Binary(cc2) => match cc2 {
                ConditionCode2::And(cc1a, cc1b) => self.check_cc1(cc1a) && self.check_cc1(cc1b),
                ConditionCode2::Or(cc1a, cc1b) => self.check_cc1(cc1a) || self.check_cc1(cc1b),
            },
        };
        if !should_jump {
            return Ok(0);
        }
        let offset = self.get(op)? as i32;
        trace!("    jcc -> {:04X}", offset);
        Ok(offset)
    }

    fn check_cc1(&self, cc1: ConditionCode1) -> bool {
        match cc1 {
            ConditionCode1::Check(flag, expect) => self.get_flag(flag) == expect,
            ConditionCode1::Eq(flag1, flag2) => self.get_flag(flag1) == self.get_flag(flag2),
            ConditionCode1::NotEq(flag1, flag2) => self.get_flag(flag1) != self.get_flag(flag2),
        }
    }

    fn get_flag(&self, flag: FlagKind) -> bool {
        match flag {
            FlagKind::CF => self.cf,
            FlagKind::OF => self.of,
            FlagKind::ZF => self.zf,
            FlagKind::SF => self.sf,
            FlagKind::PF => panic!("attempted read of parity flag"),
        }
    }

    fn do_jump(&self, op: &Operand) -> Fallible<i32> {
        let offset = self.get(op)? as i32;
        trace!("    jump -> {:04X}", offset);
        Ok(offset)
    }

    fn do_call(&mut self, op: &Operand) -> Fallible<i32> {
        let offset = self.get(op)? as i32;
        let ip = self.eip();
        self.stack.push(ip);
        *self.esp_mut() -= 4;
        trace!("    call -> {:04X}", offset);
        Ok(offset)
    }

    fn do_return(&mut self) -> Fallible<u32> {
        ensure!(!self.stack.is_empty(), "return with empty stack");
        let absolute = self.stack.pop().unwrap();
        *self.esp_mut() += 4;
        trace!("    ret -> {:04X}", absolute);
        Ok(absolute)
    }

    fn do_lea(&mut self, op1: &Operand, op2: &Operand) -> Fallible<()> {
        let v = self.lea(Self::op_as_mem(op2)?)?;
        self.put(op1, v)
    }

    // Some instructions force a certain operand type.
    fn op_as_mem(op: &Operand) -> Fallible<&MemRef> {
        match op {
            Operand::Memory(mem) => Ok(mem),
            _ => bail!("op_as_mem on non memory operand"),
        }
    }

    // [base + index*scale + disp]
    fn lea(&self, mem: &MemRef) -> Fallible<u32> {
        if let Some(ref seg_reg) = mem.segment {
            ensure!(
                self.registers[seg_reg.to_offset()] == 0,
                "non-zero segment register in mem ref"
            );
        }
        let index = if let Some(ref r) = mem.index {
            self.registers[r.to_offset()]
        } else {
            0
        } * u32::from(mem.scale);
        let base = if let Some(ref r) = mem.base {
            self.registers[r.to_offset()]
        } else {
            0
        };
        //let base = mem.base.map(|r| self.registers[r.to_offset()]).unwrap_or(0);
        //let base = mem.base.map_or(0, |ref r| self.registers[r.to_offset()]);
        let addr = if mem.displacement >= 0 {
            base + index + mem.displacement as u32
        } else if base > -mem.displacement as u32 {
            base + index - (-mem.displacement) as u32
        } else {
            (base as i32 + index as i32 + mem.displacement) as u32
        };
        Ok(addr)
    }

    fn get(&self, op: &Operand) -> Fallible<u32> {
        Ok(match op {
            Operand::Imm32(u) => *u,
            Operand::Imm32s(i) => *i as u32,
            Operand::Register(r) => {
                let base = self.registers[r.to_offset()];
                if r.is_reg16() {
                    trace!("    read_reg {} -> {:04X}", r, base & 0xFFFF);
                    base & 0xFFFF
                } else if r.is_low8() {
                    trace!("    read_reg {} -> {:02X}", r, base & 0xFF);
                    base & 0xFF
                } else if r.is_high8() {
                    trace!("    read_reg {} -> {:02X}", r, (base >> 8) & 0xFF);
                    (base >> 8) & 0xFF
                } else {
                    trace!("    read_reg {} -> {:08X}", r, base);
                    base
                }
            }
            Operand::Memory(mem) => {
                let addr = self.lea(mem)?;
                self.mem_lookup(addr, mem.size)?
            }
        })
    }

    fn mem_lookup(&self, addr: u32, size: u8) -> Fallible<u32> {
        if let Some(value) = self.value_maps.get(&addr) {
            trace!("    read_val  {:08X} -> {:08X}", addr, value);
            return Ok(*value);
        }
        for map in self.mem_maps.iter() {
            if addr >= map.start && ((addr - map.start) as usize) < map.mem.len() {
                let v = self.mem_peek((addr - map.start) as usize, &map.mem, size)?;
                trace!("    read_rw  {} @ {:08X} -> {:08X}", size, addr, v);
                return Ok(v);
            }
        }
        bail!(
            "no memory or port for address: {:08X} at ip {:08X}",
            addr,
            self.eip()
        )
    }

    fn mem_peek(&self, rel: usize, v: &[u8], size: u8) -> Fallible<u32> {
        Ok(match size {
            1 => u32::from(v[rel]),
            2 => {
                let vp: &[u16] = unsafe { mem::transmute(&v[rel..rel + 2]) };
                u32::from(vp[0])
            }
            4 => {
                let vp: &[u32] = unsafe { mem::transmute(&v[rel..rel + 4]) };
                vp[0]
            }
            _ => bail!("don't know how to handle read size {}", size),
        })
    }

    fn put(&mut self, op: &Operand, v: u32) -> Fallible<()> {
        match op {
            Operand::Register(r) => {
                if r.is_reg16() {
                    trace!("    write_reg {} <- {:04X}", r, v & 0xFFFF);
                    self.registers[r.to_offset()] &= !0xFFFF;
                    self.registers[r.to_offset()] |= v & 0xFFFF;
                } else if r.is_low8() {
                    trace!("    write_reg {} <- {:02X}", r, v & 0xFF);
                    self.registers[r.to_offset()] &= !0xFF;
                    self.registers[r.to_offset()] |= v & 0xFF;
                } else if r.is_high8() {
                    trace!("    write_reg {} <- {:04X}", r, (v & 0xFF) << 8);
                    self.registers[r.to_offset()] &= !0xFF00;
                    self.registers[r.to_offset()] |= (v & 0xFF) << 8;
                } else {
                    trace!("    write_reg {} <- {:08X}", r, v);
                    self.registers[r.to_offset()] = v;
                }
            }
            Operand::Memory(mem) => {
                let addr = self.lea(mem)?;
                self.mem_write(addr, v, mem.size)?
            }
            _ => bail!("attempted put to {}", op),
        }
        Ok(())
    }

    fn mem_write(&mut self, addr: u32, v: u32, size: u8) -> Fallible<()> {
        if let Some(value) = self.value_maps.get_mut(&addr) {
            trace!("    write_rw {}@ {:08X} <- {:08X}", size, addr, v);
            match size {
                1 => *value = (*value & 0xFFFF_FF00) | (v & 0xFF),
                2 => *value = (*value & 0xFFFF_0000) | (v & 0xFFFF),
                4 => *value = v,
                _ => bail!("don't know how to handle write size {}", size),
            }
        }
        for map in self.mem_maps.iter_mut() {
            if addr >= map.start && ((addr - map.start) as usize) < map.mem.len() {
                ensure!(
                    map.protection == MapProtection::Write,
                    "write to read-only memory"
                );
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
                trace!("    write_rw {}@ {:08X} <- {:08X}", size, addr, v);
                return Ok(());
            }
        }
        bail!("no writable memory for address: {:08X}", addr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // use std::fs;
    // use std::io::prelude::*;

    #[test]
    fn it_works() -> Fallible<()> {
        // TODO: this approach would basically needs the whole shape loader. See
        // if we can craft a more targetted test in nasm.

        //let path = "./test_data/x86/exp.asm-1035.x86";
        // let path = "./test_data/x86/a10.asm-10399.x86";
        // let mut fp = fs::File::open(path)?;
        // let mut data = Vec::new();
        // fp.read_to_end(&mut data)?;

        // let bc = ByteCode::disassemble(&data, true)?;

        // let memory = [0u8; 4096].to_vec();
        // let mut interp = Interpreter::new();
        // interp.map_writable(0x00005000, memory);
        // interp.add_code(&bc);
        // interp.interpret(0)?;

        Ok(())
    }
}
