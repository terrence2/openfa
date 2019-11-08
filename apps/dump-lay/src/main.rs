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
use failure::Fallible;
use lay::Layer;
use omnilib::OmniLib;
use pal::Palette;
use std::fs;

fn main() -> Fallible<()> {
    let omni = OmniLib::new_for_test_in_games(&["FA"])?;

    for (libname, name) in omni.find_matching("*.LAY")? {
        println!("Dumping {}:{}", libname, name);
        fs::create_dir_all(&format!("dump/lay-pal/{}-{}", libname, name))?;

        let lib = omni.library(&libname);
        let system_palette = Palette::from_bytes(&lib.load("PALETTE.PAL")?)?;
        let data = lib.load(&name)?;
        let layer = Layer::from_bytes(&data, &lib)?;
        for i in 0..5 {
            if i >= layer.num_indices() {
                continue;
            }

            let layer_data = layer.for_index(i)?;

            let r0 = layer_data.slice(0x00, 0x10)?;
            let r1 = layer_data.slice(0x10, 0x20)?;
            let r2 = layer_data.slice(0x20, 0x30)?;
            let r3 = layer_data.slice(0x30, 0x40)?;

            // We need to put rows r0, r1, and r2 into into 0xC0, 0xE0, 0xF0 somehow.
            let mut palette = system_palette.clone();
            palette.overlay_at(&r1, 0xF0 - 1)?;
            palette.overlay_at(&r0, 0xE0 - 1)?;
            palette.overlay_at(&r3, 0xD0)?;
            palette.overlay_at(&r2, 0xC0)?;

            // palette.override_one(0xFF, [0, 0, 0]);

            palette.dump_png(&format!("dump/lay-pal/{}-{}/{}.png", libname, name, i))?
        }
    }

    Ok(())
}
