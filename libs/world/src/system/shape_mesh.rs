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
use crate::component::shape_mesh::*;
use shape_chunk::{ShapeId, ShapeWidgets};
use specs::prelude::*;
use std::{
    cell::RefCell,
    collections::{hash_map::Entry, HashMap},
    time::Instant,
};

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
                            let mut widgets = xform_buffer.widgets.read().unwrap().clone();
                            widgets
                                .animate_into(
                                    shape_mesh.draw_state(),
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

pub struct XformCoalesceSystem {
    linear_buffer: Vec<f32>,
}
impl XformCoalesceSystem {
    pub fn new() -> Self {
        Self {
            linear_buffer: Vec::new(),
        }
    }
}
impl<'a> System<'a> for XformCoalesceSystem {
    type SystemData = (ReadStorage<'a, ShapeMeshXformBuffer>,);

    fn run(&mut self, (xform_buffers,): Self::SystemData) {
        let mut cnt = 0;
        for (xform_buffer,) in (&xform_buffers,).join() {
            cnt += xform_buffer.buffer.len();
        }
        self.linear_buffer.resize(cnt, 0f32);
        let mut offset = 0;
        for (xform_buffer,) in (&xform_buffers,).join() {
            let sz = xform_buffer.buffer.len();
            self.linear_buffer[offset..offset + sz].copy_from_slice(&xform_buffer.buffer);
            offset += sz;
        }
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

pub struct FlagCoalesceSystem {
    linear_buffer: Vec<u32>,
}
impl FlagCoalesceSystem {
    pub fn new() -> Self {
        Self {
            linear_buffer: Vec::new(),
        }
    }
}
impl<'a> System<'a> for FlagCoalesceSystem {
    type SystemData = (ReadStorage<'a, ShapeMeshFlagBuffer>,);

    fn run(&mut self, (flag_buffers,): Self::SystemData) {
        let mut cnt = 0;
        for (_flag_buffer,) in (&flag_buffers,).join() {
            cnt += 2;
        }
        self.linear_buffer.resize(cnt, 0u32);
        let mut offset = 0;
        for (flag_buffer,) in (&flag_buffers,).join() {
            self.linear_buffer[offset] = flag_buffer.buffer[0];
            self.linear_buffer[offset + 1] = flag_buffer.buffer[1];
            offset += 2;
        }
    }
}
