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
extern crate failure;
extern crate i386;

use clap::{App, Arg};
use failure::{err_msg, Fallible};
use i386::{ByteCode, DisassemblyError};
use std::fs;
use std::io::prelude::*;

fn main() -> Fallible<()> {
    let matches = App::new("OpenFA disassembler tool")
        .version("0.0.1")
        .author("Terrence Cole <terrence.d.cole@gmail.com>")
        .about("Disassemble a fragment of i386 code.")
        .arg(
            Arg::with_name("INPUT")
                .help("The files to disassemble")
                .multiple(true)
                .required(true),
        )
        .get_matches();

    for name in matches
        .values_of("INPUT")
        .ok_or_else(|| err_msg("no input"))?
    {
        println!("Reading: {}", name);
        let mut fp = fs::File::open(name)?;
        let mut data = Vec::new();
        fp.read_to_end(&mut data)?;

        let bc = ByteCode::disassemble(&data, true);
        if let Err(ref e) = bc {
            if !DisassemblyError::maybe_show(e, &data) {
                println!("ERROR: {}", e);
            }
        }
        let bc = bc?;
        println!("Results:\n{}", bc);
    }

    Ok(())
}
