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
use ansi::ansi;
use failure::Fallible;
use peff::PE;
use reverse::bs2s;
use std::collections::{HashMap, HashSet};
use std::mem;

pub struct Menu {}

impl Menu {
    pub fn from_bytes(name: &str, bytes: &[u8]) -> Fallible<Self> {
        let pe = PE::from_bytes(bytes)?;

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
        for reloc in &pe.relocs {
            let r = *reloc as usize;
            relocs.insert((r, 0));
            relocs.insert((r + 1, 1));
            relocs.insert((r + 2, 2));
            relocs.insert((r + 3, 3));
            let a = pe.code[r] as usize;
            let b = pe.code[r + 1] as usize;
            let c = pe.code[r + 2] as usize;
            let d = pe.code[r + 3] as usize;
            //println!("a: {:02X} {:02X} {:02X} {:02X}", d, c, b, a);
            let vtgt = (d << 24) + (c << 16) + (b << 8) + a;
            let tgt = vtgt - vaddr;
            println!(
                "tgt:{:04X} => {:04X} <> {}",
                tgt,
                vtgt,
                all_thunk_descrs.join(", ")
            );
            for thunk in &pe.thunks {
                if vtgt == thunk.vaddr as usize {
                    target_names.insert(r + 3, thunk.name.to_owned());
                    break;
                }
            }
            for (thunk_off, thunk) in &thunks {
                println!("AT:{:04X} ?= {:04X}", *thunk_off, tgt);
                if tgt == *thunk_off {
                    target_names.insert(r + 3, thunk.name.to_owned());
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
                out += &format!("{}{}{}", ansi().green(), &b, ansi());
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
    use omnilib::OmniLib;

    #[test]
    fn it_can_load_all_menus() -> Fallible<()> {
        let omni = OmniLib::new_for_test_in_games(&["FA"])?;
        for (game, name) in omni.find_matching("*.MNU")? {
            //println!("AT: {}:{}", game, name);

            //let palette = Palette::from_bytes(&omni.library(&game).load("PALETTE.PAL")?)?;
            //let img = decode_pic(&palette, &omni.library(&game).load(&name)?)?;

            let _mnu = Menu::from_bytes(
                &format!("{}:{}", game, name),
                &omni.library(&game).load(&name)?,
            )?;
        }

        Ok(())
    }
}
