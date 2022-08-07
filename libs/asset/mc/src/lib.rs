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
use i386::{Disassembler, MemBlock};
use peff::PortableExecutable;
use std::{fs::File, io::Write};

pub const MC_LOAD_BASE: u32 = 0xAA00_0000;

pub struct MissionCommandScript {
    _heap: Vec<MemBlock>,
}

impl MissionCommandScript {
    pub fn from_bytes(data: &[u8], name: &str) -> Result<Self> {
        let mut pe = PortableExecutable::from_bytes(data)?;
        pe.relocate(MC_LOAD_BASE)?;

        if false {
            let p = std::env::current_dir()?;
            let p = p.parent().unwrap();
            let p = p.parent().unwrap();
            let mut p = p.parent().unwrap().to_owned();
            p.push("__dump__");
            p.push("mc");
            p.push(name.replace(':', "_"));
            let p = p.with_extension("asm");
            let mut fp = File::create(p)?;
            fp.write_all(&pe.code)?;
        }

        let mut disasm = Disassembler::default();
        disasm.disassemble_at(0, &pe)?;
        Ok(Self {
            _heap: disasm.build_memory_view(&pe),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lib::Libs;

    #[test]
    fn it_works() -> Result<()> {
        env_logger::init();
        let libs = Libs::for_testing()?;
        for (game, _palette, catalog) in libs.all() {
            for fid in catalog.find_with_extension("MC")? {
                let meta = catalog.stat(fid)?;
                println!(
                    "At: {:>7}:{:13} @ {}",
                    game.test_dir,
                    meta.name(),
                    meta.path()
                );

                let data = catalog.read(fid)?;
                let _mc = MissionCommandScript::from_bytes(
                    data.as_ref(),
                    &format!("{}:{}", game.test_dir, meta.name()),
                )?;
            }
        }

        Ok(())
    }
}
