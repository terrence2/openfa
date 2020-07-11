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
use failure::{bail, ensure, Fallible};
use lazy_static::lazy_static;
use log::warn;
use smallvec::SmallVec;
use std::collections::{HashMap, HashSet};
use unicase::{eq_ascii, Ascii};
use winit::event::{ButtonId, ScanCode, VirtualKeyCode};

// When providing keys via a typed in command ala `bind +moveleft a`, we are
// talking about a virtual key name. When we poke a key in order to set a bind
// in the gui, we want to capture the actual scancode, because we have no idea
// what's painted on the front of the keycap.
#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub enum Key {
    Physical(ScanCode),
    Virtual(VirtualKeyCode),
    MouseButton(ButtonId),
}

lazy_static! {
    static ref MIRROR_MODIFIERS: HashSet<Ascii<&'static str>> = {
        let mut s = HashSet::new();
        s.insert(Ascii::new("Control"));
        s.insert(Ascii::new("Alt"));
        s.insert(Ascii::new("Win"));
        s.insert(Ascii::new("Shift"));
        s
    };

    #[rustfmt::skip]
    static ref KEYCODES: HashMap<Ascii<&'static str>, Key> = {
        let mut m = HashMap::new();
        m.insert(Ascii::new("A"), Key::Virtual(VirtualKeyCode::A));
        m.insert(Ascii::new("B"), Key::Virtual(VirtualKeyCode::B));
        m.insert(Ascii::new("C"), Key::Virtual(VirtualKeyCode::C));
        m.insert(Ascii::new("D"), Key::Virtual(VirtualKeyCode::D));
        m.insert(Ascii::new("E"), Key::Virtual(VirtualKeyCode::E));
        m.insert(Ascii::new("F"), Key::Virtual(VirtualKeyCode::F));
        m.insert(Ascii::new("G"), Key::Virtual(VirtualKeyCode::G));
        m.insert(Ascii::new("H"), Key::Virtual(VirtualKeyCode::H));
        m.insert(Ascii::new("I"), Key::Virtual(VirtualKeyCode::I));
        m.insert(Ascii::new("J"), Key::Virtual(VirtualKeyCode::J));
        m.insert(Ascii::new("K"), Key::Virtual(VirtualKeyCode::K));
        m.insert(Ascii::new("L"), Key::Virtual(VirtualKeyCode::L));
        m.insert(Ascii::new("M"), Key::Virtual(VirtualKeyCode::M));
        m.insert(Ascii::new("N"), Key::Virtual(VirtualKeyCode::N));
        m.insert(Ascii::new("O"), Key::Virtual(VirtualKeyCode::O));
        m.insert(Ascii::new("P"), Key::Virtual(VirtualKeyCode::P));
        m.insert(Ascii::new("Q"), Key::Virtual(VirtualKeyCode::Q));
        m.insert(Ascii::new("R"), Key::Virtual(VirtualKeyCode::R));
        m.insert(Ascii::new("S"), Key::Virtual(VirtualKeyCode::S));
        m.insert(Ascii::new("T"), Key::Virtual(VirtualKeyCode::T));
        m.insert(Ascii::new("U"), Key::Virtual(VirtualKeyCode::U));
        m.insert(Ascii::new("V"), Key::Virtual(VirtualKeyCode::V));
        m.insert(Ascii::new("W"), Key::Virtual(VirtualKeyCode::W));
        m.insert(Ascii::new("X"), Key::Virtual(VirtualKeyCode::X));
        m.insert(Ascii::new("Y"), Key::Virtual(VirtualKeyCode::Y));
        m.insert(Ascii::new("Z"), Key::Virtual(VirtualKeyCode::Z));
        m.insert(Ascii::new("Key1"), Key::Virtual(VirtualKeyCode::Key1));
        m.insert(Ascii::new("Key2"), Key::Virtual(VirtualKeyCode::Key2));
        m.insert(Ascii::new("Key3"), Key::Virtual(VirtualKeyCode::Key3));
        m.insert(Ascii::new("Key4"), Key::Virtual(VirtualKeyCode::Key4));
        m.insert(Ascii::new("Key5"), Key::Virtual(VirtualKeyCode::Key5));
        m.insert(Ascii::new("Key6"), Key::Virtual(VirtualKeyCode::Key6));
        m.insert(Ascii::new("Key7"), Key::Virtual(VirtualKeyCode::Key7));
        m.insert(Ascii::new("Key8"), Key::Virtual(VirtualKeyCode::Key8));
        m.insert(Ascii::new("Key9"), Key::Virtual(VirtualKeyCode::Key9));
        m.insert(Ascii::new("Key0"), Key::Virtual(VirtualKeyCode::Key0));
        m.insert(Ascii::new("Escape"), Key::Virtual(VirtualKeyCode::Escape));
        m.insert(Ascii::new("F1"), Key::Virtual(VirtualKeyCode::F1));
        m.insert(Ascii::new("F2"), Key::Virtual(VirtualKeyCode::F2));
        m.insert(Ascii::new("F3"), Key::Virtual(VirtualKeyCode::F3));
        m.insert(Ascii::new("F4"), Key::Virtual(VirtualKeyCode::F4));
        m.insert(Ascii::new("F5"), Key::Virtual(VirtualKeyCode::F5));
        m.insert(Ascii::new("F6"), Key::Virtual(VirtualKeyCode::F6));
        m.insert(Ascii::new("F7"), Key::Virtual(VirtualKeyCode::F7));
        m.insert(Ascii::new("F8"), Key::Virtual(VirtualKeyCode::F8));
        m.insert(Ascii::new("F9"), Key::Virtual(VirtualKeyCode::F9));
        m.insert(Ascii::new("F10"), Key::Virtual(VirtualKeyCode::F10));
        m.insert(Ascii::new("F11"), Key::Virtual(VirtualKeyCode::F11));
        m.insert(Ascii::new("F12"), Key::Virtual(VirtualKeyCode::F12));
        m.insert(Ascii::new("F13"), Key::Virtual(VirtualKeyCode::F13));
        m.insert(Ascii::new("F14"), Key::Virtual(VirtualKeyCode::F14));
        m.insert(Ascii::new("F15"), Key::Virtual(VirtualKeyCode::F15));
        m.insert(Ascii::new("F16"), Key::Virtual(VirtualKeyCode::F16));
        m.insert(Ascii::new("F17"), Key::Virtual(VirtualKeyCode::F17));
        m.insert(Ascii::new("F18"), Key::Virtual(VirtualKeyCode::F18));
        m.insert(Ascii::new("F19"), Key::Virtual(VirtualKeyCode::F19));
        m.insert(Ascii::new("F20"), Key::Virtual(VirtualKeyCode::F20));
        m.insert(Ascii::new("F21"), Key::Virtual(VirtualKeyCode::F21));
        m.insert(Ascii::new("F22"), Key::Virtual(VirtualKeyCode::F22));
        m.insert(Ascii::new("F23"), Key::Virtual(VirtualKeyCode::F23));
        m.insert(Ascii::new("F24"), Key::Virtual(VirtualKeyCode::F24));
        m.insert(Ascii::new("Snapshot"), Key::Virtual(VirtualKeyCode::Snapshot));
        m.insert(Ascii::new("Scroll"), Key::Virtual(VirtualKeyCode::Scroll));
        m.insert(Ascii::new("Pause"), Key::Virtual(VirtualKeyCode::Pause));
        m.insert(Ascii::new("Insert"), Key::Virtual(VirtualKeyCode::Insert));
        m.insert(Ascii::new("Home"), Key::Virtual(VirtualKeyCode::Home));
        m.insert(Ascii::new("Delete"), Key::Virtual(VirtualKeyCode::Delete));
        m.insert(Ascii::new("End"), Key::Virtual(VirtualKeyCode::End));
        m.insert(Ascii::new("PageDown"), Key::Virtual(VirtualKeyCode::PageDown));
        m.insert(Ascii::new("PageUp"), Key::Virtual(VirtualKeyCode::PageUp));
        m.insert(Ascii::new("Left"), Key::Virtual(VirtualKeyCode::Left));
        m.insert(Ascii::new("Up"), Key::Virtual(VirtualKeyCode::Up));
        m.insert(Ascii::new("Right"), Key::Virtual(VirtualKeyCode::Right));
        m.insert(Ascii::new("Down"), Key::Virtual(VirtualKeyCode::Down));
        m.insert(Ascii::new("Back"), Key::Virtual(VirtualKeyCode::Back));
        m.insert(Ascii::new("Return"), Key::Virtual(VirtualKeyCode::Return));
        m.insert(Ascii::new("Space"), Key::Virtual(VirtualKeyCode::Space));
        m.insert(Ascii::new("Compose"), Key::Virtual(VirtualKeyCode::Compose));
        m.insert(Ascii::new("Caret"), Key::Virtual(VirtualKeyCode::Caret));
        m.insert(Ascii::new("Numlock"), Key::Virtual(VirtualKeyCode::Numlock));
        m.insert(Ascii::new("Numpad0"), Key::Virtual(VirtualKeyCode::Numpad0));
        m.insert(Ascii::new("Numpad1"), Key::Virtual(VirtualKeyCode::Numpad1));
        m.insert(Ascii::new("Numpad2"), Key::Virtual(VirtualKeyCode::Numpad2));
        m.insert(Ascii::new("Numpad3"), Key::Virtual(VirtualKeyCode::Numpad3));
        m.insert(Ascii::new("Numpad4"), Key::Virtual(VirtualKeyCode::Numpad4));
        m.insert(Ascii::new("Numpad5"), Key::Virtual(VirtualKeyCode::Numpad5));
        m.insert(Ascii::new("Numpad6"), Key::Virtual(VirtualKeyCode::Numpad6));
        m.insert(Ascii::new("Numpad7"), Key::Virtual(VirtualKeyCode::Numpad7));
        m.insert(Ascii::new("Numpad8"), Key::Virtual(VirtualKeyCode::Numpad8));
        m.insert(Ascii::new("Numpad9"), Key::Virtual(VirtualKeyCode::Numpad9));
        m.insert(Ascii::new("AbntC1"), Key::Virtual(VirtualKeyCode::AbntC1));
        m.insert(Ascii::new("AbntC2"), Key::Virtual(VirtualKeyCode::AbntC2));
        m.insert(Ascii::new("Add"), Key::Virtual(VirtualKeyCode::Add));
        m.insert(Ascii::new("Apostrophe"), Key::Virtual(VirtualKeyCode::Apostrophe));
        m.insert(Ascii::new("Apps"), Key::Virtual(VirtualKeyCode::Apps));
        m.insert(Ascii::new("At"), Key::Virtual(VirtualKeyCode::At));
        m.insert(Ascii::new("Ax"), Key::Virtual(VirtualKeyCode::Ax));
        m.insert(Ascii::new("Backslash"), Key::Virtual(VirtualKeyCode::Backslash));
        m.insert(Ascii::new("Calculator"), Key::Virtual(VirtualKeyCode::Calculator));
        m.insert(Ascii::new("Capital"), Key::Virtual(VirtualKeyCode::Capital));
        m.insert(Ascii::new("Colon"), Key::Virtual(VirtualKeyCode::Colon));
        m.insert(Ascii::new("Comma"), Key::Virtual(VirtualKeyCode::Comma));
        m.insert(Ascii::new("Convert"), Key::Virtual(VirtualKeyCode::Convert));
        m.insert(Ascii::new("Decimal"), Key::Virtual(VirtualKeyCode::Decimal));
        m.insert(Ascii::new("Divide"), Key::Virtual(VirtualKeyCode::Divide));
        m.insert(Ascii::new("Equals"), Key::Virtual(VirtualKeyCode::Equals));
        m.insert(Ascii::new("Grave"), Key::Virtual(VirtualKeyCode::Grave));
        m.insert(Ascii::new("Kana"), Key::Virtual(VirtualKeyCode::Kana));
        m.insert(Ascii::new("Kanji"), Key::Virtual(VirtualKeyCode::Kanji));
        m.insert(Ascii::new("LAlt"), Key::Virtual(VirtualKeyCode::LAlt));
        m.insert(Ascii::new("LBracket"), Key::Virtual(VirtualKeyCode::LBracket));
        m.insert(Ascii::new("LControl"), Key::Virtual(VirtualKeyCode::LControl));
        m.insert(Ascii::new("LShift"), Key::Virtual(VirtualKeyCode::LShift));
        m.insert(Ascii::new("LWin"), Key::Virtual(VirtualKeyCode::LWin));
        m.insert(Ascii::new("Mail"), Key::Virtual(VirtualKeyCode::Mail));
        m.insert(Ascii::new("MediaSelect"), Key::Virtual(VirtualKeyCode::MediaSelect));
        m.insert(Ascii::new("MediaStop"), Key::Virtual(VirtualKeyCode::MediaStop));
        m.insert(Ascii::new("Minus"), Key::Virtual(VirtualKeyCode::Minus));
        m.insert(Ascii::new("Multiply"), Key::Virtual(VirtualKeyCode::Multiply));
        m.insert(Ascii::new("Mute"), Key::Virtual(VirtualKeyCode::Mute));
        m.insert(Ascii::new("MyComputer"), Key::Virtual(VirtualKeyCode::MyComputer));
        m.insert(Ascii::new("NavigateForward"), Key::Virtual(VirtualKeyCode::NavigateForward));
        m.insert(Ascii::new("NavigateBackward"), Key::Virtual(VirtualKeyCode::NavigateBackward));
        m.insert(Ascii::new("NextTrack"), Key::Virtual(VirtualKeyCode::NextTrack));
        m.insert(Ascii::new("NoConvert"), Key::Virtual(VirtualKeyCode::NoConvert));
        m.insert(Ascii::new("NumpadComma"), Key::Virtual(VirtualKeyCode::NumpadComma));
        m.insert(Ascii::new("NumpadEnter"), Key::Virtual(VirtualKeyCode::NumpadEnter));
        m.insert(Ascii::new("NumpadEquals"), Key::Virtual(VirtualKeyCode::NumpadEquals));
        m.insert(Ascii::new("OEM102"), Key::Virtual(VirtualKeyCode::OEM102));
        m.insert(Ascii::new("Period"), Key::Virtual(VirtualKeyCode::Period));
        m.insert(Ascii::new("PlayPause"), Key::Virtual(VirtualKeyCode::PlayPause));
        m.insert(Ascii::new("Power"), Key::Virtual(VirtualKeyCode::Power));
        m.insert(Ascii::new("PrevTrack"), Key::Virtual(VirtualKeyCode::PrevTrack));
        m.insert(Ascii::new("RAlt"), Key::Virtual(VirtualKeyCode::RAlt));
        m.insert(Ascii::new("RBracket"), Key::Virtual(VirtualKeyCode::RBracket));
        m.insert(Ascii::new("RControl"), Key::Virtual(VirtualKeyCode::RControl));
        m.insert(Ascii::new("RShift"), Key::Virtual(VirtualKeyCode::RShift));
        m.insert(Ascii::new("RWin"), Key::Virtual(VirtualKeyCode::RWin));
        m.insert(Ascii::new("Semicolon"), Key::Virtual(VirtualKeyCode::Semicolon));
        m.insert(Ascii::new("Slash"), Key::Virtual(VirtualKeyCode::Slash));
        m.insert(Ascii::new("Sleep"), Key::Virtual(VirtualKeyCode::Sleep));
        m.insert(Ascii::new("Stop"), Key::Virtual(VirtualKeyCode::Stop));
        m.insert(Ascii::new("Subtract"), Key::Virtual(VirtualKeyCode::Subtract));
        m.insert(Ascii::new("Sysrq"), Key::Virtual(VirtualKeyCode::Sysrq));
        m.insert(Ascii::new("Tab"), Key::Virtual(VirtualKeyCode::Tab));
        m.insert(Ascii::new("Underline"), Key::Virtual(VirtualKeyCode::Underline));
        m.insert(Ascii::new("Unlabeled"), Key::Virtual(VirtualKeyCode::Unlabeled));
        m.insert(Ascii::new("VolumeDown"), Key::Virtual(VirtualKeyCode::VolumeDown));
        m.insert(Ascii::new("VolumeUp"), Key::Virtual(VirtualKeyCode::VolumeUp));
        m.insert(Ascii::new("Wake"), Key::Virtual(VirtualKeyCode::Wake));
        m.insert(Ascii::new("WebBack"), Key::Virtual(VirtualKeyCode::WebBack));
        m.insert(Ascii::new("WebFavorites"), Key::Virtual(VirtualKeyCode::WebFavorites));
        m.insert(Ascii::new("WebForward"), Key::Virtual(VirtualKeyCode::WebForward));
        m.insert(Ascii::new("WebHome"), Key::Virtual(VirtualKeyCode::WebHome));
        m.insert(Ascii::new("WebRefresh"), Key::Virtual(VirtualKeyCode::WebRefresh));
        m.insert(Ascii::new("WebSearch"), Key::Virtual(VirtualKeyCode::WebSearch));
        m.insert(Ascii::new("WebStop"), Key::Virtual(VirtualKeyCode::WebStop));
        m.insert(Ascii::new("Yen"), Key::Virtual(VirtualKeyCode::Yen));
        m.insert(Ascii::new("Copy"), Key::Virtual(VirtualKeyCode::Copy));
        m.insert(Ascii::new("Paste"), Key::Virtual(VirtualKeyCode::Paste));
        m.insert(Ascii::new("Cut"), Key::Virtual(VirtualKeyCode::Cut));
        m
    };
}

