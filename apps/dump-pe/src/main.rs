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
use anyhow::Result;
use catalog::{Catalog, FileId};
use i386::{Disassembler, DisassemblyError};
use lib::{Libs, LibsOpts};
use peff::PortableExecutable;
use std::collections::HashSet;
use structopt::StructOpt;

/// Dump PE files
#[derive(Debug, StructOpt)]
struct Opt {
    /// PE files to dump
    inputs: Vec<String>,

    #[structopt(short, long)]
    disassemble: bool,

    #[structopt(flatten)]
    libs_opts: LibsOpts,
}

fn main() -> Result<()> {
    env_logger::init();
    let opt = Opt::from_args();
    let libs = Libs::bootstrap(&opt.libs_opts)?;
    for (game, _palette, catalog) in libs.selected() {
        for input in &opt.inputs {
            for fid in catalog.find_glob(input)? {
                let meta = catalog.stat(fid)?;
                println!("{}:{:13} @ {}", game.test_dir, meta.name(), meta.path());
                println!(
                    "{}",
                    "=".repeat(1 + game.test_dir.len() + meta.name().len())
                );
                show_pe(fid, catalog, opt.disassemble)?;
            }
        }
    }

    Ok(())
}

fn show_pe(fid: FileId, catalog: &Catalog, disassemble: bool) -> Result<()> {
    let (_, width) = terminal_size();
    let relocs_per_line = (width - 3) / 7;
    let bytes_per_line = (width - 3) / 3;

    let content = catalog.read(fid)?;
    let mut pe = PortableExecutable::from_bytes(&content)?;
    pe.relocate(0xAA00_0000)?;

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
    println!("\n");

    if disassemble {
        let mut disasm = Disassembler::default();
        let out = disasm.disassemble_at(0, &pe);
        if let Err(ref e) = out {
            if !DisassemblyError::maybe_show(e, &pe.code) {
                println!("ERROR: {}", e);
            }
        }

        println!("i386 -");
        for bc in disasm.build_memory_view(&pe) {
            println!("{}", bc);
        }
    } else {
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
    }

    println!();
    Ok(())
}
