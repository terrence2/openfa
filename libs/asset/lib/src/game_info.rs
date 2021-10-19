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
use std::env;

// FIXME: should this all be in a common crate?
pub struct GameInfo {
    pub name: &'static str,
    pub long_name: &'static str,
    pub developer: &'static str,
    pub publisher: &'static str,
    pub release_year: usize,
    pub release_month: usize,
    pub release_day: usize,
    pub test_dir: &'static str,
    pub allow_packed_t2: bool,
}

impl GameInfo {
    pub fn packed_label(&self) -> String {
        format!("packed:{}", self.test_dir)
    }

    pub fn unpacked_label(&self) -> String {
        format!("unpacked:{}", self.test_dir)
    }

    pub fn label(&self) -> String {
        let use_packed = env::var("USE_PACKED").unwrap_or_else(|_| "0".to_owned());
        if use_packed == "1" || use_packed.to_ascii_lowercase().starts_with('t') {
            self.packed_label()
        } else {
            self.unpacked_label()
        }
    }
}

const USNF: GameInfo = GameInfo {
    name: "USNF",
    long_name: "U.S. Navy Fighters",
    developer: "Electronic Arts Inc.",
    publisher: "Electronic Arts Inc.",
    release_year: 1994,
    release_month: 11,
    release_day: 1,
    test_dir: "USNF",
    allow_packed_t2: false,
};

const USMF: GameInfo = GameInfo {
    name: "MF",
    long_name: "U.S. Navy Fighters Expansion Disk: Marine Fighters",
    developer: "Electronic Arts Inc.",
    publisher: "Electronic Arts Inc.",
    release_year: 1995,
    release_month: 0,
    release_day: 0,
    test_dir: "MF",
    allow_packed_t2: false,
};

const ATF: GameInfo = GameInfo {
    name: "ATF",
    long_name: "Jane's ATF: Advanced Tactical Fighters",
    developer: "Jane's Combat Simulations",
    publisher: "Electronic Arts Inc.",
    test_dir: "ATF",
    release_year: 1996,
    release_month: 3,
    release_day: 31,
    allow_packed_t2: false,
};

const ATF_NATO: GameInfo = GameInfo {
    name: "ATF Nato Fighters",
    long_name: "Jane's ATF: Nato Fighters",
    developer: "Jane's Combat Simulations",
    publisher: "Electronic Arts Inc.",
    release_year: 1996,
    release_month: 9,
    release_day: 30,
    test_dir: "ATFNATO",
    allow_packed_t2: false,
};

const USNF97: GameInfo = GameInfo {
    name: "US Navy Fighters '97",
    long_name: "Jane's US Navy Fighters '97",
    developer: "Jane's Combat Simulations",
    publisher: "Electronic Arts Inc.",
    release_year: 1996,
    release_month: 11,
    release_day: 0,
    test_dir: "USNF97",
    allow_packed_t2: true,
};

const ATF_GOLD: GameInfo = GameInfo {
    name: "ATF Gold",
    long_name: "Jane's ATF: Gold Edition",
    developer: "Jane's Combat Simulations",
    publisher: "Electronic Arts Inc.",
    release_year: 1997,
    release_month: 0,
    release_day: 0,
    test_dir: "ATFGOLD",
    allow_packed_t2: true,
};

const FIGHTERS_ANTHOLOGY: GameInfo = GameInfo {
    name: "Fighters Anthology",
    long_name: "Jane's Fighters Anthology",
    developer: "Jane's Combat Simulations",
    publisher: "Electronic Arts Inc.",
    release_year: 1998,
    release_month: 0,
    release_day: 0,
    test_dir: "FA",
    allow_packed_t2: true,
};

pub const GAME_INFO: [&GameInfo; 7] = [
    &USNF,
    &USMF,
    &ATF,
    &ATF_NATO,
    &USNF97,
    &ATF_GOLD,
    &FIGHTERS_ANTHOLOGY,
];
