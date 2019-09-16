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
use shape_chunk::{DrawState, ShapeErrata, ShapeId, ShapeWidgets};
use specs::{Component, VecStorage};
use std::sync::{Arc, RwLock};

pub struct ShapeMesh {
    shape_id: ShapeId,
    draw_state: DrawState,
}

impl Component for ShapeMesh {
    type Storage = VecStorage<Self>;
}

impl ShapeMesh {
    pub fn new(shape_id: ShapeId) -> Self {
        Self {
            shape_id,
            draw_state: Default::default(),
        }
    }

    pub fn shape_id(&self) -> ShapeId {
        self.shape_id
    }

    pub fn draw_state(&self) -> &DrawState {
        &self.draw_state
    }
}

pub struct ShapeMeshTransformBuffer {
    pub buffer: [f32; 6],
}
impl Component for ShapeMeshTransformBuffer {
    type Storage = VecStorage<Self>;
}
impl ShapeMeshTransformBuffer {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self { buffer: [0f32; 6] }
    }
}

pub struct ShapeMeshFlagBuffer {
    pub buffer: [u32; 2],
    pub errata: ShapeErrata,
}
impl Component for ShapeMeshFlagBuffer {
    type Storage = VecStorage<Self>;
}
impl ShapeMeshFlagBuffer {
    pub fn new(errata: ShapeErrata) -> Self {
        Self {
            buffer: [0u32; 2],
            errata,
        }
    }
}

pub struct ShapeMeshXformBuffer {
    pub shape_id: ShapeId,
    pub buffer: Vec<f32>,
    pub widgets: Arc<RwLock<ShapeWidgets>>,
}
impl Component for ShapeMeshXformBuffer {
    type Storage = VecStorage<Self>;
}
impl ShapeMeshXformBuffer {
    pub fn new(shape_id: ShapeId, widgets: Arc<RwLock<ShapeWidgets>>) -> Self {
        let num_floats = widgets.read().unwrap().num_transformer_floats();
        let mut buffer = Vec::with_capacity(num_floats);
        buffer.resize(num_floats, 0f32);
        Self {
            shape_id,
            buffer,
            widgets,
        }
    }
}
