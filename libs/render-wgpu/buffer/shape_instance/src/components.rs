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
use crate::SlotId;
use shape_chunk::{DrawState, ShapeErrata, ShapeId};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapeRefComp {
    pub shape_id: ShapeId,
}

impl ShapeRefComp {
    pub fn new(shape_id: ShapeId) -> Self {
        Self { shape_id }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapeComponent {
    pub slot_id: SlotId,
    pub draw_state: DrawState,
}
impl ShapeComponent {
    pub fn new(slot_id: SlotId, errata: ShapeErrata) -> Self {
        Self {
            slot_id,
            draw_state: DrawState::new(errata),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapeTransformBuffer {
    pub buffer: [f32; 6],
}
impl Default for ShapeTransformBuffer {
    fn default() -> Self {
        Self { buffer: [0f32; 6] }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapeFlagBuffer {
    pub buffer: [u32; 2],
}
impl Default for ShapeFlagBuffer {
    fn default() -> Self {
        Self { buffer: [0u32; 2] }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapeXformBuffer {
    pub buffer: [[f32; 6]; 14],
}
impl Default for ShapeXformBuffer {
    fn default() -> Self {
        //let num_floats = widgets.read().unwrap().num_transformer_floats();
        //let mut buffer = Vec::with_capacity(num_floats);
        //buffer.resize(num_floats, 0f32);
        Self {
            buffer: [[0f32; 6]; 14],
        }
    }
}
