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
use omnilib::{make_opt_struct, OmniLib};
use simplelog::{Config, LevelFilter, TermLogger};
use structopt::StructOpt;
use xt::{Envelope, HardpointType, NpcType, ObjectType, PlaneType, ProjectileType, TypeManager};

make_opt_struct!(
    #[structopt(name = "dump-xt", about = "Show the contents of an XT file")]
    Opt {
        #[structopt(help = "XT file to load")]
        omni_input => String
    }
);

fn main() -> Fallible<()> {
    let opt = Opt::from_args();
    TermLogger::init(LevelFilter::Warn, Config::default())?;

    let (omni, game, name) = opt.find_input(&opt.omni_input)?;
    let lib = omni.library(&game);
    let types = TypeManager::new(lib.clone());
    let xt = types.load(&name)?;

    let ot = &xt.ot();
    println!("{:>25}", "ObjectType");
    println!("{:>25}", "==========");
    for field in ObjectType::fields() {
        println!("{:>25}: {}", field, ot.get_field(field));
    }
    println!();

    if let Ok(nt) = xt.nt() {
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

    if let Ok(jt) = xt.jt() {
        println!("{:>25}", "Projectile Type");
        println!("{:>25}", "===============");
        for field in ProjectileType::fields() {
            println!("{:>25}: {}", field, jt.get_field(field));
        }
    }

    if let Ok(pt) = xt.pt() {
        println!("{:>25}", "Plane Type");
        println!("{:>25}", "==========");
        for field in PlaneType::fields() {
            if field == &"envelopes" {
                continue;
            }
            println!("{:>25}: {}", field, pt.get_field(field));
        }
        for (i, env) in pt.envelopes.iter().enumerate() {
            println!();
            println!("{:>25}: {:02}", "Envelope", i + 1);
            println!("{:>25}====", "========");
            for field in Envelope::fields() {
                println!("{:>25}: {}", field, env.get_field(field));
            }
        }
    }

    Ok(())
}
