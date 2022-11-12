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
use absolute_unit::{feet, knots, meters, meters_per_second, nautical_miles_per_hour};
use anyhow::Result;
use catalog::Catalog;
use lib::{Libs, LibsOpts};
use pt::EnvelopeIntersection;
use structopt::StructOpt;
use xt::{HardpointType, NpcType, ObjectType, PlaneType, ProjectileType, TypeManager};

/// Show the contents of OT, PT, NT, and JT files
#[derive(Debug, StructOpt)]
struct Opt {
    /// The XT files to load
    inputs: Vec<String>,

    /// Show just one field
    #[structopt(short, long)]
    field: Option<String>,

    /// Show envelope intercept at altitude
    #[structopt(short, long)]
    altitude: Option<i32>,

    #[structopt(flatten)]
    libs_opts: LibsOpts,
}

fn main() -> Result<()> {
    env_logger::init();
    let opt = Opt::from_args();
    let libs = Libs::bootstrap(&opt.libs_opts)?;
    for (game, _palette, catalog) in libs.selected() {
        for input in &opt.inputs {
            for fid in catalog.find_glob(input)? {
                let meta = catalog.stat(fid)?;
                if let Some(req_field) = &opt.field {
                    show_xt_field(meta.name(), req_field, catalog)?;
                } else {
                    println!("At: {}:{:13} @ {}", game.test_dir, meta.name(), meta.path());
                    show_xt(meta.name(), opt.altitude, catalog)?;
                }
            }
        }
    }

    Ok(())
}

fn show_xt_field(name: &str, req_field: &str, catalog: &Catalog) -> Result<()> {
    let type_manager = TypeManager::empty();
    let xt = type_manager.load(name, catalog)?;
    let ot = xt.ot();
    for &field in ObjectType::fields() {
        if field == req_field {
            println!("{:<14}:{} = {}", name, field, ot.get_field(field));
        }
    }
    if let Some(nt) = xt.nt() {
        for &field in NpcType::fields() {
            if field == req_field {
                println!("{:<14}:{} = {}", name, field, nt.get_field(field));
            }
        }
    }
    if let Some(jt) = xt.jt() {
        for &field in ProjectileType::fields() {
            if field == req_field {
                println!("{:<14}:{} = {}", name, field, jt.get_field(field));
            }
        }
    }
    if let Some(pt) = xt.pt() {
        for &field in PlaneType::fields() {
            if field == req_field {
                println!("{:<14}:{} = {}", name, field, pt.get_field(field));
            }
        }
    }
    Ok(())
}

fn show_xt(name: &str, altitude: Option<i32>, catalog: &Catalog) -> Result<()> {
    let type_manager = TypeManager::empty();
    let xt = type_manager.load(name, catalog)?;

    let ot = &xt.ot();
    println!("{:>25}", "ObjectType");
    println!("{:>25}", "==========");
    for field in ObjectType::fields() {
        println!("{:>25}: {}", field, ot.get_field(field));
    }
    println!();

    if let Some(nt) = xt.nt() {
        println!("{:>25}", "NPC Type");
        println!("{:>25}", "========");
        for field in NpcType::fields() {
            if field == &"hards" {
                continue;
            }
            println!("{:>25}: {}", field, nt.get_field(field));
        }
        for (i, hp) in nt.hards.iter().enumerate() {
            println!();
            println!("{:>25}: {:02}", "Hardpoint", i + 1);
            println!("{:>25}====", "=========");
            for field in HardpointType::fields() {
                println!("{:>25}: {}", field, hp.get_field(field));
            }
        }
        println!();
    }

    if let Some(jt) = xt.jt() {
        println!("{:>25}", "Projectile Type");
        println!("{:>25}", "===============");
        for field in ProjectileType::fields() {
            println!("{:>25}: {}", field, jt.get_field(field));
        }
    }

    if let Some(pt) = xt.pt() {
        println!("{:>25}", "Plane Type");
        println!("{:>25}", "==========");
        for field in PlaneType::fields() {
            if field == &"envelopes" {
                continue;
            }
            if field == &"max_speed_sea_level" {
                println!("{:>25}: {:0.0}", field, knots!(pt.max_speed_sea_level));
            } else if field == &"max_speed_36a" {
                println!("{:>25}: {:0.0}", field, knots!(pt.max_speed_36a));
            } else {
                println!("{:>25}: {}", field, pt.get_field(field));
            }
        }
        if let Some(altitude) = altitude {
            if let Some(env) = pt.envelopes.envelope(1) {
                if let Some(stall_speed) = env.find_min_lift_speed_at(meters!(feet!(altitude))) {
                    if let EnvelopeIntersection::Inside { to_over_speed, .. } = env
                        .find_g_load_extrema(
                            stall_speed + meters_per_second!(1f64),
                            meters!(feet!(altitude)),
                        )
                    {
                        println!(
                            "{} @{}: {:0.0}, {:0.0}",
                            env.gload,
                            altitude,
                            knots!(stall_speed),
                            knots!(meters_per_second!(stall_speed.f64() + to_over_speed + 1f64))
                        );
                    }
                }
            }
        } else {
            for (i, env) in pt.envelopes.iter().enumerate() {
                println!();
                println!("{:>25}: {:02}", "Envelope", i + 1);
                println!("{:>25}====", "========");
                println!("{:>25}: {}", "gload", env.get_field("gload"));
                println!("{:>25}: {}", "stall_lift", env.get_field("stall_lift"));
                println!(
                    "{:>25}: {}",
                    "max_speed_idx",
                    env.get_field("max_speed_index")
                );
                println!("{:>25}:", "shape");
                for i in 0..env.count {
                    let shape = &env.shape.coord(i as usize);
                    println!(
                        "{:>25}     {:>4.4} {:>6.4}",
                        " ",
                        knots!(shape.speed()),
                        feet!(shape.altitude())
                    );
                }
            }
            println!("@ Env 1 -");
            for i in 0..9 {
                if let (Some(min), Some(max)) = pt
                    .envelopes
                    .envelope(1)
                    .unwrap()
                    .find_x_extrema(meters!(feet!(i * 5_000)))
                {
                    println!(
                        "{}\t{:0.1}\t{:0.1}",
                        i * 5_000,
                        nautical_miles_per_hour!(min).f64(),
                        nautical_miles_per_hour!(max).f64()
                    );
                }
            }
        }
    }

    Ok(())
}