impl Key {
    pub fn from_virtual(s: &str) -> Fallible<Self> {
        if let Some(key) = KEYCODES.get(&Ascii::new(s)) {
            return Ok(*key);
        }
        if s.len() > 5 && eq_ascii(&s[0..5], "mouse") {
            let button = s[5..].parse::<u32>()?;
            return Ok(Key::MouseButton(button));
        }
        bail!("unknown virtual keycode")
    }
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct KeySet {
    pub keys: SmallVec<[Key; 2]>,
}

impl KeySet {
    // Parse keysets of the form a+b+c; e.g. LControl+RControl+Space into
    // a discreet keyset.
    //
    // Note that there is a special case for the 4 modifiers in which we
    // expect to be able to refer to "Control" and not care what key it is.
    // In this case we emit all possible keysets, combinatorially.
    pub fn from_virtual(keyset: &str) -> Fallible<Vec<Self>> {
        let mut out = vec![SmallVec::<[Key; 2]>::new()];
        for keyname in keyset.split('+') {
            if let Ok(key) = Key::from_virtual(keyname) {
                for tmp in &mut out {
                    tmp.push(key);
                }
            } else if MIRROR_MODIFIERS.contains(&Ascii::new(keyname)) {
                let mut next_out = Vec::new();
                for mut tmp in out.drain(..) {
                    let mut cpy = tmp.clone();
                    tmp.push(Key::from_virtual(&format!("L{}", keyname))?);
                    cpy.push(Key::from_virtual(&format!("R{}", keyname))?);
                    next_out.push(tmp);
                    next_out.push(cpy);
                }
                out = next_out;
            } else {
                warn!("unknown key name: {}", keyname);
            }
        }
        ensure!(!out.is_empty(), "no key matching {}", keyset);
        Ok(out.drain(..).map(|v| Self { keys: v }).collect::<Vec<_>>())
    }

