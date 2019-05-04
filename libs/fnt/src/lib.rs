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
use failure::{ensure, Fallible};
use peff::PE;
use reverse::bs2s;
use std::mem;

pub struct Fnt {}

const FNT_LOAD_BASE: u32 = 0x0000_0000;

impl Fnt {
    pub fn from_bytes(bytes: &[u8]) -> Fallible<Self> {
        let mut pe = PE::from_bytes(bytes)?;
        pe.relocate(FNT_LOAD_BASE)?;

        let dwords: &[u32] = unsafe { mem::transmute(&pe.code[0..1028]) };
        let header = dwords[0];

        let mut out = String::new();
        for i in 1..256 {
            let target = dwords[i] as usize;
            ensure!(target < pe.code.len(), "out of bounds");

            let next = if i < 255 {
                dwords[i + 1] as usize
            } else {
                pe.code.len() - 9
            };
            ensure!(next <= pe.code.len(), "next out of bounds");
            ensure!(pe.code[next - 1] == 0xC3, "expected to end in C3");

            let a = bs2s(&pe.code[target..next]).trim_end().to_owned();
            out += &format!("{:04X} ({})  ", i - 1, a);
            //println!("{}", bs2s(&pe.code[target..target + 8]));
        }

        //println!("{:?}", pe.relocs);
        //println!("{}", bs2s(&pe.code));
        println!("{}: {}", header, out);
        Ok(Self {})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use failure::Error;
    use omnilib::OmniLib;

    #[test]
    fn it_can_parse_all_fnt_files() -> Fallible<()> {
        assert_eq!(2 + 2, 4);

        let omni = OmniLib::new_for_test_in_games(&[
            "USNF", "MF", "ATF", "ATFNATO", "ATFGOLD", "USNF97", "FA",
        ])?;
        for (game, name) in omni.find_matching("*.FNT")?.iter() {
            // println!(
            //     "At: {}:{:13} @ {}",
            //     game,
            //     name,
            //     omni.path(game, name)
            //         .or_else::<Error, _>(|_| Ok("<none>".to_string()))?
            // );

            let lib = omni.library(game);
            let _fnt = Fnt::from_bytes(&lib.load(name)?)?;
        }

        Ok(())
    }
}
