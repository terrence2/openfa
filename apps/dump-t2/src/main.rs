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
use catalog::{Catalog, FileId};
use lib::{CatalogManager, CatalogOpts};
use std::time::Instant;
use structopt::StructOpt;
use t2::Terrain;

/// Print contents of T2 files, with various options.
#[derive(Debug, StructOpt)]
struct Opt {
    /// Back of the envelop profiling.
    #[structopt(long)]
    profile: bool,

    /// One or more MM files to process
    inputs: Vec<String>,

    #[structopt(flatten)]
    catalog_opts: CatalogOpts,
}

const PROFILE_COUNT: usize = 10000;

fn main() -> Result<()> {
    env_logger::init();
    let opt = Opt::from_args();
    let catalogs = CatalogManager::bootstrap(&opt.catalog_opts)?;
    for (game, catalog) in catalogs.selected() {
        for input in &opt.inputs {
            for fid in catalog.find_glob(input)? {
                let meta = catalog.stat_sync(fid)?;
                println!(
                    "{}:{:13} @ {}",
                    game.test_dir,
                    meta.name(),
                    meta.path()
                        .map(|v| v.to_string_lossy())
                        .unwrap_or_else(|| "<none>".into())
                );
                show_t2(fid, opt.profile, catalog)?;
            }
        }
    }

    Ok(())
}

fn show_t2(fid: FileId, profile: bool, catalog: &Catalog) -> Result<()> {
    let raw = &catalog.read_sync(fid)?;
    if profile {
        let start = Instant::now();
        for _ in 0..PROFILE_COUNT {
            let _ = Terrain::from_bytes(raw)?;
        }
        println!(
            "load time: {}ms",
            (start.elapsed().as_micros() / PROFILE_COUNT as u128) as f64 / 1000.0
        );
        return Ok(());
    }
    let t2 = Terrain::from_bytes(raw)?;
    println!("map name:    {}", t2.name());
    println!("width:       {}", t2.width());
    println!("height:      {}", t2.height());
    println!("extent e-w:  {}", t2.extent_east_west_in_ft());
    println!("extent n-s:  {}", t2.extent_north_south_in_ft());
    println!();

    Ok(())
}
