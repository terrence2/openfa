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
use shape_chunk::{DrawState, ShapeId};
use specs::{Component, VecStorage};

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
