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
    pub unique_files: &'static [&'static str],
    pub cd_libs: &'static [&'static str],
}

impl GameInfo {
    // FIXME: we should remove all of this
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
    unique_files: &["USNF.EXE"],
    cd_libs: &["1.LIB", "2.LIB", "3.LIB", "5.LIB", "6.LIB", "7.LIB"],
};

const USMF: GameInfo = GameInfo {
    name: "Marine Fighters",
    long_name: "U.S. Navy Fighters Expansion Disk: Marine Fighters",
    developer: "Electronic Arts Inc.",
    publisher: "Electronic Arts Inc.",
    release_year: 1995,
    release_month: 0,
    release_day: 0,
    test_dir: "MF",
    allow_packed_t2: false,
    unique_files: &["42.2D", "KURILE.T2"],
    cd_libs: &["8.LIB"],
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
    unique_files: &["ATF.BAT"],
    cd_libs: &["4C.LIB", "9.LIB"],
};

const ATF_NATO: GameInfo = GameInfo {
    name: "ATF Nato",
    long_name: "Jane's ATF: Nato Fighters",
    developer: "Jane's Combat Simulations",
    publisher: "Electronic Arts Inc.",
    release_year: 1996,
    release_month: 9,
    release_day: 30,
    test_dir: "ATFNATO",
    allow_packed_t2: false,
    unique_files: &["NATO.BAT", "BAL.T2"],
    cd_libs: &["4C.LIB", "10.LIB"],
};

const USNF97: GameInfo = GameInfo {
    name: "USNF '97",
    long_name: "Jane's US Navy Fighters '97",
    developer: "Jane's Combat Simulations",
    publisher: "Electronic Arts Inc.",
    release_year: 1996,
    release_month: 11,
    release_day: 0,
    test_dir: "USNF97",
    allow_packed_t2: true,
    unique_files: &["USNF.EXE", "USNF_1.LIB"],
    cd_libs: &["USNF_3.LIB", "USNF_7.LIB", "USNF_8.LIB", "USNF_10.LIB"],
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
    unique_files: &["ATF.EXE", "ATF_1.LIB"],
    cd_libs: &["ATF_3.LIB", "ATF_4C.LIB", "ATF_10.LIB"],
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
    unique_files: &["FA.EXE"],
    cd_libs: &[
        // CD1
        "FA_4C.LIB",
        "FA_7.LIB",
        // CD2
        "FA_3.LIB",
        "FA_10.LIB",
        "FA_10B.LIB",
        "FA_11.LIB",
        "FA_11B.LIB",
    ],
};

pub const GAME_INFO: [&GameInfo; 7] = [
    &FIGHTERS_ANTHOLOGY,
    &ATF_GOLD,
    &USNF97,
    &ATF_NATO,
    &ATF,
    &USMF,
    &USNF,
];
