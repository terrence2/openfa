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

extern crate bitflags;
extern crate failure;
extern crate peff;

use bitflags::bitflags;
use std::{fmt, mem};

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

#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn p2s(bs: *const u8, start: usize, end: usize) -> String {
    let mut v = Vec::new();
    for i in start..end {
        b2h(unsafe { *bs.add(i) }, &mut v);
        v.push(' ');
    }
    v.iter().collect::<String>()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Color {
    Black = 30,
    Red = 31,
    Green = 32,
    Yellow = 33,
    Blue = 34,
    Magenta = 35,
    Cyan = 36,
    White = 37,
    BrightBlack = 90,
    BrightRed = 91,
    BrightGreen = 92,
    BrightYellow = 93,
    BrightBlue = 94,
    BrightMagenta = 95,
    BrightCyan = 96,
    BrightWhite = 97,
}

impl Color {
    pub fn put(self, v: &mut Vec<char>) {
        for c in format!("{}", self as u8).chars() {
            v.push(c);
        }
    }
    pub fn put_bg(self, v: &mut Vec<char>) {
        for c in format!("{}", (self as u8) + 10).chars() {
            v.push(c);
        }
    }
    pub fn fmt(self) -> String {
        format!("{}", self as u8)
    }
    pub fn fmt_bg(self) -> String {
        format!("{}", (self as u8) + 10)
    }
}

bitflags! {
    struct StyleFlags: u8 {
        const BOLD          = 0b0000_0001;
        const DIMMED        = 0b0000_0010;
        const ITALIC        = 0b0000_0100;
        const UNDERLINE     = 0b0000_1000;
        const BLINK         = 0b0001_0000;
        const REVERSE       = 0b0010_0000;
        const HIDDEN        = 0b0100_0000;
        const STRIKETHROUGH = 0b1000_0000;
    }
}

impl StyleFlags {
    fn put(self, v: &mut Vec<char>) -> bool {
        let mut acc = Vec::new();
        if self.contains(StyleFlags::BOLD) {
            acc.push('1');
        }
        if self.contains(StyleFlags::DIMMED) {
            acc.push('2');
        }
        if self.contains(StyleFlags::ITALIC) {
            acc.push('3');
        }
        if self.contains(StyleFlags::UNDERLINE) {
            acc.push('4');
        }
        if self.contains(StyleFlags::BLINK) {
            acc.push('5');
        }
        if self.contains(StyleFlags::REVERSE) {
            acc.push('7');
        }
        if self.contains(StyleFlags::HIDDEN) {
            acc.push('8');
        }
        if self.contains(StyleFlags::STRIKETHROUGH) {
            acc.push('9');
        }
        if !acc.is_empty() {
            for (i, &c) in acc.iter().enumerate() {
                v.push(c);
                if i + 1 < acc.len() {
                    v.push(';');
                }
            }
        }
        !acc.is_empty()
    }
}

#[derive(Debug, PartialEq)]
pub struct Escape {
    foreground: Option<Color>,
    background: Option<Color>,
    styles: StyleFlags,
}

impl Escape {
    pub fn new() -> Self {
        Escape {
            foreground: None,
            background: None,
            styles: StyleFlags::empty(),
        }
    }

    pub fn fg(mut self, clr: Color) -> Self {
        self.foreground = Some(clr);
        self
    }

    pub fn bg(mut self, clr: Color) -> Self {
        self.background = Some(clr);
        self
    }

    #[allow(dead_code)]
    pub fn bold(mut self) -> Self {
        self.styles |= StyleFlags::BOLD;
        self
    }

    #[allow(dead_code)]
    pub fn dimmed(mut self) -> Self {
        self.styles |= StyleFlags::DIMMED;
        self
    }

    #[allow(dead_code)]
    pub fn italic(mut self) -> Self {
        self.styles |= StyleFlags::ITALIC;
        self
    }

    #[allow(dead_code)]
    pub fn underline(mut self) -> Self {
        self.styles |= StyleFlags::UNDERLINE;
        self
    }

    #[allow(dead_code)]
    pub fn blink(mut self) -> Self {
        self.styles |= StyleFlags::BLINK;
        self
    }

    #[allow(dead_code)]
    pub fn reverse(mut self) -> Self {
        self.styles |= StyleFlags::REVERSE;
        self
    }

    #[allow(dead_code)]
    pub fn hidden(mut self) -> Self {
        self.styles |= StyleFlags::HIDDEN;
        self
    }

    #[allow(dead_code)]
    pub fn strike_through(mut self) -> Self {
        self.styles |= StyleFlags::STRIKETHROUGH;
        self
    }

