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

// Dump a list of files to hex and show them in alphabetical order.
extern crate clap;
extern crate reverse;

use clap::{Arg, App};
use std::fs;
use std::io::prelude::*;
use reverse::b2h;

fn main() {
    let matches = App::new("OpenFA reversing tool")
        .version("0.0.1")
        .author("Terrence Cole <terrence.d.cole@gmail.com>")
        .about("Show hex dump of many files at once.")
        .arg(Arg::with_name("INPUT")
            .help("The pics to convert")
            .multiple(true)
            .required(true))
        .get_matches();

    let mut strs = Vec::new();
    for name in matches.values_of("INPUT").unwrap() {
        let mut fp = fs::File::open(name).unwrap();
        let mut data = Vec::new();
        fp.read_to_end(&mut data).unwrap();

        let mut hex = Vec::new();
        for b in data {
            b2h(b, &mut hex);
            hex.push(' ');
        }

        strs.push((name, hex.iter().collect::<String>()));
    }
    strs.sort_by(|&(_, ref a), &(_, ref b)| a.cmp(b));
    for (name, hex) in strs {
        println!("{:32} - {}", name, hex);
    }
}
