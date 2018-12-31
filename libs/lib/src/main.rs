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

// Unpack lib files.

extern crate clap;
extern crate failure;
extern crate humansize;
extern crate lib;

use clap::{App, Arg, SubCommand};
use failure::Error;
use humansize::{file_size_opts as options, FileSize};
use std::{
    fs::{create_dir, remove_file, File},
    io::Write,
    path::Path,
};

fn main() -> Result<(), Error> {
    let matches = App::new("openfa lib unpacker")
        .version("0.1")
        .author("Terrence Cole <terrence.d.cole@gmail.com>")
        .about("List and unpack part or all of an EA lib file or files")
        .subcommand(
            SubCommand::with_name("ls")
                .about("list the contents of a lib")
                .arg(
                    Arg::with_name("INPUT")
                        .help("Sets the input libs to unpack")
                        .required(true)
                        .multiple(true),
                ),
        )
        .subcommand(
            SubCommand::with_name("unpack")
                .about("unpack the given lib files")
                .arg(
                    Arg::with_name("output")
                        .long("--output")
                        .short("-o")
                        .required(true)
                        .takes_value(true)
                        .value_name("DIR")
                        .help("output into the given directory"),
                )
                .arg(
                    Arg::with_name("INPUT")
                        .help("Sets the input libs to unpack")
                        .required(true)
                        .multiple(true),
                ),
        )
        .get_matches();

    if let Some(matches) = matches.subcommand_matches("ls") {
        let inputs = matches.values_of("INPUT").unwrap();
        let multi_input = inputs.len() > 1;
        for (i, input) in inputs.enumerate() {
            let libfile = lib::Library::from_paths(&[Path::new(input).to_owned()])?;
            if multi_input {
                if i != 0 {
                    println!();
                }
                println!("{}:", input);
            }
            for name in libfile.find_matching("*")?.iter() {
                let info = libfile.stat(name)?;
                let mut psize = info.packed_size.file_size(options::BINARY).unwrap();
                if psize.ends_with(" B") {
                    psize += "  ";
                }
                let mut asize = info.unpacked_size.file_size(options::BINARY).unwrap();
                if asize.ends_with(" B") {
                    asize += "  ";
                }
                let ratio = if info.packed_size == info.unpacked_size {
                    "~".to_owned()
                } else {
                    format!(
                        "{:0.3}x",
                        info.packed_size as f64 / info.unpacked_size as f64
                    )
                };

                println!(
                    "{:15} {:?} {:>12} {:>12}  {}",
                    info.name, info.compression, psize, asize, ratio
                );
            }
        }
    }

    if let Some(matches) = matches.subcommand_matches("unpack") {
        let inputs = matches.values_of("INPUT").expect("no inputs specified");
        let outname: String = matches
            .value_of("output")
            .expect("no output specified")
            .to_owned();
        let output = Path::new(&outname);
        for input in inputs {
            let libname = Path::new(input)
                .file_name()
                .expect("no filename in library");
            let outdir = output.join(libname);
            let libfile = lib::Library::from_paths(&[Path::new(input).to_owned()])?;
            if !outdir.exists() {
                create_dir(&outdir)?;
            }
            for name in libfile.find_matching("*")?.iter() {
                let outfilename = outdir.join(name);
                println!("{}:{} -> {:?}", input, name, outfilename);
                let content = libfile.load(name)?;
                if outfilename.exists() {
                    remove_file(&outfilename)?;
                }
                let mut fp = File::create(outfilename)?;
                fp.write_all(&content)?;
            }
        }
    }

    Ok(())
}
