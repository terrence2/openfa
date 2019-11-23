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
use crate::{components::*, ShapeInstanceBuffer};
use shape_chunk::{ShapeId, ShapeWidgets};
use specs::prelude::*;
use std::{
    cell::RefCell,
    collections::{hash_map::Entry, HashMap},
    time::Instant,
};
use universe::component::Transform;

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
        ReadStorage<'a, ShapeComponent>,
        WriteStorage<'a, ShapeXformBuffer>,
    );

    fn run(&mut self, (shapes, mut xform_buffers): Self::SystemData) {
        let now = Instant::now();
        (&shapes, &mut xform_buffers)
            .par_join()
            .for_each(|(shape, xform_buffer)| {
                WIDGET_CACHE.with(|widget_cache| {
                    match widget_cache.borrow_mut().entry(xform_buffer.shape_id) {
                        Entry::Occupied(mut e) => {
                            e.get_mut()
                                .animate_into(
                                    &shape.draw_state,
                                    &self.start,
                                    &now,
                                    &mut xform_buffer.buffer,
                                )
                                .unwrap();
                        }
                        Entry::Vacant(e) => {
                            let mut widgets = xform_buffer.widgets.read().unwrap().clone();
                            widgets
                                .animate_into(
                                    &shape.draw_state,
                                    &self.start,
                                    &now,
                                    &mut xform_buffer.buffer,
                                )
                                .unwrap();
                            e.insert(widgets);
                        }
                    }
                });
            });
    }
}

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
        ReadStorage<'a, ShapeComponent>,
        WriteStorage<'a, ShapeFlagBuffer>,
    );

    fn run(&mut self, (shapes, mut flag_buffers): Self::SystemData) {
        (&shapes, &mut flag_buffers)
            .par_join()
            .for_each(|(shape, flag_buffer)| {
                shape
                    .draw_state
                    .build_mask_into(&self.start, flag_buffer.errata, &mut flag_buffer.buffer)
                    .unwrap();
            });
    }
}

pub struct TransformUpdateSystem;
impl<'a> System<'a> for TransformUpdateSystem {
    type SystemData = (
        ReadStorage<'a, Transform>,
        WriteStorage<'a, ShapeTransformBuffer>,
    );

    fn run(&mut self, (transforms, mut transform_buffers): Self::SystemData) {
        (&transforms, &mut transform_buffers).par_join().for_each(
            |(transform, transform_buffer)| {
                transform_buffer.buffer = transform.compact();
            },
        );
    }
}

pub struct CoalesceSystem<'b> {
    inst_man: &'b mut ShapeInstanceBuffer,
}
impl<'b> CoalesceSystem<'b> {
    pub fn new(inst_man: &'b mut ShapeInstanceBuffer) -> Self {
        Self { inst_man }
    }
}
impl<'a, 'b> System<'a> for CoalesceSystem<'b> {
    type SystemData = (
        ReadStorage<'a, ShapeComponent>,
        ReadStorage<'a, ShapeTransformBuffer>,
        ReadStorage<'a, ShapeFlagBuffer>,
    );

    fn run(&mut self, (shapes, transform_buffers, flag_buffers): Self::SystemData) {
        for (shape, transform_buffer, flag_buffer) in
            (&shapes, &transform_buffers, &flag_buffers).join()
        {
            self.inst_man
                .push_values(shape.slot_id, &transform_buffer.buffer, flag_buffer.buffer);
        }
    }
}
