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
/*
use criterion::{criterion_group, criterion_main, Criterion};
use failure::Fallible;
use nalgebra::Point3;
use omnilib::OmniLib;
use shape_chunk::{DrawSelection, OpenChunk};
use specs::prelude::*;
use std::time::Instant;
use universe::{
    system::shape_mesh::{
        FlagCoalesceSystem, FlagUpdateSystem, XformCoalesceSystem, XformUpdateSystem,
    },
    Universe,
};
use window::{GraphicsConfigBuilder, GraphicsWindow};

fn set_up_world() -> Fallible<Universe> {
    let omni = OmniLib::new_for_test_in_games(&["FA"])?;
    let mut universe = Universe::new(omni.library("FA"))?;
    let window = GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?;
    let mut upload = OpenChunk::new(&window)?;

    // Has a continuously updating transform / rotation.
    // Has flags that may get updated at any time via AI or player:
    //    AB, hook, flaps, aileron, gear, bay, brake, rudder, alive/dead, animation, sams, ejects
    // Has xforms tracking motion for modified state via AI/player:
    //    canard, ab, gear, sweep
    let shape_id = upload.upload_shape(
        "F31.SH",
        DrawSelection::NormalModel,
        universe.system_palette(),
        universe.library(),
        &window,
    )?;

    let part = upload.part(shape_id);
    for _ in 0..10_000 {
        let _ent = universe.create_flyer(shape_id, Point3::new(0f64, 0f64, 0f64), part)?;
    }
    Ok(universe)
}

fn criterion_benchmark(c: &mut Criterion) {
    // Set up universe
    let start = Instant::now();
    let mut world = set_up_world().unwrap();

    let mut update_dispatcher = DispatcherBuilder::new()
        .with(FlagUpdateSystem::new(&start), "flag-update", &[])
        .with(XformUpdateSystem::new(&start), "xform-update", &[])
        .build();
    c.bench_function("update all", move |b| {
        b.iter(|| {
            world.run(&mut update_dispatcher);
        })
    });

    let mut world = set_up_world().unwrap();
    let mut upload_dispatcher = DispatcherBuilder::new()
        .with(FlagCoalesceSystem::new(), "flag-coalesce", &[])
        .with(XformCoalesceSystem::new(), "xform-coalesce", &[])
        .build();

    c.bench_function("upload all", move |b| {
        b.iter(|| {
            world.run(&mut upload_dispatcher);
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
*/
fn main() {}