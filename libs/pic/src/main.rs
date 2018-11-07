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
extern crate clap;
extern crate failure;
extern crate image;
extern crate pal;
extern crate pic;

use clap::{App, Arg};
use failure::Fallible;
use pal::Palette;
use pic::decode_pic;
use std::fs;
use std::io::prelude::*;

fn main() -> Fallible<()> {
    let matches = App::new("OpenFA pic tool")
        .version("0.0.1")
        .author("Terrence Cole <terrence.d.cole@gmail.com>")
        .about("Convert a pic to a png.")
        .arg(
            Arg::with_name("INPUT")
                .help("The pics to convert")
                .multiple(true)
                .required(true),
        ).get_matches();

    let mut fp = fs::File::open("../pal/test_data/PALETTE.PAL")?;
    let mut palette_data = Vec::new();
    fp.read_to_end(&mut palette_data)?;
    let palette = Palette::from_bytes(&palette_data)?;

    for name in matches.values_of("INPUT").unwrap() {
        println!("Converting: {}", name);
        let mut fp = fs::File::open(name)?;
        let mut data = Vec::new();
        fp.read_to_end(&mut data)?;

        let img = decode_pic(&palette, &data)?;
        img.save(name.to_owned() + ".png")?;
    }

    Ok(())
}
