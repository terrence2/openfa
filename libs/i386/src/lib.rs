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
use std::collections::HashMap;

pub struct Machine {
    registers: [u32; 8],
    memory_map: HashMap<usize, usize>
}

impl Machine {
    pub fn new() -> Self {
        Machine {
            registers: [0, 0, 0, 0, 0, 0, 0, 0],
            memory_map: HashMap::new(),
        }
    }

    pub fn compile_bytecode(code: &[u8]) {
        let mut instrs = Vec::new();
    }
}

pub struct FakeX86 {
}

impl FakeX86 {
    fn virtual_interpret(code: &[u8]) {
        let mut state = FakeX86State::clean();
        let mut ip = 0;
        loop {
            match code[ip] {
                // PREFIX
                0x66 => {
                    state.prefix_operand_override = true;
                    ip += 1;
                },
                // CMPW
                0x83 => {
                    // Check that this is set on all actual users.
                    assert!(state.prefix_operand_override);
                    let mods = code[ip + 1];
                    let a1_ptr: &[u32] = unsafe { mem::transmute(&code[ip + 2..ip + 6]) };
                    let a1: u32 = a1_ptr[0];
                    let a2: u8 = code[ip + 7];
                    ip += 2 + 4 + 1;
                }
                // JNE
                0x75 => {

                }
                _ => {
                    panic!("Unknown x86 opcode")
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
