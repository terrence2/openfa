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
use image::GenericImageView;
use lib::{GameInfo, Libs, LibsOpts};
use pal::Palette;
use pic::Pic;
use std::{
    env, fs,
    path::{Path, PathBuf},
};
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

    #[structopt(flatten)]
    libs_opts: LibsOpts,
}

fn main() -> Result<()> {
    let opt = Opt::from_args();
    let files = Libs::input_files(&opt.inputs, "*.PIC")?;
    if !files.is_empty() {
        if let Some(exe_root) = env::current_exe()?.parent() {
            env::set_current_dir(exe_root)?;
        }
    }
    let libs = Libs::bootstrap(&opt.libs_opts)?;
    for (game, palette, catalog) in libs.selected() {
        for input in &opt.inputs {
            for fid in catalog.find_glob(input)? {
                let meta = catalog.stat(fid)?;
                println!("{}:{:13} @ {}", game.test_dir, meta.name(), meta.path());
                show_pic(fid, game, palette, catalog, &opt)?;
            }
        }
    }
    if let Some((_, palette, _)) = libs.selected().next() {
        for path in &files {
            transcribe_pic(path, palette)?;
        }
    }

    Ok(())
}

fn transcribe_pic(path: &Path, palette: &Palette) -> Result<()> {
    let image = Pic::decode(palette, &fs::read(path)?)?;
    let mut path = path.to_owned();
    path.set_extension("png");
    image.save(path)?;
    Ok(())
}

fn show_pic(
    fid: FileId,
    game: &GameInfo,
    system_palette: &Palette,
    catalog: &Catalog,
    opt: &Opt,
) -> Result<()> {
    let meta = catalog.stat(fid)?;
    let content = catalog.read(fid)?;
    let image = Pic::from_bytes(content.as_ref())?;

    println!(
        "{}",
        "=".repeat(1 + game.test_dir.len() + meta.name().len())
    );
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
            system_palette.to_owned()
        };
        let image = Pic::decode(&palette, &content)?;
        let mut path = PathBuf::from(target);
        if path.is_dir() {
            path.push(meta.name().replace("PIC", "PNG"));
            image.save(path)?;
        } else {
            image.save(path)?;
        }
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

    Ok(())
}
