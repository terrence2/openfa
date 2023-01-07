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
use anyhow::{ensure, Context, Result};
use gltf::{
    buffer::Source,
    mesh::{Mode, Semantic},
    Gltf,
};
use packed_struct::packed_struct;
use peff::PortableExecutable;
use sh::Record;
use std::{
    fs,
    fs::{File, OpenOptions},
    io,
    io::{Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};
use structopt::StructOpt;

/// Inject a DXF exported by dump-sh back into a SH
#[derive(Debug, StructOpt)]
struct Opt {
    /// The GLTF to pull points from
    #[structopt(short = "g", long = "gltf")]
    gltf_input: Option<PathBuf>,

    /// The CSV to pull writes from
    #[structopt(short = "c", long = "csv")]
    csv_input: Option<PathBuf>,

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
    let container = PortableExecutable::from_bytes(&source)
        .with_context(|| format!("reading shape {}", opt.sh_input.display()))?;
    let code_offset = container.section_info["CODE"].file_offset();
    let target_path = if let Some(output_path) = opt.output {
        fs::copy(&opt.sh_input, &output_path)?;
        output_path
    } else {
        opt.sh_input
    };
    let update = OpenOptions::new()
        .write(true)
        .read(false)
        .create(false)
        .create_new(false)
        .append(false)
        .truncate(false)
        .open(&target_path)
        .with_context(|| format!("opening {} for update", target_path.display()))?;

    if let Some(path) = &opt.gltf_input {
        update_from_gltf(update, code_offset, path)?;
    } else if let Some(path) = &opt.csv_input {
        update_from_csv(update, code_offset, path)?;
    }

    Ok(())
}

fn update_from_csv(mut update: File, code_offset: u32, csv_path: &Path) -> Result<()> {
    let mut cnt = 0;
    let mut rdr = csv::Reader::from_reader(
        File::open(csv_path)
            .with_context(|| format!("opening CSV file at {}", csv_path.display()))?,
    );
    for (i, result) in rdr.deserialize().enumerate() {
        let record: Record = result?;
        cnt += 1;

        ensure!(record.instr_number == i, "mismatched instruction number!");
        ensure!(
            record.file_offset == record.code_offset + code_offset as usize,
            "CSV code offset does nto match shape code offset"
        );
        let content = record
            .raw_content
            .split(' ')
            .filter(|s| !s.is_empty())
            .map(|s| u8::from_str_radix(s, 16).expect("Non-byte in content"))
            .collect::<Vec<u8>>();
        ensure!(
            content.len() == record.instr_size,
            "content length does not mach instruction size"
        );
        update.seek(SeekFrom::Start(record.file_offset as u64))?;
        update.write_all(&content)?;

        print!(".");
        std::io::stdout().flush()?;
    }
    println!("\nWrote {} records!", cnt);
    Ok(())
}

fn update_from_gltf(mut update: File, code_offset: u32, gltf_path: &Path) -> Result<()> {
    let gltf_dir = gltf_path.parent().expect("gltf in subdir").to_owned();
    let gltf = Gltf::from_reader(io::BufReader::new(File::open(gltf_path)?))?;
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
