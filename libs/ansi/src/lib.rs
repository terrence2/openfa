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
    BlackBG = 40,
    RedBG = 41,
    GreenBG = 42,
    YellowBG = 43,
    BlueBG = 44,
    MagentaBG = 45,
    CyanBG = 46,
    WhiteBG = 47,
    BrightBlackBG = 100,
    BrightRedBG = 101,
    BrightGreenBG = 102,
    BrightYellowBG = 103,
    BrightBlueBG = 104,
    BrightMagentaBG = 105,
    BrightCyanBG = 106,
    BrightWhiteBG = 107,
}

impl Color {
    fn put(&self, v: &mut Vec<char>) {
        for c in format!("{}", *self as isize).chars() {
            v.push(c);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Style {
    Bold = '1' as isize,
    Dimmed = '2' as isize,
    Italic = '3' as isize,
    Underline = '4' as isize,
    Blink = '5' as isize,
    Reverse = '7' as isize,
    Hidden = '8' as isize,
    StrikeThrough = '9' as isize,
}

impl Style {
    fn put(self, v: &mut Vec<char>) {
        v.push(self as u8 as char);
    }
}

#[derive(Debug, PartialEq)]
pub struct Escape {
    color: Option<Color>,
    styles: HashSet<Style>,
}

impl Escape {
    pub fn new() -> Self {
        Escape {
            color: None,
            styles: HashSet::new(),
        }
    }

    pub fn color(mut self, clr: Color) -> Self {
        self.color= Some(clr);
        return self;
    }

    #[allow(dead_code)]
    pub fn bold(mut self) -> Self {
        self.styles.insert(Style::Bold);
        return self;
    }

    #[allow(dead_code)]
    pub fn dimmed(mut self) -> Self {
        self.styles.insert(Style::Dimmed);
        return self;
    }

    #[allow(dead_code)]
    pub fn italic(mut self) -> Self {
        self.styles.insert(Style::Italic);
        return self;
    }

    #[allow(dead_code)]
    pub fn underline(mut self) -> Self {
        self.styles.insert(Style::Underline);
        return self;
    }

    #[allow(dead_code)]
    pub fn blink(mut self) -> Self {
        self.styles.insert(Style::Blink);
        return self;
    }

    #[allow(dead_code)]
    pub fn reverse(mut self) -> Self {
        self.styles.insert(Style::Reverse);
        return self;
    }

    #[allow(dead_code)]
    pub fn hidden(mut self) -> Self {
        self.styles.insert(Style::Hidden);
        return self;
    }

    #[allow(dead_code)]
    pub fn strike_through(mut self) -> Self {
        self.styles.insert(Style::StrikeThrough);
        return self;
    }

    #[allow(dead_code)]
    pub fn put_reset(v: &mut Vec<char>) {
        for c in "\x1B[0m".chars() {
            v.push(c);
        }
    }

    pub fn put(&self, v: &mut Vec<char>) {
        if self.color.is_none() && self.styles.len() == 0 {
            return;
        }
//        let mut style = self.styles
//            .iter()
//            .map(|s| format!("{}", s.encode()))
//            .collect::<Vec<String>>();
        v.push('\x1B');
        v.push('[');
        let mut have_chars = false;
        if let Some(c) = self.color {
            if have_chars {
                v.push(';');
            }
            c.put(v);
            have_chars = true;
        }
        v.push('m');
    }
}
