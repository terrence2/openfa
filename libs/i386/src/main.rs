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
extern crate clap;
extern crate i386;
extern crate reverse;

use clap::{Arg, App};
use i386::{DisassemblyError, Instr};
use reverse::bs2s;
use std::fs;
use std::io::prelude::*;

fn main() {
    let matches = App::new("OpenFA disassembler tool")
        .version("0.0.1")
        .author("Terrence Cole <terrence.d.cole@gmail.com>")
        .about("Disassemble a fragment of i386 code.")
        .arg(Arg::with_name("INPUT")
            .help("The files to disassemble")
            .multiple(true)
            .required(true))
        .get_matches();

    for name in matches.values_of("INPUT").unwrap() {
        println!("Reading: {}", name);
        let mut fp = fs::File::open(name).unwrap();
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
        }
        let bc = bc.unwrap();
        println!("OUT: {:?}", bc);
    }
}
