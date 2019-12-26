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

use ansi::{ansi, Color};
use std::mem;

pub fn n2h(n: u8) -> char {
    match n {
        0 => '0',
        1 => '1',
        2 => '2',
        3 => '3',
        4 => '4',
        5 => '5',
        6 => '6',
        7 => '7',
        8 => '8',
        9 => '9',
        10 => 'A',
        11 => 'B',
        12 => 'C',
        13 => 'D',
        14 => 'E',
        15 => 'F',
        _ => panic!("expected a nibble, got: {}", n),
    }
}

pub fn b2h(b: u8, v: &mut Vec<char>) {
    v.push(n2h(b >> 4));
    v.push(n2h(b & 0xF));
}

pub fn b2b(b: u8, v: &mut Vec<char>) {
    for i in 0..8 {
        if i == 4 {
            v.push('_');
        }
        let off = -(i - 7);
        if (b >> off) & 0b1 == 1 {
            v.push('1');
        } else {
            v.push('0');
        }
    }
}

pub fn bs2s(bs: &[u8]) -> String {
    let mut v = Vec::new();
    for &b in bs.iter() {
        b2h(b, &mut v);
        v.push(' ');
    }
    v.iter().collect::<String>()
}

pub fn bs_2_i16(bs: &[u8]) -> String {
    let mut s = String::new();
    let d: &[i16] = unsafe { mem::transmute(bs) };
    for i in 0..bs.len() / 2 {
        s += &format!("{:02X}{:02X}({}) ", bs[i * 2], bs[i * 2 + 1], d[i],);
    }
    s
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn p2s(bs: *const u8, start: usize, end: usize) -> String {
    let mut v = Vec::new();
    for i in start..end {
        b2h(unsafe { *bs.add(i) }, &mut v);
        v.push(' ');
    }
    v.iter().collect::<String>()
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn p_2_i16(bs: *const u8, start: usize, end: usize) -> String {
    let mut s = String::new();
    let b: &[u8] = unsafe { std::slice::from_raw_parts(bs.add(start), end - start) };
    let d: &[i16] = unsafe { mem::transmute(b) };
    for i in 0..(end - start) / 2 {
        s += &format!("{:02X}{:02X}({}) ", b[i * 2], b[i * 2 + 1], d[i],);
    }
    s
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ShowMode {
    AllOneLine,
    AllPerLine,
    UnknownFacet,
    UnknownMinus,
    Unknown,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagKind {
    RelocatedCall(String),
    RelocatedRef,
    RelocationTarget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub kind: TagKind,
    pub offset: usize,
    pub length: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub enum SectionKind {
    Main(u16),
    Unknown,
    Invalid,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Section {
    pub kind: SectionKind,
    pub offset: usize,
    pub length: usize,
}

impl Section {
    pub fn new(kind: u16, offset: usize, length: usize) -> Self {
        Section {
            kind: SectionKind::Main(kind),
            offset,
            length,
        }
    }

    pub fn unknown(offset: usize, length: usize) -> Self {
        Section {
            kind: SectionKind::Unknown,
            offset,
            length,
        }
    }

    pub fn color(&self) -> Color {
        match self.kind {
            SectionKind::Main(k) => match k {
                0xFFFF => Color::Blue,
                0x00F0 => Color::Green,
                0x00F2 => Color::Blue,
                0x00DA => Color::Magenta,
                0x00CA => Color::Blue,
                0x00B8 => Color::Blue,
                0x0042 => Color::Yellow,
                0x00E2 => Color::Yellow,
                0x007A => Color::Blue,
                0x00CE => Color::Magenta,
                0x0078 => Color::Blue,
                0x00C8 => Color::Magenta,
                0x00A6 => Color::Blue,
                0x00AC => Color::Magenta,
                0x0082 => Color::Green,
                0x1E1E => Color::Red,
                0x00FC => Color::Cyan,
                _ => Color::Red,
            },
            SectionKind::Unknown => Color::BrightBlack,
            _ => Color::Red,
        }
    }

    // pub fn show(&self) -> bool {
    //     return true;
    //     if let SectionKind::Unknown = self.kind {
    //         return true;
    //     }
    //     return false;
    // }
}

#[allow(clippy::cognitive_complexity)]
pub fn format_sections(
    code: &[u8],
    sections: &[Section],
    tags: &mut Vec<Tag>,
    mode: &ShowMode,
) -> Vec<String> {
    // Assert that sections tightly abut.
    // let mut next_offset = 0;
    // for section in sections {
    //     assert_eq!(section.offset, next_offset);
    //     next_offset = section.offset + section.length;
    // }

    // Assert that there are no tags overlapping.
    tags.sort_by(|a, b| a.offset.cmp(&b.offset));
    tags.dedup();
    for (i, tag_a) in tags.iter().enumerate() {
        for (j, tag_b) in tags.iter().enumerate() {
            if j > i {
                // println!("{:?}@{}+{}; {:?}@{}+{}", tag_a.kind, tag_a.offset, tag_a.length, tag_b.kind, tag_b.offset, tag_b.length);
                assert!(tag_a.offset <= tag_b.offset);
                assert!(
                    tag_a.offset + tag_a.length <= tag_b.offset
                        || tag_a.offset + tag_a.length >= tag_b.offset + tag_b.length
                );
            }
        }
    }

    let mut out = Vec::new();

    // Simple view of all sections concatenated.
    match mode {
        ShowMode::AllOneLine => {
            let mut line: Vec<char> = Vec::new();
            for section in sections {
                accumulate_section(code, section, tags, &mut line);
            }
            out.push(line.iter().collect::<String>());
        }
        ShowMode::AllPerLine => {
            for section in sections {
                let mut line: Vec<char> = Vec::new();
                accumulate_section(code, section, tags, &mut line);
                out.push(line.iter().collect::<String>());
            }
        }
        ShowMode::Unknown => {
            for section in sections {
                if let SectionKind::Unknown = section.kind {
                    let mut line: Vec<char> = Vec::new();
                    accumulate_section(code, section, tags, &mut line);
                    out.push(line.iter().collect::<String>());
                }
            }
        }
        ShowMode::UnknownMinus => {
            for (i, section) in sections.iter().enumerate() {
                if let SectionKind::Unknown = section.kind {
                    let mut line: Vec<char> = Vec::new();
                    accumulate_section(code, section, tags, &mut line);
                    if i > 2 {
                        accumulate_section(code, &sections[i - 3], tags, &mut line);
                    }
                    if i > 1 {
                        accumulate_section(code, &sections[i - 2], tags, &mut line);
                    }
                    if i > 0 {
                        accumulate_section(code, &sections[i - 1], tags, &mut line);
                    }
                    out.push(line.iter().collect::<String>());
                }
            }
        }
        ShowMode::UnknownFacet => {
            for section in sections {
                if let SectionKind::Unknown = section.kind {
                    if section.length > 0 && code[section.offset] == 0xFC {
                        let mut line: Vec<char> = Vec::new();
                        //accumulate_section(code, section, tags, &mut line);
                        accumulate_facet_section(code, section, &mut line);
                        out.push(line.iter().collect::<String>());
                    }
                }
            }
        }
        ShowMode::Custom => {
            // Grab sections that we care about and stuff them into lines.
            for (i, _section) in sections.iter().enumerate() {
                let mut line: Vec<char> = Vec::new();
                if i > 0 {
                    if let SectionKind::Main(k) = sections[i - 1].kind {
                        if k != 0xFC {
                            continue;
                        }
                        if let SectionKind::Unknown = sections[i].kind {
                            line.push('0');
                            line.push('|');
                            line.push(' ');
                            if k == 0xFC {
                                accumulate_facet_section(code, &sections[i - 1], &mut line);
                            } else {
                                accumulate_section(code, &sections[i - 1], tags, &mut line)
                            }
                            accumulate_section(code, &sections[i], tags, &mut line);
                            out.push(line.iter().collect::<String>());
                        } else {
                            line.push('1');
                            line.push('|');
                            line.push(' ');
                            if k == 0xFC {
                                accumulate_facet_section(code, &sections[i - 1], &mut line);
                            } else {
                                accumulate_section(code, &sections[i - 1], tags, &mut line)
                            }
                            out.push(line.iter().collect::<String>());
                        }
                    }
                }
            }
        }
    }

    out
}

pub fn accumulate_section(code: &[u8], section: &Section, tags: &[Tag], v: &mut Vec<char>) {
    if section.length == 0 {
        return;
    }
    if section.offset + section.length > code.len() {
        println!("OVERFLOW at section: {:?}", section);
        return;
    }

    let mut nul = Vec::new();
    let n = &mut nul;

    let section_tags = find_tags_in_section(section, tags);
    if let Some(t) = section_tags.first() {
        if t.offset == section.offset {
            ansi().underline().put(tgt(v, n));
        }
    }

    if section.length == 1 {
        ansi().bg(section.color()).put(tgt(v, n));
        b2h(code[section.offset], v);
        ansi().put(tgt(v, n));
        v.push(' ');
        return;
    }

    ansi().bg(section.color()).put(tgt(v, n));
    b2h(code[section.offset], v);
    v.push(' ');
    b2h(code[section.offset + 1], v);
    //    v.push('_');
    //    v.push('_');
    ansi().put(tgt(v, n));
    ansi().fg(section.color()).put(tgt(v, n));
    let mut off = section.offset + 2;
    for &b in &code[section.offset + 2..section.offset + section.length] {
        // Push any tag closers.
        for tag in section_tags.iter() {
            if tag.offset + tag.length == off {
                if let TagKind::RelocatedCall(ref target) = &tag.kind {
                    ansi().put(tgt(v, n));
                    v.push('(');
                    ansi().fg(Color::Red).put(tgt(v, n));
                    for c in target.chars() {
                        v.push(c)
                    }
                    ansi().put(tgt(v, n));
                    v.push(')');
                    v.push(' ');
                }
                ansi().put(tgt(v, n));
                ansi().fg(section.color()).put(tgt(v, n));
            }
        }
        v.push(' ');
        // Push any tag openers.
        for tag in section_tags.iter() {
            if tag.offset == off {
                match &tag.kind {
                    TagKind::RelocatedCall(_) => ansi().dimmed().put(tgt(v, n)),
                    TagKind::RelocatedRef => ansi().bg(Color::BrightRed).bold().put(tgt(v, n)),
                    TagKind::RelocationTarget => ansi()
                        .fg(Color::BrightMagenta)
                        .strike_through()
                        .put(tgt(v, n)),
                };
            }
        }
        b2h(b, v);
        off += 1;
    }
    ansi().put(tgt(v, n));
    v.push(' ');
}

const COLORIZE: bool = true;

fn tgt<'a>(x: &'a mut Vec<char>, y: &'a mut Vec<char>) -> &'a mut Vec<char> {
    if COLORIZE {
        return x;
    }
    y
}

fn accumulate_facet_section(code: &[u8], section: &Section, line: &mut Vec<char>) {
    if section.offset + section.length >= code.len() {
        println!("OVERFLOW at section: {:?}", section);
        return;
    }
    let mut nul = Vec::new();
    let n = &mut nul;

    ansi().bg(section.color()).put(tgt(line, n));
    b2h(code[section.offset], line);
    ansi().put(tgt(line, n));

    ansi().fg(section.color()).put(tgt(line, n));
    line.push(' ');
    b2b(code[section.offset + 1], line);
    line.push('_');
    b2b(code[section.offset + 2], line);

    for &b in &code[section.offset + 3..section.offset + section.length] {
        line.push(' ');
        b2h(b, line);
    }

    ansi().put(tgt(line, n));
    line.push(' ');
}

fn find_tags_in_section(section: &Section, tags: &[Tag]) -> Vec<Tag> {
    tags.iter()
        .filter(|t| t.offset >= section.offset && t.offset < section.offset + section.length)
        .map(std::borrow::ToOwned::to_owned)
        .collect::<Vec<Tag>>()
}

pub fn get_all_tags(pe: &peff::PE) -> Vec<Tag> {
    // println!("PE: vaddr:{:04X}", pe.code_vaddr);
    // for (key, thunk) in pe.thunks.clone().unwrap().iter() {
    //     println!(
    //         "THUNK> @{:04X}: {}: {:04X} -> {}",
    //         key, thunk.ordinal, thunk.vaddr, thunk.name
    //     );
    // }
    let mut tags = Vec::new();
    for &reloc in pe.relocs.iter() {
        assert!((reloc as usize) + 4 <= pe.code.len());
        // Look up the word we need to relocate in the binary.
        let dwords: &[u32] = unsafe { mem::transmute(&pe.code[reloc as usize..]) };
        let thunk_ptr = dwords[0];
        for thunk in pe.thunks.iter() {
            let thunk_addr = thunk.vaddr;
            if thunk_ptr == thunk_addr {
                // This relocation is for a pointer into the thunk table; store the name so
                // that we can print the name instead of the address.
                //println!("Relocating {:04X} in code to {}", thunk_ptr, &thunk.name);
                tags.push(Tag {
                    kind: TagKind::RelocatedCall(thunk.name.clone()),
                    offset: reloc as usize,
                    length: 4,
                });
            } else {
                // This relocation is to somewhere in code; mark both it and the target word
                // of the pointer that is stored at the reloc position.
                tags.push(Tag {
                    kind: TagKind::RelocatedRef,
                    offset: reloc as usize,
                    length: 4,
                });

                if thunk_ptr - pe.code_addr < pe.code.len() as u32 - 4 {
                    assert!(thunk_ptr > pe.code_addr, "thunked ptr before code");
                    assert!(
                        thunk_ptr <= pe.code_addr + pe.code.len() as u32 - 4,
                        "thunked ptr after code"
                    );
                    let code_offset = thunk_ptr - pe.code_addr;
                    // let value_to_relocate_arr: &[u16] =
                    //     unsafe { mem::transmute(&pe.code[code_offset as usize..]) };
                    // let value_to_relocate = value_to_relocate_arr[0];
                    // println!("Relocating {:X} at offset {:X}", value_to_relocate, code_offset);
                    tags.push(Tag {
                        kind: TagKind::RelocationTarget,
                        offset: code_offset as usize,
                        length: 2,
                    });
                }
            }
        }
    }
    tags
}
