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
#[macro_use]
extern crate bitflags;

use std::collections::HashSet;

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
    pub fn put(&self, v: &mut Vec<char>) {
        for c in format!("{}", *self as u8).chars() {
            v.push(c);
        }
    }
    pub fn put_bg(&self, v: &mut Vec<char>) {
        for c in format!("{}", (*self as u8) + 10).chars() {
            v.push(c);
        }
    }
}

bitflags! {
    struct StyleFlags: u8 {
        const BOLD          = 0b00000001;
        const DIMMED        = 0b00000010;
        const ITALIC        = 0b00000100;
        const UNDERLINE     = 0b00001000;
        const BLINK         = 0b00010000;
        const REVERSE       = 0b00100000;
        const HIDDEN        = 0b01000000;
        const STRIKETHROUGH = 0b10000000;
    }
}

impl StyleFlags {
    fn put(&self, v: &mut Vec<char>) -> bool {
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
        if acc.len() > 0 {
            for (i, &c) in acc.iter().enumerate() {
                v.push(c);
                if i + 1 < acc.len() {
                    v.push(';');
                }
            }
        }
        return acc.len() > 0;
    }
}

#[derive(Debug, PartialEq)]
pub struct Escape {
    foreground: Option<Color>,
    background: Option<Color>,
    styles: StyleFlags
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
        return self;
    }

    pub fn bg(mut self, clr: Color) -> Self {
        self.background = Some(clr);
        return self;
    }

    #[allow(dead_code)]
    pub fn bold(mut self) -> Self {
        self.styles |= StyleFlags::BOLD;
        return self;
    }

    #[allow(dead_code)]
    pub fn dimmed(mut self) -> Self {
        self.styles |= StyleFlags::DIMMED;
        return self;
    }

    #[allow(dead_code)]
    pub fn italic(mut self) -> Self {
        self.styles |= StyleFlags::ITALIC;
        return self;
    }

    #[allow(dead_code)]
    pub fn underline(mut self) -> Self {
        self.styles |= StyleFlags::UNDERLINE;
        return self;
    }

    #[allow(dead_code)]
    pub fn blink(mut self) -> Self {
        self.styles |= StyleFlags::BLINK;
        return self;
    }

    #[allow(dead_code)]
    pub fn reverse(mut self) -> Self {
        self.styles |= StyleFlags::REVERSE;
        return self;
    }

    #[allow(dead_code)]
    pub fn hidden(mut self) -> Self {
        self.styles |= StyleFlags::HIDDEN;
        return self;
    }

    #[allow(dead_code)]
    pub fn strike_through(mut self) -> Self {
        self.styles |= StyleFlags::STRIKETHROUGH;
        return self;
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
        let mut have_chars = self.styles.put(v);
        if let Some(c) = self.foreground {
            if have_chars {
                v.push(';');
            }
            c.put(v);
            have_chars = true;
        }
        if let Some(c) = self.background {
            if have_chars {
                v.push(';');
            }
            c.put_bg(v);
            have_chars = true;
        }
        v.push('m');
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_flags() {
        let mut style = StyleFlags::empty();
        style |= StyleFlags::BOLD;
        style |= StyleFlags::ITALIC;
        let mut acc = Vec::new();
        style.put(acc);
        assert_eq!(acc, vec!['1', ';', '3']);
    }
}
