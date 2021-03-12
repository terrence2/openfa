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
use lib::{from_dos_string, CatalogBuilder};
use mm::MissionMap;
use std::time::Instant;
use structopt::StructOpt;
use xt::TypeManager;

/// Print contents of MM files, with various options.
#[derive(Debug, StructOpt)]
struct Opt {
    /// Back of the envelop profiling.
    #[structopt(long)]
    profile: bool,

    /// One or more MM files to process
    inputs: Vec<String>,
}

const PROFILE_COUNT: usize = 10000;

fn main() -> Result<()> {
    env_logger::init();
    let opt = Opt::from_args();
    let (catalog, inputs) = CatalogBuilder::build_and_select(&opt.inputs)?;
    if inputs.is_empty() {
        println!("No inputs found!");
        return Ok(());
    }

    let type_manager = TypeManager::empty();
    for &fid in &inputs {
        let label = catalog.file_label(fid)?;
        let _game = label.split(':').last().unwrap();
        let _meta = catalog.stat_sync(fid)?;

        let raw = catalog.read_sync(fid)?;
        let content = from_dos_string(raw).to_owned();

        if opt.profile {
            let start = Instant::now();
            for _ in 0..PROFILE_COUNT {
                let _ = MissionMap::from_str(&content, &type_manager, &catalog)?;
            }
            println!(
                "load time: {}ms",
                (start.elapsed().as_micros() / PROFILE_COUNT as u128) as f64 / 1000.0
            );
            return Ok(());
        }
        let mm = MissionMap::from_str(&content, &type_manager, &catalog)?;
        println!("map name:    {}", mm.map_name());
        println!("t2 name:     {}", mm.t2_name());
        println!("layer name:  {}", mm.layer_name());
        println!("layer index: {}", mm.layer_index());
        println!("tmap count:  {}", mm.texture_maps().len());
        println!("tdic count:  {}", mm.texture_dictionary().len());
        println!("obj count:   {}", mm.objects().len());
    }

    Ok(())
}
