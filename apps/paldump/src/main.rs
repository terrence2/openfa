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
use clap::{App, Arg};
use failure::Error;
use pal::Palette;
use std::{fs, io::Read};

fn main() -> Result<(), Error> {
    let matches = App::new("OpenFA palette dumper")
        .version("0.0.1")
        .author("Terrence Cole <terrence.d.cole@gmail.com>")
        .about("Figure out what bits belong where.")
        .arg(
            Arg::with_name("dump")
                .long("--dump")
                .short("-d")
                .help("dump the palette to a png")
                .takes_value(false)
                .required(false),
        )
        .arg(
            Arg::with_name("POSITIONS")
                .help("The palette indexes to dump")
                .multiple(true)
                .required(false),
        )
        .get_matches();

    let mut fp = fs::File::open("test_data/PALETTE.PAL")?;
    let mut data = Vec::new();
    fp.read_to_end(&mut data)?;
    let pal = Palette::from_bytes(&data)?;

    if matches.is_present("dump") {
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

    if let Some(positions) = matches.values_of("POSITIONS") {
        for pos_str in positions {
            let pos = if pos_str.starts_with("0x") {
                usize::from_str_radix(&pos_str[2..], 16)?
            } else {
                pos_str.parse::<usize>()?
            };
            println!("{:02X} => {:?}", pos, pal.rgb(pos)?);
        }
    }

    Ok(())
}
