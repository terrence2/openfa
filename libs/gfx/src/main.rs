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
extern crate gfx;

use clap::{App, Arg};
use failure::Error;
use gfx::Window;

fn main() -> Result<(), Error> {
    let matches = App::new("OpenFA gfx tool")
        .version("0.0.1")
        .author("Terrence Cole <terrence.d.cole@gmail.com>")
        .about("Show info about the current gfx environment.")
        .arg(
            Arg::with_name("list")
                .long("--list")
                .takes_value(false)
                .required(false),
        )
        .get_matches();

    if matches.is_present("list") {
        let win = Window::new(800, 600, "gfx").unwrap();
        for (i, ref adapter) in win.enumerate_adapters().iter().enumerate() {
            println!(
                "{}: {} (vendor: {} / device: {}){}",
                i,
                adapter.info.name,
                adapter.info.vendor,
                adapter.info.device,
                if adapter.info.software_rendering {
                    " SOFTWARE"
                } else {
                    ""
                }
            );
            println!("    Capabilities: {:?}", win.capabilities(&adapter));
            println!("    Formats: {:?}", win.formats(&adapter));
            println!(
                "    Presentation Modes: {:?}",
                win.presentation_modes(&adapter)
            );
        }
    }

    return Ok(());
}
