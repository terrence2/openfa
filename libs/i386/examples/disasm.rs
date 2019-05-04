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
use failure::Fallible;
use i386::{ByteCode, DisassemblyError};
use std::fs;
use std::io::prelude::*;

fn main() -> Fallible<()> {
    let name = "test_data/i386/x86/ast.asm-2390.x86";
    let mut fp = fs::File::open(name)?;
    let mut data = Vec::new();
    fp.read_to_end(&mut data)?;

    let bc = ByteCode::disassemble_until(0, &data, |_| false);
    if let Err(ref e) = bc {
        if !DisassemblyError::maybe_show(e, &data) {
            println!("ERROR: {}", e);
        }
    }
    let bc = bc?;
    println!("i386 Bytecode:\n{}", bc);

    Ok(())
}
