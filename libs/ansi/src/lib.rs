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
    #[allow(dead_code)]
    Black = 30,
    #[allow(dead_code)]
    Red = 31,
    #[allow(dead_code)]
    Green = 32,
    #[allow(dead_code)]
    Yellow = 33,
    #[allow(dead_code)]
    Blue = 34,
    #[allow(dead_code)]
    Purple = 35,
    #[allow(dead_code)]
    Cyan = 36,
    #[allow(dead_code)]
    White = 37,
}

impl Color {
    fn encode_foreground(&self) -> u8 {
        return self.clone() as u8;
    }

    fn encode_background(self) -> u8 {
        self.encode_foreground() + 10
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Style {
    Bold = 1,
    Dimmed = 2,
    Italic = 3,
    Underline = 4,
    Blink = 5,
    Reverse = 7,
    Hidden = 8,
    StrikeThrough = 9,
}

impl Style {
    fn encode(&self) -> u8 {
        return self.clone() as u8;
    }
}

#[derive(Debug, PartialEq)]
pub struct Span {
    pub content: String,
    foreground: Option<Color>,
    background: Option<Color>,
    styles: HashSet<Style>,
}

impl Span {
    pub fn new(content: &str) -> Self {
        Span {
            content: content.to_owned(),
            foreground: None,
            background: None,
            styles: HashSet::new(),
        }
    }

    pub fn width(&self) -> usize {
        return self.content.chars().count();
    }

    pub fn foreground(mut self, clr: Color) -> Self {
        self.foreground = Some(clr);
        return self;
    }

    #[allow(dead_code)]
    pub fn background(mut self, clr: Color) -> Self {
        self.background = Some(clr);
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
    pub fn get_reset_style(escape_for_readline: bool) -> String {
        return Self::make_readline_safe("\x1B[0m", escape_for_readline);
    }

    pub fn format(&self) -> String {
        let style = self.format_style(false);
        return style + &self.content + &Self::get_reset_style(false);
    }

    pub fn format_style(&self, escape_for_readline: bool) -> String {
        if self.foreground.is_none() && self.background.is_none() && self.styles.len() == 0 {
            return "".to_owned();
        }
        let mut style = self.styles
            .iter()
            .map(|s| format!("{}", s.encode()))
            .collect::<Vec<String>>();
        style.append(&mut self.background
            .iter()
            .map(|c| format!("{}", c.encode_background()))
            .collect::<Vec<String>>());
        style.append(&mut self.foreground
            .iter()
            .map(|c| format!("{}", c.encode_foreground()))
            .collect::<Vec<String>>());
        return Self::make_readline_safe(&("\x1B[".to_owned() + &style.join(";") + "m"),
                                        escape_for_readline);
    }

    pub fn make_readline_safe(s: &str, escape_for_readline: bool) -> String {
        match escape_for_readline {
            true => "\\[".to_owned() + s + "\\]",
            false => s.to_owned(),
        }
    }
}
