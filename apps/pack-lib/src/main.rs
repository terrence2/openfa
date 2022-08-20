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
use lib::LibWriter;
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "pack")]
/// A LIB packing tool for all Janes Fighters Anthology series games
struct Opts {
    #[structopt(short = "-o", long = "--output", parse(from_os_str))]
    /// Output unpacked files into this directory
    output_lib: PathBuf,

    #[structopt(parse(from_os_str))]
    /// The files to pack
    inputs: Vec<PathBuf>,
}

fn main() -> Result<()> {
    env_logger::init();
    let opts = Opts::from_args();
    let mut writer = LibWriter::new(&opts.output_lib, u16::try_from(opts.inputs.len())?)?;
    for input in &opts.inputs {
        writer.add_file(input)?;
    }
    writer.finish()?;
    Ok(())
}
