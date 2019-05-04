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
use omnilib::{make_opt_struct, OmniLib};
use pal::Palette;
use pic::Pic;
use std::{fs, iter};
use structopt::StructOpt;

make_opt_struct!(#[structopt(
    name = "picdump",
    about = "Extract PICs to PNG files and show PIC metadata"
)]
Opt {
    #[structopt(
        short = "s",
        long = "show-palette",
        help = "Dump the palette here as a PNG"
    )]
    show_palette => Option<String>,

    #[structopt(
        short = "d",
        long = "dump-palette",
        help = "Dump the palette here as a PAL"
    )]
    dump_palette => Option<String>,

    #[structopt(
        short = "b",
        long = "gray-scale",
        help = "Output as grayscale rather than palettized"
    )]
    grayscale => bool,

    #[structopt(
        short = "u",
        long = "use-palette",
        help = "Use the given palette when decoding"
    )]
    use_palette => Option<String>,

    #[structopt(
        short = "o",
        long = "output",
        help = "Write the image to the given file"
    )]
    write_image => Option<String>
});

fn main() -> Fallible<()> {
    let opt = Opt::from_args();

    let (omni, inputs) = opt.find_inputs()?;
    if inputs.is_empty() {
        println!("No inputs found!");
        return Ok(());
    }

    for (game, name) in &inputs {
        let lib = omni.library(&game);
        let content = lib.load(&name)?;
        let image = Pic::from_bytes(&content)?;

        println!("{}:{}", game, name);
        println!(
            "{}",
            iter::repeat("=")
                .take(1 + game.len() + name.len())
                .collect::<String>()
        );
        println!("format: {:?}", image.format);
        println!("width:  {}px", image.width);
        println!("height: {}px", image.height);
        if let Some(pal) = image.palette {
            println!("colors: {:?}", pal.color_count);

            if let Some(ref path) = opt.show_palette {
                pal.dump_png(&path)?;
            }

            if let Some(ref path) = opt.dump_palette {
                pal.dump_pal(&path)?;
            }
        }

        if let Some(target) = &opt.write_image {
            let palette = if let Some(path) = &opt.use_palette {
                Palette::from_bytes(&fs::read(path)?)?
            } else if opt.grayscale {
                Palette::grayscale()?
            } else {
                Palette::from_bytes(&lib.load("PALETTE.PAL")?)?
            };
            let image = Pic::decode(&palette, &content)?;
            image.save(target)?;
        }
    }

    Ok(())
}
