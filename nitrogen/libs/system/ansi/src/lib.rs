// This file is part of Nitrogen.
//
// Nitrogen is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Nitrogen is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with Nitrogen.  If not, see <http://www.gnu.org/licenses/>.
use bitflags::bitflags;
use std::fmt;

#[cfg(target_family = "unix")]
use std::mem;

#[cfg(target_family = "unix")]
pub fn terminal_size() -> (u16, u16) {
    unsafe {
        if libc::isatty(libc::STDOUT_FILENO) != 1 {
            return (24, 80);
        }

        let mut winsize: libc::winsize = mem::zeroed();

        // FIXME: ".into()" used as a temporary fix for a libc bug
        // https://github.com/rust-lang/libc/pull/704
        #[allow(clippy::identity_conversion)]
        libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ.into(), &mut winsize);
        if winsize.ws_row > 0 && winsize.ws_col > 0 {
            (winsize.ws_row as u16, winsize.ws_col as u16)
        } else {
            (24, 80)
        }
    }
}

#[cfg(target_family = "windows")]
pub fn terminal_size() -> (u16, u16) {
    (80, 24)
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

    // Shortcuts for foreground colors
    pub fn black(self) -> Self {
        self.fg(Color::Black)
    }

    pub fn red(self) -> Self {
        self.fg(Color::Red)
    }

    pub fn green(self) -> Self {
        self.fg(Color::Green)
    }

    pub fn yellow(self) -> Self {
        self.fg(Color::Yellow)
    }

    pub fn blue(self) -> Self {
        self.fg(Color::Blue)
    }

    pub fn magenta(self) -> Self {
        self.fg(Color::Magenta)
    }

    pub fn cyan(self) -> Self {
        self.fg(Color::Cyan)
    }

    pub fn white(self) -> Self {
        self.fg(Color::White)
    }

    // Shortcut to upgrade a color to it's "bright" alternate.
    pub fn bright(mut self) -> Self {
        self.foreground = match self.foreground {
            None => None,
            Some(Color::Black) => Some(Color::BrightBlack),
            Some(Color::Red) => Some(Color::BrightRed),
            Some(Color::Green) => Some(Color::BrightGreen),
            Some(Color::Yellow) => Some(Color::BrightYellow),
            Some(Color::Blue) => Some(Color::BrightBlue),
            Some(Color::Magenta) => Some(Color::BrightMagenta),
            Some(Color::Cyan) => Some(Color::BrightCyan),
            Some(Color::White) => Some(Color::BrightWhite),
            Some(bright) => Some(bright),
        };
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

    pub fn apply(&self, msg: &str) -> String {
        format!("{}{}{}", self, msg, Self::new())
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

pub fn ansi() -> Escape {
    Escape::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_formatting() {
        println!(
            "{}Hello{}, {}World{}!",
            ansi().fg(Color::Green).bold(),
            ansi(),
            ansi().fg(Color::Blue).bold(),
            ansi()
        );
    }

    #[test]
    fn apply_formatting() {
        println!(
            "{}, {}!",
            ansi().green().apply("Hello"),
            ansi().blue().apply("World")
        );
    }

    #[test]
    fn apply_bright_formatting() {
        println!(
            "{}, {}!",
            ansi().green().bright().apply("Hello"),
            ansi().blue().bright().apply("World")
        );
    }

    #[test]
    fn apply_terminal_size() {
        let (h, w) = terminal_size();
        assert!(h > 0);
        assert!(w > 0);
    }

    #[test]
    fn style_flags() {
        let mut style = StyleFlags::empty();
        style |= StyleFlags::BOLD;
        style |= StyleFlags::ITALIC;
        let mut acc = Vec::new();
        style.put(&mut acc);
        assert_eq!(acc, vec!['1', ';', '3']);
    }
}
