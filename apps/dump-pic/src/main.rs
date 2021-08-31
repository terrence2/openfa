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
use image::GenericImageView;
use lib::CatalogBuilder;
use pal::Palette;
use pic::Pic;
use std::fs;
use structopt::StructOpt;

/// Extract PICs to PNG files and show PIC metadata
#[derive(Debug, StructOpt)]
struct Opt {
    /// Dump the palette here as a PNG
    #[structopt(short, long)]
    show_palette: Option<String>,

    /// Dump the palette here as a PAL
    #[structopt(short, long)]
    dump_palette: Option<String>,

    /// Print the image as ascii
    #[structopt(short = "a", long = "ascii")]
    show_ascii: bool,

    /// Output as grayscale rather than palettized
    #[structopt(short = "b", long = "gray-scale")]
    grayscale: bool,

    /// Use the given palette when decoding
    #[structopt(short = "u", long = "use-palette")]
    use_palette: Option<String>,

    /// Write the image to the given file
    #[structopt(short = "o", long = "output")]
    write_image: Option<String>,

    /// One or more PIC files to process
    inputs: Vec<String>,
}

fn main() -> Result<()> {
    let opt = Opt::from_args();
    let (catalog, inputs) = CatalogBuilder::build_and_select(&opt.inputs)?;
    if inputs.is_empty() {
        println!("No inputs found!");
        return Ok(());
    }

    for &fid in &inputs {
        let label = catalog.file_label(fid)?;
        let game = label.split(':').last().unwrap();
        let meta = catalog.stat_sync(fid)?;

        let content = catalog.read_sync(fid)?;
        let image = Pic::from_bytes(&content)?;

        println!("{}:{}", game, meta.name());
        println!("{}", "=".repeat(1 + game.len() + meta.name().len()));
        println!("format: {:?}", image.format());
        println!("width:  {}px", image.width());
        println!("height: {}px", image.height());
        if let Some(pal) = image.palette() {
            println!("colors: {:?}", pal.color_count);

            if let Some(ref path) = opt.show_palette {
                pal.dump_png(path)?;
            }

            if let Some(ref path) = opt.dump_palette {
                pal.dump_pal(path)?;
            }
        }

        if let Some(target) = &opt.write_image {
            let palette = if let Some(path) = &opt.use_palette {
                Palette::from_bytes(&fs::read(path)?)?
            } else if opt.grayscale {
                Palette::grayscale()?
            } else {
                Palette::from_bytes(&catalog.read_labeled_name_sync(&label, "PALETTE.PAL")?)?
            };
            let image = Pic::decode(&palette, &content)?;
            image.save(target)?;
        }

        if opt.show_ascii {
            let palette = Palette::grayscale()?;
            let image = Pic::decode(&palette, &content)?;
            let (width, height) = image.dimensions();
            for h in 0..height {
                for w in 0..width {
                    print!("{:02X} ", image.get_pixel(w, h)[0]);
                }
                println!();
            }
        }
    }

    Ok(())
}
