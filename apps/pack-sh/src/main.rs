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
use gltf::{
    buffer::Source,
    mesh::{Mode, Semantic},
    Gltf,
};
use packed_struct::packed_struct;
use peff::PortableExecutable;
use std::{
    fs,
    fs::{File, OpenOptions},
    io,
    io::{Seek, SeekFrom, Write},
    path::PathBuf,
};
use structopt::StructOpt;

/// Inject a DXF exported by dump-sh back into a SH
#[derive(Debug, StructOpt)]
struct Opt {
    /// The GLTF to pull points from
    #[structopt(short = "g", long = "gltf")]
    gltf_input: PathBuf,

    /// The SH file to pull from (destructive unless using --output !!!)
    #[structopt(short = "s", long = "sh")]
    sh_input: PathBuf,

    /// The SH file to write to (write into --sh if not specified)
    #[structopt(short = "o", long = "output")]
    output: Option<PathBuf>,
}

#[packed_struct]
struct Vertex {
    position: [f32; 3],
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

    let gltf_path = opt.gltf_input;
    let gltf_dir = gltf_path.parent().expect("gltf in subdir").to_owned();
    let gltf = Gltf::from_reader(io::BufReader::new(File::open(&gltf_path)?))?;
    for scene in gltf.scenes() {
        for node in scene.nodes() {
            let name = node.name().expect("name");
            let parts = name.split('-').collect::<Vec<_>>();
            assert_eq!(parts[0], "vxbuf");
            let mesh = node.mesh().expect("mesh");
            let prim = mesh.primitives().next().expect("primitives");
            assert_eq!(prim.mode(), Mode::Points);
            let (kind, accessor) = prim.attributes().next().expect("primitive");
            assert_eq!(kind, Semantic::Positions);
            let view = accessor.view().expect("view");
            assert_eq!(parts[1].parse::<usize>()?, view.index());
            let vxbuf_code_address = usize::from_str_radix(parts[2], 16)?;
            let vxbuf_file_address = code_offset as usize + vxbuf_code_address;
            update.seek(SeekFrom::Start(vxbuf_file_address as u64 + 6))?;
            let buffer = view.buffer();
            let bin_name = match buffer.source() {
                Source::Uri(filename) => filename,
                Source::Bin => panic!("expected separated bin files!"),
            };
            let mut bin_path = gltf_dir.clone();
            bin_path.push(bin_name);
            let data = fs::read(bin_path)?;
            let data = &data[view.offset()..view.offset() + view.length()];
            let verts = Vertex::overlay_slice(data)?;
            println!(
                "patching: {} - {:?} @ 0x{:08X}",
                node.index(),
                node.name(),
                vxbuf_file_address
            );
            for (i, vert) in verts.iter().enumerate() {
                let address = vxbuf_file_address + (i + 1) * 6;
                update.seek(SeekFrom::Start(address as u64))?;
                update.write_all(&(vert.position[0] as i16).to_le_bytes())?;
                update.write_all(&(vert.position[1] as i16).to_le_bytes())?;
                update.write_all(&(vert.position[2] as i16).to_le_bytes())?;
            }
        }
    }

    Ok(())
}
