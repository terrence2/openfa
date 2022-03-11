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
use crate::{
    chunk::{DrawState, ShapeErrata, ShapeId},
    SlotId, TransformType,
};
use bevy_ecs::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapeSlot {
    pub slot_id: SlotId,
}
impl ShapeSlot {
    pub fn new(slot_id: SlotId) -> Self {
        Self { slot_id }
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct ShapeTransformBuffer {
    pub buffer: TransformType,
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct ShapeFlagBuffer {
    pub buffer: [u32; 2],
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct ShapeXformBuffer {
    pub buffer: [[f32; 6]; 14],
}
