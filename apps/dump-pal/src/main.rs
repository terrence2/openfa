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
use structopt::StructOpt;

make_opt_struct!(
    #[structopt(name = "paldump", about = "Show the contents of a PAL file")]
    Opts {
        #[structopt(short = "d", long = "dump", help = "Dump the palette to a png")]
        dump => bool,

        #[structopt(short = "p", long = "position", help = "Show the color at this offset offset")]
        position => Option<String>,

        #[structopt(help = "PAL file to context")]
        omni_input => String
    }
);

fn main() -> Fallible<()> {
    let opt = Opts::from_args();

    let (omni, game, name) = opt.find_input(&opt.omni_input)?;
    let lib = omni.library(&game);
    let pal = Palette::from_bytes(&lib.load(&name)?)?;

    if opt.dump {
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
        let img = image::ImageRgb8(buf);
        img.save("palette.png")?;

        return Ok(());
    }

    if let Some(pos_str) = opt.position {
        let pos = if pos_str.starts_with("0x") {
            usize::from_str_radix(&pos_str[2..], 16)?
        } else {
            pos_str.parse::<usize>()?
        };
        println!("{:02X} => {:?}", pos, pal.rgb(pos)?);
    }

    Ok(())
}
