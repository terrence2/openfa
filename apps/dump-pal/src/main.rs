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
use catalog::{Catalog, FileId};
use lib::{GameInfo, Libs, LibsOpts};
use pal::Palette;
use std::fs;
use structopt::StructOpt;

/// Dump and query PAL files
#[derive(Debug, StructOpt)]
struct Opt {
    /// Dump the palette to a png
    #[structopt(short, long)]
    dump: bool,

    /// Show the color at this offset
    #[structopt(short, long)]
    position: Option<String>,

    /// PAL file to analyze
    inputs: Vec<String>,

    #[structopt(flatten)]
    libs_opts: LibsOpts,
}

fn main() -> Result<()> {
    let opt = Opt::from_args();
    let libs = Libs::bootstrap(&opt.libs_opts)?;
    for (game, catalog) in libs.selected() {
        for input in &opt.inputs {
            for fid in catalog.find_glob(input)? {
                let meta = catalog.stat(fid)?;
                println!("{}:{:13} @ {}", game.test_dir, meta.name(), meta.path());
                println!(
                    "{}",
                    "=".repeat(1 + game.test_dir.len() + meta.name().len())
                );
                show_pal(fid, game, &catalog, &opt)?;
            }
        }
    }

    Ok(())
}

fn show_pal(fid: FileId, game: &GameInfo, catalog: &Catalog, opt: &Opt) -> Result<()> {
    let meta = catalog.stat(fid)?;
    let pal = Palette::from_bytes(&catalog.read(fid)?)?;
    if opt.dump {
        println!("Dumping palette: {}:{}", game.test_dir, meta.name());

        let size = 80;
        let mut buf = image::ImageBuffer::new(16u32 * size, 16u32 * size);
        for i in 0..16 {
            for j in 0..16 {
                let off = (j << 4 | i) as usize;
                for ip in 0..size {
                    for jp in 0..size {
                        buf.put_pixel(i * size + ip, j * size + jp, pal.rgb(off)?);
                    }
                }
            }
        }
        fs::create_dir_all(&format!("dump/palette/{}-{}", game.test_dir, meta.name()))?;
        let output = format!("dump/palette/{}-{}/palette.png", game.test_dir, meta.name());
        buf.save(&output)?;

        return Ok(());
    }

    if let Some(pos_str) = opt.position.clone() {
        let pos = if let Some(hex) = pos_str.strip_prefix("0x") {
            usize::from_str_radix(hex, 16)?
        } else {
            pos_str.parse::<usize>()?
        };
        println!("{:02X} => {:?}", pos, pal.rgb(pos)?);
    }
    println!();

    Ok(())
}
