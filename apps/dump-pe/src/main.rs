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
use ansi::{ansi, terminal_size};
use failure::Fallible;
use omnilib::{make_opt_struct, OmniLib};
use peff::PE;
use std::{collections::HashSet, iter};
use structopt::StructOpt;

make_opt_struct!(
    #[structopt(
        name = "pedump",
        about = "Show the contents of a PF portable executable file"
    )]
    Opts {
        #[structopt(help = "PE files to output")]
        omni_inputs => Vec<String>
    }
);

fn main() -> Fallible<()> {
    let opt = Opts::from_args();

    let (_, width) = terminal_size();
    let relocs_per_line = (width - 3) / 7;
    let bytes_per_line = (width - 3) / 3;

    let (omni, inputs) = opt.find_inputs(&opt.omni_inputs)?;
    if inputs.is_empty() {
        println!("No inputs found!");
        return Ok(());
    }

    for (game, name) in &inputs {
        let lib = omni.library(&game);
        let content = lib.load(&name)?;
        let pe = PE::from_bytes(&content)?;

        println!("{}:{}", game, name);
        println!(
            "{}",
            iter::repeat("=")
                .take(1 + game.len() + name.len())
                .collect::<String>()
        );
        println!("image base: 0x{:08X}", pe.image_base);

        for (name, section) in &pe.section_info {
            println!("{} @", name);
            println!("\tvaddr: 0x{:04X}", section.virtual_address);
            println!("\tvsize: 0x{:04X}", section.virtual_size);
            println!("\trawsz: 0x{:04X}", section.size_of_raw_data);
        }

        println!("thunks -");
        for thunk in &pe.thunks {
            println!(
                "\t{:>2} - {:20} @ 0x{:04X}",
                thunk.ordinal, thunk.name, thunk.vaddr
            );
        }

        println!("relocs -");
        print!("  ");
        let mut relocs = HashSet::new();
        let mut offset = 0;
        for reloc in &pe.relocs {
            assert!(*reloc <= 0xFFFF);
            if offset == relocs_per_line {
                offset = 0;
                println!();
                print!("  ");
            }
            relocs.insert(*reloc);
            relocs.insert(*reloc + 1);
            relocs.insert(*reloc + 2);
            relocs.insert(*reloc + 3);
            print!("0x{:04X} ", reloc);
            offset += 1;
        }
        println!();

        println!("code -");
        print!("  ");
        let mut offset = 0;
        for (i, b) in pe.code.iter().enumerate() {
            if offset == bytes_per_line {
                offset = 0;
                println!();
                print!("  ");
            }
            if relocs.contains(&(i as u32)) {
                print!("{}{:02X}{} ", ansi().green(), b, ansi());
            } else {
                print!("{:02X} ", b);
            }
            offset += 1;
        }
        println!();

        println!();
    }

    Ok(())
}
