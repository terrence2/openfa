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
use i386::{ByteCode, Memonic};
use reverse::bs2s;
use std::{fs::File, io::Write};

pub const MC_LOAD_BASE: u32 = 0xAA00_0000;

pub struct MissionCommandScript {}

impl MissionCommandScript {
    pub fn from_bytes(data: &[u8], name: &str) -> Result<Self> {
        let mut pe = peff::PortableExecutable::from_bytes(data)?;

        // Do default relocation to a high address. This makes offsets appear
        // 0-based and tags all local pointers with an obvious flag.
        pe.relocate(MC_LOAD_BASE)?;

        // let mut p = std::env::current_dir()?;
        // let mut p = p.parent().unwrap();
        // let mut p = p.parent().unwrap();
        // let mut p = p.parent().unwrap().to_owned();
        // p.push("__dump__");
        // p.push("mc");
        // p.push(name.replace(':', "_"));
        // let mut p = p.with_extension("asm").to_owned();
        // println!("pwd: {:?}", p);
        // let mut fp = File::create(p)?;
        // fp.write(&pe.code);

        // Seed external jumps with our implicit initial jump.
        let mut external_jumps = HashSet::new();
        external_jumps.insert(0);

        while !external_jumps.is_empty() {
            trace!(
                "top of loop at {:04X} with external jumps: {:?}",
                *offset,
                external_jumps
                    .iter()
                    .map(|n| format!("{:04X}", n))
                    .collect::<Vec<_>>()
            );

            // If we've found an external jump, that's good evidence that this is x86 code, so just
            // go ahead and decode that.
            if external_jumps.contains(offset) {
                external_jumps.remove(offset);
                trace!("ip reached external jump");

                let (bc, return_state) =
                    Self::disassemble_to_ret(&pe.code[*offset..], *offset, trampolines)?;
                trace!("decoded {} instructions", bc.instrs.len());

                Self::find_external_jumps(*offset, &bc, &mut external_jumps);
                trace!(
                    "new external jumps: {:?}",
                    external_jumps
                        .iter()
                        .map(|n| format!("{:04X}", n))
                        .collect::<Vec<_>>(),
                );
            }
        }

        // println!("ABOUT TO DISASM: {} : {}", name, bs2s(&pe.code[0..20]));
        // let bc = ByteCode::disassemble_until(0, &pe.code, |codes, rem| {
        //     let last = &codes[codes.len() - 1];
        //     (last.memonic == Memonic::Return && rem[0] == 0 && rem[1] == 0)
        //         || last.memonic == Memonic::Jump
        // })?;
        // println!("{}\n---------", name);
        // println!("{}", bc);
        // println!();

        Ok(Self {})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lib::Libs;
    use simplelog::{Config, LevelFilter, TermLogger};

    #[test]
    fn it_works() -> Result<()> {
        TermLogger::init(LevelFilter::Info, Config::default())?;

        let libs = Libs::for_testing()?;
        for (game, _palette, catalog) in libs.all() {
            for fid in catalog.find_with_extension("MC")? {
                let meta = catalog.stat(fid)?;
                // println!("At: {}:{:13} @ {}", game.test_dir, meta.name(), meta.path());

                if game.test_dir == "FA" && meta.name() == "U34.MC" {
                    let data = catalog.read(fid)?;
                    let _mc = MissionCommandScript::from_bytes(
                        data.as_ref(),
                        &format!("{}:{}", game.test_dir, meta.name()),
                    )?;
                }
            }
        }

        Ok(())
    }
}
