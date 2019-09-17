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
use criterion::{criterion_group, criterion_main, Criterion};
use failure::Fallible;
use nalgebra::Point3;
use omnilib::OmniLib;
use shape_chunk::{DrawSelection, OpenChunk, ShapeId, ShapeWidgets};
use specs::prelude::*;
use std::{
    cell::RefCell,
    collections::{hash_map::Entry, HashMap},
    time::Instant,
};
use window::{GraphicsConfigBuilder, GraphicsWindow};
use world::{component::*, World};

pub struct FlagUpdateSystem {
    start: Instant,
}
impl FlagUpdateSystem {
    pub fn new(start: &Instant) -> Self {
        Self { start: *start }
    }
}
impl<'a> System<'a> for FlagUpdateSystem {
    type SystemData = (
        ReadStorage<'a, ShapeMesh>,
        WriteStorage<'a, ShapeMeshFlagBuffer>,
    );

    fn run(&mut self, (shape_meshs, mut flag_buffers): Self::SystemData) {
        (&shape_meshs, &mut flag_buffers)
            .par_join()
            .for_each(|(shape_mesh, flag_buffer)| {
                shape_mesh
                    .draw_state()
                    .build_mask_into(&self.start, flag_buffer.errata, &mut flag_buffer.buffer)
                    .unwrap();
            });
    }
}

thread_local! {
    pub static WIDGET_CACHE: RefCell<HashMap<ShapeId, ShapeWidgets>> = RefCell::new(HashMap::new());
}

pub struct XformUpdateSystem {
    start: Instant,
}
impl XformUpdateSystem {
    pub fn new(start: &Instant) -> Self {
        Self { start: *start }
    }
}
impl<'a> System<'a> for XformUpdateSystem {
    type SystemData = (
        ReadStorage<'a, ShapeMesh>,
        WriteStorage<'a, ShapeMeshXformBuffer>,
    );

    fn run(&mut self, (shape_meshs, mut xform_buffers): Self::SystemData) {
        let now = Instant::now();
        (&shape_meshs, &mut xform_buffers)
            .par_join()
            .for_each(|(shape_mesh, xform_buffer)| {
                WIDGET_CACHE.with(|widget_cache| {
                    match widget_cache.borrow_mut().entry(xform_buffer.shape_id) {
                        Entry::Occupied(mut e) => {
                            e.get_mut()
                                .animate_into(
                                    shape_mesh.draw_state(),
                                    &self.start,
                                    &now,
                                    &mut xform_buffer.buffer,
                                )
                                .unwrap();
                        }
                        Entry::Vacant(e) => {
                            let widgets = xform_buffer.widgets.read().unwrap().clone();
                            e.insert(widgets);
                        }
                    }
                });
            });
    }
}

fn set_up_world() -> Fallible<World> {
    let omni = OmniLib::new_for_test_in_games(&["FA"])?;
    let world = World::new(omni.library("FA"))?;
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
        world.system_palette(),
        world.library(),
        &window,
    )?;

    let part = unsafe { upload.part(shape_id) };
    for _ in 0..10_000 {
        let _ent = world.create_flyer(shape_id, Point3::new(0f64, 0f64, 0f64), part)?;
    }
    Ok(world)
}

fn criterion_benchmark(c: &mut Criterion) {
    // Set up world
    let start = Instant::now();
    let world = set_up_world().unwrap();

    let mut update_dispatcher = DispatcherBuilder::new()
        .with(FlagUpdateSystem::new(&start), "flag-update", &[])
        .with(XformUpdateSystem::new(&start), "xform-update", &[])
        .build();

    c.bench_function("update all flags", move |b| {
        b.iter(|| {
            world.run(&mut update_dispatcher);
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
