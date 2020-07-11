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
use crate::{Command, Key, KeySet};
use failure::Fallible;
use smallvec::{smallvec, SmallVec};
use std::collections::{HashMap, HashSet};
use winit::event::ElementState;

// Map from key, buttons, and axes to commands.
pub struct Bindings {
    pub name: String,
    press_chords: HashMap<Key, Vec<(KeySet, String)>>,
    release_keys: HashMap<Key, HashSet<String>>,
}

impl Bindings {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            press_chords: HashMap::new(),
            release_keys: HashMap::new(),
        }
    }

    pub fn bind(mut self, command: &str, keyset: &str) -> Fallible<Self> {
        let (activate, deactivate) = if command.starts_with('+') {
            (command, Some(format!("-{}", &command[1..])))
        } else {
            (command, None)
        };
        for ks in KeySet::from_virtual(keyset)?.drain(..) {
            let sets = self
                .press_chords
                .entry(ks.activating())
                .or_insert_with(Vec::new);

            if let Some(ref d) = deactivate {
                for key in &ks.keys {
                    let keys = self.release_keys.entry(*key).or_insert_with(HashSet::new);
                    keys.insert(d.to_owned());
                }
            }

            sets.push((ks, activate.to_owned()));
            sets.sort_by_key(|(set, _)| usize::max_value() - set.keys.len());
        }
        Ok(self)
    }

    pub fn match_key(
        &self,
        key: Key,
        state: ElementState,
        key_states: &HashMap<Key, ElementState>,
    ) -> SmallVec<[Command; 4]> {
        if state == ElementState::Pressed {
            if let Some(chords) = self.press_chords.get(&key) {
                for (chord, activate) in chords {
                    if Self::chord_is_pressed(&chord.keys, key_states) {
                        return smallvec![Command::from_string(activate.to_owned())];
                    }
                }
            }
        } else if let Some(commands) = self.release_keys.get(&key) {
            return commands
                .iter()
                .map(|v| Command::from_string(v.to_owned()))
                .collect::<SmallVec<_>>();
        }
        smallvec![]
    }

    fn chord_is_pressed(binding_keys: &[Key], key_states: &HashMap<Key, ElementState>) -> bool {
        for binding_key in binding_keys.iter() {
            if let Some(current_state) = key_states.get(binding_key) {
                if *current_state == ElementState::Released {
                    return false;
                }
            } else {
                return false;
            }
        }
        true
    }
}
