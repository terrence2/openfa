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
use failure::{bail, ensure, Fallible};
use packed_struct::packed_struct;
use peff::PE;
use reverse::bs2s;
use std::collections::{HashMap, HashSet};
use std::mem;

packed_struct!(PreloadHeader {
    _0 => function: u32,
    _1 => unk0: u16,
    _2 => unk1: u16,
    _3 => unk2: u16,
    _4 => unk3: u16,
    _5 => unk4: u16,
    _6 => flag_or_ascii: u8
});

packed_struct!(PreloadFooter {
    _1 => unk0: u16,
    _2 => unk1: u16,
    _3 => unk2: u16
});

#[derive(Debug, Eq, PartialEq)]
pub enum PreloadKind {
    ChoosePreload,
    GrafPrefPreload,
    Info2640Preload,
    Info640Preload,
    MultiPreload,
    SndPrefPreload,
    TestDiagPreload,
    TopCenterDialog,
}

impl PreloadKind {
    fn from_name(name: &str) -> Fallible<Self> {
        Ok(match name {
            "_ChoosePreload" => PreloadKind::ChoosePreload,
            "_GrafPrefPreload" => PreloadKind::GrafPrefPreload,
            "_Info2640Preload" => PreloadKind::Info2640Preload,
            "_Info640Preload" => PreloadKind::Info640Preload,
            "_MultiPreload" => PreloadKind::MultiPreload,
            "_SndPrefPreload" => PreloadKind::SndPrefPreload,
            "_TestDiagPreload" => PreloadKind::TestDiagPreload,
            "_TopCenterDialog" => PreloadKind::TopCenterDialog,
            _ => bail!("unknown preload kind: {}", name),
        })
    }
}

#[derive(Debug)]
pub struct Preload {
    kind: Option<PreloadKind>,
    unk_header_0: u16,
    unk_header_1: u16,
    unk_header_2: u16,
    unk_header_3: u16,
    unk_header_4: u16, // usually 0, but 0x100 in one case
    name: Option<String>,
    unk_footer_0: u16,
    unk_footer_1: u16,
    unk_footer_2: u16,
}

impl Preload {
    fn from_bytes(
        bytes: &[u8],
        offset: &mut usize,
        pe: &PE,
        trampolines: &HashMap<u32, String>,
    ) -> Fallible<Preload> {
        let header_ptr: *const PreloadHeader = bytes.as_ptr() as *const _;
        let header: &PreloadHeader = unsafe { &*header_ptr };
        ensure!(
            header.unk4() == 0 || header.unk4() == 0x0100,
            "expected 0 or 100 word in header"
        );

        let trampoline_target = header.function().saturating_sub(pe.code_vaddr);
        let kind = if trampolines.contains_key(&trampoline_target) {
            Some(PreloadKind::from_name(&trampolines[&trampoline_target])?)
        } else {
            None
        };

        let header_end_offset = *offset + mem::size_of::<PreloadHeader>();
        let (name, footer_offset) = match header.flag_or_ascii() {
            0 => (None, header_end_offset),
            0xFF => (None, header_end_offset + 1),
            _ => {
                let mut off = header_end_offset;
                let mut name = String::new();
                while pe.code[off] != 0 {
                    name.push(pe.code[off] as char);
                    off += 1;
                }
                (Some(name), off + 1)
            }
        };
        let footer_ptr: *const PreloadFooter = bytes.as_ptr() as *const _;
        let footer: &PreloadFooter = unsafe { &*footer_ptr };

        let end_offset = footer_offset + mem::size_of::<PreloadFooter>();
        *offset += end_offset - *offset;
        Ok(Self {
            kind,
            unk_header_0: header.unk0(),
            unk_header_1: header.unk1(),
            unk_header_2: header.unk2(),
            unk_header_3: header.unk3(),
            unk_header_4: header.unk4(),
            name,
            unk_footer_0: footer.unk0(),
            unk_footer_1: footer.unk1(),
            unk_footer_2: footer.unk2(),
        })
    }
}

packed_struct!(DrawActionHeader {
    _0 => function: u32,
    _1 => unk0: u16,
    _2 => unk1: u16,
    _4 => zeros0: [u8; 9],
    _5 => flag0: u8,
    _6 => unk2: u16,
    _7 => ptr_to_label: u32,
    _8 => zeros1: [u8; 8],
    _9 => unk3: u16,
    _10 => maybe_data: u32
});

#[derive(Debug)]
pub struct DrawAction {
    pub unk0: u16,
    pub unk1: u16,
    pub flag: u8,
    pub unk2: u16,
    pub label: String,
    pub unk3: u16,
}