    #[allow(dead_code)]
    pub fn put_reset(v: &mut Vec<char>) {
        for c in "\x1B[0m".chars() {
            v.push(c);
        }
    }

    pub fn put(&self, v: &mut Vec<char>) {
        if self.foreground.is_none() && self.background.is_none() && self.styles.is_empty() {
            return Self::put_reset(v);
        }
        v.push('\x1B');
        v.push('[');
        let have_chars = self.styles.put(v);
        if let Some(c) = self.foreground {
            if have_chars {
                v.push(';');
            }
            c.put(v);
        }
        if let Some(c) = self.background {
            if have_chars {
                v.push(';');
            }
            c.put_bg(v);
        }
        v.push('m');
    }
}

impl Default for Escape {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for Escape {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut s = Vec::new();
        self.put(&mut s);
        write!(f, "{}", s.iter().collect::<String>())
    }
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

#[allow(clippy::cyclomatic_complexity)]
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
        ShowMode::AllPerLine => for section in sections {
            let mut line: Vec<char> = Vec::new();
            accumulate_section(code, section, tags, &mut line);
            out.push(line.iter().collect::<String>());
        },
        ShowMode::Unknown => for section in sections {
            if let SectionKind::Unknown = section.kind {
                let mut line: Vec<char> = Vec::new();
                accumulate_section(code, section, tags, &mut line);
                out.push(line.iter().collect::<String>());
            }
        },
        ShowMode::UnknownMinus => for (i, section) in sections.iter().enumerate() {
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
        },
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
            Escape::new().underline().put(tgt(v, n));
        }
    }

    if section.length == 1 {
        Escape::new().bg(section.color()).put(tgt(v, n));
        b2h(code[section.offset], v);
        Escape::new().put(tgt(v, n));
        v.push(' ');
        return;
    }

    Escape::new().bg(section.color()).put(tgt(v, n));
    b2h(code[section.offset], v);
    v.push(' ');
    b2h(code[section.offset + 1], v);
    //    v.push('_');
    //    v.push('_');
    Escape::new().put(tgt(v, n));
    Escape::new().fg(section.color()).put(tgt(v, n));
    let mut off = section.offset + 2;
    for &b in &code[section.offset + 2..section.offset + section.length] {
        // Push any tag closers.
        for tag in section_tags.iter() {
            if tag.offset + tag.length == off {
                if let TagKind::RelocatedCall(ref target) = &tag.kind {
                    Escape::new().put(tgt(v, n));
                    v.push('(');
                    Escape::new().fg(Color::Red).put(tgt(v, n));
                    for c in target.chars() {
                        v.push(c)
                    }
                    Escape::new().put(tgt(v, n));
                    v.push(')');
                    v.push(' ');
                }
                Escape::new().put(tgt(v, n));
                Escape::new().fg(section.color()).put(tgt(v, n));
            }
        }
        v.push(' ');
        // Push any tag openers.
        for tag in section_tags.iter() {
            if tag.offset == off {
                match &tag.kind {
                    TagKind::RelocatedCall(_) => Escape::new().dimmed().put(tgt(v, n)),
                    TagKind::RelocatedRef => {
                        Escape::new().bg(Color::BrightRed).bold().put(tgt(v, n))
                    }
                    TagKind::RelocationTarget => Escape::new()
                        .fg(Color::BrightMagenta)
                        .strike_through()
                        .put(tgt(v, n)),
                };
            }
        }
        b2h(b, v);
        off += 1;
    }
    Escape::new().put(tgt(v, n));
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

    Escape::new().bg(section.color()).put(tgt(line, n));
    b2h(code[section.offset], line);
    Escape::new().put(tgt(line, n));

    Escape::new().fg(section.color()).put(tgt(line, n));
    line.push(' ');
    b2b(code[section.offset + 1], line);
    line.push('_');
    b2b(code[section.offset + 2], line);

    for &b in &code[section.offset + 3..section.offset + section.length] {
        line.push(' ');
        b2h(b, line);
    }

    Escape::new().put(tgt(line, n));
    line.push(' ');
}

fn find_tags_in_section(section: &Section, tags: &[Tag]) -> Vec<Tag> {
    tags.iter()
        .filter(|t| t.offset >= section.offset && t.offset < section.offset + section.length)
        .map(|t| t.to_owned())
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

#[cfg(test)]
mod tests {
    use super::*;
    use failure::Fallible;

    #[test]
    fn style_flags() -> Fallible<()> {
        let mut style = StyleFlags::empty();
        style |= StyleFlags::BOLD;
        style |= StyleFlags::ITALIC;
        let mut acc = Vec::new();
        style.put(&mut acc);
        assert_eq!(acc, vec!['1', ';', '3']);
        Ok(())
    }
}