    // Get the activating key in the keyset.
    pub fn activating(&self) -> Key {
        assert!(!self.keys.is_empty());
        *self.keys.last().unwrap()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_can_create_keys() -> Fallible<()> {
        assert_eq!(Key::from_virtual("A")?, Key::Virtual(VirtualKeyCode::A));
        assert_eq!(Key::from_virtual("a")?, Key::Virtual(VirtualKeyCode::A));
        assert_eq!(
            Key::from_virtual("PageUp")?,
            Key::Virtual(VirtualKeyCode::PageUp)
        );
        assert_eq!(
            Key::from_virtual("pageup")?,
            Key::Virtual(VirtualKeyCode::PageUp)
        );
        assert_eq!(
            Key::from_virtual("pAgEuP")?,
            Key::Virtual(VirtualKeyCode::PageUp)
        );
        Ok(())
    }

    #[test]
    fn test_can_create_mouse() -> Fallible<()> {
        assert_eq!(Key::from_virtual("MoUsE5000")?, Key::MouseButton(5000));
        Ok(())
    }

    #[test]
    fn test_can_create_keysets() -> Fallible<()> {
        assert_eq!(KeySet::from_virtual("a+b")?.len(), 1);
        assert_eq!(KeySet::from_virtual("Control+Win+a")?.len(), 4);
        assert_eq!(KeySet::from_virtual("Control+b+Shift")?.len(), 4);
        Ok(())
    }
}
