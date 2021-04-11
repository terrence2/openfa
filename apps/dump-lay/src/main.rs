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
use anyhow::Result;
use lay::Layer;
use lib::CatalogBuilder;
use pal::Palette;
use std::fs;
use structopt::StructOpt;

/// Dump LAY files
#[derive(Debug, StructOpt)]
struct Opt {
    /// Layer files to dump
    inputs: Vec<String>,
}

fn main() -> Result<()> {
    let opt = Opt::from_args();
    let (catalog, inputs) = CatalogBuilder::build_and_select(&opt.inputs)?;
    for &fid in &inputs {
        let label = catalog.file_label(fid)?;
        let game = label.split(':').last().unwrap();
        let name = catalog.stat_sync(fid)?.name().to_owned();
        fs::create_dir_all(&format!("dump/lay-pal/{}-{}", game, name))?;

        let system_palette_data = catalog.read_labeled_name_sync(&label, "PALETTE.PAL")?;
        let system_palette = Palette::from_bytes(&system_palette_data)?;

        let layer_data = catalog.read_sync(fid)?;
        let layer = Layer::from_bytes(&layer_data, &system_palette)?;
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

            let output = format!("dump/lay-pal/{}-{}/{}", game, name, i);
            println!("Writing: {}.png", output);
            palette.dump_png(&output)?
        }
    }

    Ok(())
}
