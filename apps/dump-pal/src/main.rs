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
use failure::Fallible;
use lib::CatalogBuilder;
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
}

fn main() -> Fallible<()> {
    let opt = Opt::from_args();
    let (catalog, inputs) = CatalogBuilder::build_and_select(&opt.inputs)?;
    for &fid in &inputs {
        let label = catalog.file_label(fid)?;
        let game = label.split(':').last().unwrap();
        let meta = catalog.stat_sync(fid)?;

        let pal = Palette::from_bytes(&catalog.read_sync(fid)?)?;
        if opt.dump {
            println!("Dumping palette: {}:{}", label, meta.name);

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
            let img = image::RgbImage::from(buf);
            fs::create_dir_all(&format!("dump/palette/{}-{}", game, meta.name))?;
            let output = format!("dump/palette/{}-{}/palette.png", game, meta.name);
            img.save(&output)?;

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
    }

    Ok(())
}