impl DrawAction {
    fn from_bytes(
        bytes: &[u8],
        offset: &mut usize,
        pe: &PE,
        trampolines: &HashMap<u32, String>,
    ) -> Fallible<Self> {
        let header_ptr: *const DrawActionHeader = bytes.as_ptr() as *const _;
        let header: &DrawActionHeader = unsafe { &*header_ptr };
        ensure!(
            header.zeros0() == [0, 0, 0, 0, 0, 0, 0, 0, 0],
            "expected 9 zeros in draw action header"
        );
        ensure!(
            header.zeros1() == [0, 0, 0, 0, 0, 0, 0, 0],
            "expected 8 zeros in draw action header"
        );

        let label_ptr = header
            .ptr_to_label()
            .saturating_sub(pe.code_vaddr)
            .saturating_sub(pe.image_base);
        let label = if trampolines.contains_key(&label_ptr) {
            trampolines[&label_ptr].clone()
        } else {
            let mut label = String::new();
            let mut off = label_ptr as usize;
            while pe.code[off] != 0 {
                label.push(pe.code[off] as char);
                off += 1;
            }
            label
        };
        *offset += mem::size_of::<DrawActionHeader>();

        Ok(DrawAction {
            unk0: header.unk0(),
            unk1: header.unk1(),
            flag: header.flag0(),
            unk2: header.unk2(),
            label,
            unk3: header.unk3(),
        })
    }
}

// 2C 11 00 00 [_DrawRocker] 00 00 00 00 00 00 00 00 12 00 10 00 00 00 00 00 00 00 00 00 00 00 00 00
// 5C 10 00 00  00 09 00 00 00 00 00
packed_struct!(DrawRockerHeader {
    _0 => function: u32,
    _01 => unk0: u16,
    _02 => unk1: u16,
    _1 => zeros0: [u8; 4],
    _2 => unk2: u16,
    _3 => unk3: u16,
    _4 => zeros1: [u8; 12],
    _5 => ptr_to_label: u32,
    _6 => unk4: u16,
    _7 => zeros2: [u8; 5]
});

#[derive(Debug)]
pub struct DrawRocker {
    unk0: u16,
    unk1: u16,
    unk2: u16,
    unk3: u16,
    unk4: u16,
    label: String,
}

impl DrawRocker {
    fn from_bytes(
        bytes: &[u8],
        offset: &mut usize,
        _pe: &PE,
        _trampolines: &HashMap<u32, String>,
    ) -> Fallible<Self> {
        let header_ptr: *const DrawRockerHeader = bytes.as_ptr() as *const _;
        let header: &DrawRockerHeader = unsafe { &*header_ptr };
        ensure!(
            header.zeros0() == [0, 0, 0, 0],
            "expected 4 zeros in draw action header"
        );
        ensure!(
            header.zeros1() == [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            "expected 12 zeros in draw action header"
        );
        ensure!(
            header.zeros2() == [0, 0, 0, 0, 0],
            "expected 5 zeros in draw action header"
        );
        *offset += mem::size_of::<DrawRockerHeader>();

        Ok(DrawRocker {
            unk0: header.unk0(),
            unk1: header.unk1(),
            unk2: header.unk2(),
            unk3: header.unk3(),
            unk4: header.unk4(),
            label: String::new(),
        })
    }
}

#[derive(Debug)]
pub enum Widget {
    Preload(Preload),
    Action(DrawAction),
    Rocker(DrawRocker),
}

pub struct Dialog {
    pub widgets: Vec<Widget>,
}

impl Dialog {
    pub fn from_bytes(bytes: &[u8]) -> Fallible<Self> {
        let pe = PE::from_bytes(bytes)?;
        if pe.code.is_empty() {
            return Ok(Self {
                widgets: Vec::new(),
            });
        }

        let mut offset = 0;
        let trampolines = Self::find_trampolines(&pe)?;
        let targets = Self::find_targets(&pe, &trampolines)?;

        let preload = Preload::from_bytes(&pe.code, &mut offset, &pe, &trampolines)?;
        let mut widgets = Vec::new();
        widgets.push(Widget::Preload(preload));

        loop {
            let code = &pe.code[offset..];
            let dwords: &[u32] = unsafe { mem::transmute(code) };
            if dwords[0] == 0 || dwords[0] == 0x0203_0201 {
                break;
            }
            let ptr = dwords[0]
                .saturating_sub(pe.code_vaddr)
                .saturating_sub(pe.image_base);
            ensure!(
                trampolines.contains_key(&ptr),
                "expected a pointer in first dword"
            );
            // if !trampolines.contains_key(&ptr) {
            //     break;
            // }

            match trampolines[&ptr].as_ref() {
                "_DrawAction" => {
                    let action = DrawAction::from_bytes(code, &mut offset, &pe, &trampolines)?;
                    widgets.push(Widget::Action(action));
                }
                "_DrawRocker" => {
                    let rocker = DrawRocker::from_bytes(code, &mut offset, &pe, &trampolines)?;
                    widgets.push(Widget::Rocker(rocker));
                }
                _ => {
                    println!("skipping: {}", trampolines[&ptr]);
                    break;
                }
            }

            // If the last dword of the prior instruction is a target, then we reached the end sometimes.
            if targets.contains(&(offset - 4)) {
                println!("Stopping because targets contains our offset (less 4)");
                break;
            }
        }
        Ok(Self { widgets })
    }

