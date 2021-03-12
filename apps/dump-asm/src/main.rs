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
use anyhow::Result;
use i386::{ByteCode, DisassemblyError};
use simplelog::{Config, LevelFilter, TermLogger};
use std::{fs, io::prelude::*, path::PathBuf};
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "disasm", about = "Disassemble and show an assembly fragment.")]
struct Opt {
    /// Trace disassembly process
    #[structopt(short = "v", long = "verbose")]
    verbose: bool,

    /// Input file
    #[structopt(parse(from_os_str))]
    input: PathBuf,
}

fn main() -> Result<()> {
    let opt = Opt::from_args();

    let level = if opt.verbose {
        LevelFilter::Trace
    } else {
        LevelFilter::Debug
    };
    TermLogger::init(level, Config::default())?;

    let mut fp = fs::File::open(opt.input)?;
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
