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
use dxf::{entities::*, Drawing};
use peff::PortableExecutable;
use std::{
    fs,
    fs::OpenOptions,
    io::{Seek, SeekFrom, Write},
    path::PathBuf,
};
use structopt::StructOpt;

/// Inject a DXF exported by dump-sh back into a SH
#[derive(Debug, StructOpt)]
struct Opt {
    /// The DXF file to pull from
    #[structopt(short = "d", long = "dxf")]
    gltf_input: PathBuf,

    /// The SH file to pull from (destructive unless using --output !!!)
    #[structopt(short = "s", long = "sh")]
    sh_input: PathBuf,

    /// The SH file to write to (write into --sh if not specified)
    #[structopt(short = "o", long = "output")]
    output: Option<PathBuf>,
}

fn main() -> Result<()> {
    env_logger::init();
    let opt = Opt::from_args();

    let source = fs::read(&opt.sh_input)?;
    let container = PortableExecutable::from_bytes(&source)?;
    let code_offset = container.section_info["CODE"].file_offset();
    let target_path = if let Some(output_path) = opt.output {
        fs::copy(&opt.sh_input, &output_path)?;
        output_path
    } else {
        opt.sh_input
    };
    let mut update = OpenOptions::new()
        .write(true)
        .read(false)
        .create(false)
        .create_new(false)
        .append(false)
        .truncate(false)
        .open(target_path)?;

    /*
    let drawing = Drawing::load_file(opt.dxf_input)?;

    for e in drawing.entities() {
        #[allow(clippy::single_match)]
        match e.specific {
            EntityType::Vertex(ref vert) => {
                let vert_offset = code_offset + vert.identifier as u32;
                update.seek(SeekFrom::Start(vert_offset as u64))?;
                update.write_all(&(vert.location.x as i16).to_le_bytes())?;
                update.write_all(&(vert.location.y as i16).to_le_bytes())?;
                update.write_all(&(vert.location.z as i16).to_le_bytes())?;
                //println!("Vertex {} on {}", vert.identifier, e.common.layer);
            }
            _ => (),
        }
    }
     */

    Ok(())
}
