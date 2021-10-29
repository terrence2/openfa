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
#![allow(clippy::transmute_ptr_to_ptr)]

use ansi::ansi;
use anyhow::Result;
use peff::PortableExecutable;
use reverse::bs2s;
use std::collections::{HashMap, HashSet};
use std::mem;

pub struct Menu {}

impl Menu {
    pub fn from_bytes(name: &str, bytes: &[u8]) -> Result<Self> {
        let pe = PortableExecutable::from_bytes(bytes)?;

        if !pe.section_info.contains_key("CODE") {
            return Ok(Self {});
        }

        let vaddr = pe.section_info["CODE"].virtual_address as usize;

        let mut all_thunk_descrs = Vec::new();
        for thunk in &pe.thunks {
            all_thunk_descrs.push(format!("{}:{:04X}", thunk.name, thunk.vaddr));
        }

        let mut thunks = HashMap::new();
        let mut thunk_offset = pe.code.len() - 6;
        loop {
            if pe.code[thunk_offset] == 0xFF && pe.code[thunk_offset + 1] == 0x25 {
                let dwords: *const u32 =
                    unsafe { mem::transmute(pe.code[thunk_offset + 2..].as_ptr() as *const u8) };
                let tgt = unsafe { *dwords };
                let mut found = false;
                for thunk in &pe.thunks {
                    if thunk.vaddr == tgt {
                        found = true;
                        thunks.insert(thunk_offset, thunk.clone());
                        break;
                    }
                }
                assert!(found, "no matching thunk");
                thunk_offset -= 6;
            } else {
                break;
            }
        }

        let mut relocs = HashSet::new();
        let mut targets = HashSet::new();
        let mut target_names = HashMap::new();
        for reloc_ptr in &pe.relocs {
            let reloc = *reloc_ptr as usize;
            relocs.insert((reloc, 0));
            relocs.insert((reloc + 1, 1));
            relocs.insert((reloc + 2, 2));
            relocs.insert((reloc + 3, 3));
            let tgt = [
                pe.code[reloc] as usize,
                pe.code[reloc + 1] as usize,
                pe.code[reloc + 2] as usize,
                pe.code[reloc + 3] as usize,
            ];
            let vtgt = (tgt[3] << 24) | (tgt[2] << 16) | (tgt[1] << 8) | tgt[0];
            let tgt = vtgt - vaddr;
            println!(
                "tgt:{:04X} => {:04X} <> {}",
                tgt,
                vtgt,
                all_thunk_descrs.join(", ")
            );
            for thunk in &pe.thunks {
                if vtgt == thunk.vaddr as usize {
                    target_names.insert(reloc + 3, thunk.name.to_owned());
                    break;
                }
            }
            for (thunk_off, thunk) in &thunks {
                println!("AT:{:04X} ?= {:04X}", *thunk_off, tgt);
                if tgt == *thunk_off {
                    target_names.insert(reloc + 3, thunk.name.to_owned());
                    break;
                }
            }
            //assert!(tgt <= pe.code.len());
            targets.insert(tgt);
            targets.insert(tgt + 1);
            targets.insert(tgt + 2);
            targets.insert(tgt + 3);
        }

        let mut out = String::new();
        let mut offset = 0;
        while offset < pe.code.len() {
            let b = bs2s(&pe.code[offset..offset + 1]);
            if relocs.contains(&(offset, 0)) && targets.contains(&offset) {
                out += &format!("\n{:04X}: {}{}{}", offset, ansi().magenta(), &b, ansi());
            } else if (relocs.contains(&(offset, 1))
                || relocs.contains(&(offset, 2))
                || relocs.contains(&(offset, 3)))
                && targets.contains(&offset)
            {
                out += &format!("{}{}{}", ansi().magenta(), &b, ansi());
            } else if relocs.contains(&(offset, 0)) {
                out += &format!("\n{:04X}: {}{}{}", offset, ansi().green(), &b, ansi());
            } else if relocs.contains(&(offset, 1)) {
                out += &format!("{}{}{}", ansi().green(), &b, ansi());
            } else if relocs.contains(&(offset, 2)) {
                out += &format!("{}{}{}", ansi().cyan(), &b, ansi());
            } else if relocs.contains(&(offset, 3)) {
                if target_names.contains_key(&offset) {
                    out += &format!(
                        "{}{}{}[{}] ",
                        ansi().green(),
                        &b,
                        ansi(),
                        target_names[&offset]
                    );
                } else {
                    out += &format!("{}{}{}", ansi().green(), &b, ansi());
                }
            } else if targets.contains(&offset) {
                out += &format!("{}{}{}", ansi().red(), &b, ansi());
            } else {
                out += &b;
            }
            offset += 1;
        }

        println!("{} - {}", out, name);

        Ok(Menu {})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lib::CatalogManager;

    #[test]
    fn it_can_load_all_menus() -> Result<()> {
        let catalogs = CatalogManager::for_testing()?;
        for (game, catalog) in catalogs.all() {
            for fid in catalog.find_with_extension("MNU")? {
                let meta = catalog.stat_sync(fid)?;
                println!(
                    "At: {}:{:13} @ {}",
                    game.test_dir,
                    meta.name(),
                    meta.path()
                        .map(|v| v.to_string_lossy())
                        .unwrap_or_else(|| "<none>".into())
                );
                let _mnu = Menu::from_bytes(
                    &format!("{}:{}", game.test_dir, meta.name()),
                    &catalog.read_sync(fid)?,
                )?;
            }
        }

        Ok(())
    }
}
