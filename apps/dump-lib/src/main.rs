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
use catalog::Catalog;
use failure::Fallible;
use humansize::{file_size_opts as options, FileSize};
use lib::LibDrawer;
use std::{
    fs::{create_dir, remove_file, File},
    io::Write,
    path::PathBuf,
};
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "unpack")]
/// A LIB unpacking tool for all Janes Fighters Anthology series games
enum Opt {
    #[structopt(name = "ls")]
    /// List the contents of a lib
    List {
        #[structopt(parse(from_os_str))]
        /// The lib files to list
        inputs: Vec<PathBuf>,
    },

    #[structopt(name = "unpack")]
    /// Unpack the given lib file
    Unpack {
        #[structopt(short = "-o", long = "--output", parse(from_os_str))]
        /// Output unpacked libs into this directory
        output_path: PathBuf,

        #[structopt(parse(from_os_str))]
        /// The lib files to unpack
        inputs: Vec<PathBuf>,
    },
}

fn main() -> Fallible<()> {
    let opt = Opt::from_args();

    match opt {
        Opt::List { inputs } => handle_ls(inputs),
        Opt::Unpack {
            inputs,
            output_path,
        } => handle_unpack(inputs, output_path),
    }
}

fn handle_ls(inputs: Vec<PathBuf>) -> Fallible<()> {
    let multi_input = inputs.len() > 1;
    for (i, input) in inputs.iter().enumerate() {
        let catalog = Catalog::with_drawers(vec![LibDrawer::from_path(0, input)?])?;
        if multi_input {
            if i != 0 {
                println!();
            }
            println!("{}:", input.to_string_lossy());
        }
        for name in catalog.find_matching_names("*")?.iter() {
            let info = catalog.stat_name_sync(name)?;
            let mut psize = info.packed_size.file_size(options::BINARY).unwrap();
            if psize.ends_with(" B") {
                psize += "  ";
            }
            let mut asize = info.unpacked_size.file_size(options::BINARY).unwrap();
            if asize.ends_with(" B") {
                asize += "  ";
            }
            let ratio = if info.packed_size == info.unpacked_size && info.unpacked_size > 0 {
                "~".to_owned()
            } else {
                format!(
                    "{:0.3}x",
                    info.packed_size as f64 / info.unpacked_size as f64
                )
            };

            println!(
                "{:15} {:<8} {:>12} {:>12}  {}",
                info.name,
                info.compression.unwrap_or("none"),
                psize,
                asize,
                ratio
            );
        }
    }

    Ok(())
}

fn handle_unpack(inputs: Vec<PathBuf>, output_path: PathBuf) -> Fallible<()> {
    for input in &inputs {
        let libname = input.file_name().expect("no filename in library");
        let outdir = output_path.join(libname);
        let catalog = Catalog::with_drawers(vec![LibDrawer::from_path(0, input)?])?;
        if !outdir.exists() {
            create_dir(&outdir)?;
        }
        for name in catalog.find_matching_names("*")?.iter() {
            let outfilename = outdir.join(name);
            println!("{:?}:{} -> {:?}", input, name, outfilename);
            let content = catalog.read_name_sync(name)?;
            if outfilename.exists() {
                remove_file(&outfilename)?;
            }
            let mut fp = File::create(outfilename)?;
            fp.write_all(&content)?;
        }
    }

    Ok(())
}