    fn find_trampolines(pe: &PE) -> Fallible<HashMap<u32, String>> {
        ensure!(pe.code.len() >= 6, "PE too short for trampolines");
        let mut tramps = HashMap::new();
        let mut tramp_offset = pe.code.len() - 6;
        loop {
            if pe.code[tramp_offset] == 0xFF && pe.code[tramp_offset + 1] == 0x25 {
                let dwords: *const u32 =
                    unsafe { mem::transmute(pe.code[tramp_offset + 2..].as_ptr() as *const u8) };
                let tgt = unsafe { *dwords };
                let mut found = false;
                for thunk in &pe.thunks {
                    if thunk.vaddr == tgt.saturating_sub(pe.image_base) {
                        found = true;
                        tramps.insert(tramp_offset as u32, thunk.name.clone());
                        break;
                    }
                }
                assert!(found, "no matching thunk");
                tramp_offset -= 6;
            } else {
                break;
            }
        }
        Ok(tramps)
    }

    fn find_targets(pe: &PE, trampolines: &HashMap<u32, String>) -> Fallible<HashSet<usize>> {
        let mut targets = HashSet::new();
        for reloc in &pe.relocs {
            let r = *reloc as usize;
            let dwords: &[u32] = unsafe { mem::transmute(&pe.code[r..]) };
            let ptr = dwords[0]
                .saturating_sub(pe.code_vaddr)
                .saturating_sub(pe.image_base);
            if trampolines.contains_key(&ptr) {
                continue;
            }
            targets.insert(ptr as usize);
        }
        Ok(targets)
    }

    #[allow(clippy::many_single_char_names)]
    #[allow(clippy::if_same_then_else)]
    pub fn explore(name: &str, bytes: &[u8]) -> Fallible<()> {
        let pe = PE::from_bytes(bytes)?;
        if pe.code.is_empty() {
            return Ok(());
        }

        let vaddr = pe.code_vaddr;

        //println!("=== {} ======", name);

        let mut all_thunk_descrs = Vec::new();
        for thunk in &pe.thunks {
            all_thunk_descrs.push(format!("{}:{:04X}", thunk.name, thunk.vaddr));
        }

        let tramps = Self::find_trampolines(&pe)?;

        let mut relocs = HashSet::new();
        let mut targets = HashSet::new();
        let mut target_names = HashMap::new();
        for reloc in &pe.relocs {
            let r = *reloc as usize;
            relocs.insert((r, 0));
            relocs.insert((r + 1, 1));
            relocs.insert((r + 2, 2));
            relocs.insert((r + 3, 3));
            let a = u32::from(pe.code[r]);
            let b = u32::from(pe.code[r + 1]);
            let c = u32::from(pe.code[r + 2]);
            let d = u32::from(pe.code[r + 3]);
            //println!("a: {:02X} {:02X} {:02X} {:02X}", d, c, b, a);
            let vtgt = (d << 24) + (c << 16) + (b << 8) + a;
            let tgt = vtgt - vaddr;
            // println!(
            //     "tgt:{:04X} => {:04X} <> {}",
            //     tgt,
            //     vtgt,
            //     all_thunk_descrs.join(", ")
            // );
            for thunk in &pe.thunks {
                if vtgt == thunk.vaddr {
                    target_names.insert(r + 3, thunk.name.to_owned());
                    break;
                }
            }
            for (tramp_off, thunk_name) in &tramps {
                //println!("AT:{:04X} ?= {:04X}", *tramp_off, tgt);
                if tgt == *tramp_off {
                    target_names.insert(r + 3, thunk_name.to_owned());
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
            let b = bs2s(&pe.code[offset..=offset]);
            if relocs.contains(&(offset, 0)) {
                out += &format!("\n {}{}{}", ansi().green(), &b, ansi());
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
                    out += &format!("{}{}{} ", ansi().green(), &b, ansi());
                }
            } else if targets.contains(&(offset as u32)) {
                out += &format!("{}{}{}", ansi().red(), &b, ansi());
            //} else if offset == 0 {
            //    out += &format!("0000: ");
            } else {
                out += &b;
            }
            offset += 1;
        }

        println!("{} - {}", out, name);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omnilib::OmniLib;

    #[test]
    fn it_can_load_all_dialogs() -> Fallible<()> {
        //let omni = OmniLib::new_for_test_in_games(&["ATF"])?;
        let omni = OmniLib::new_for_test()?;
        for (game, name) in omni.find_matching("*.DLG")?.iter() {
            println!("AT: {}:{}", game, name);

            //let palette = Palette::from_bytes(&omni.library(&game).load("PALETTE.PAL")?)?;
            //let img = decode_pic(&palette, &omni.library(&game).load(&name)?)?;

            Dialog::explore(&name, &omni.library(&game).load(&name)?)?;
            let _dlg = Dialog::from_bytes(&omni.library(&game).load(&name)?)?;
        }

        Ok(())
    }
}
