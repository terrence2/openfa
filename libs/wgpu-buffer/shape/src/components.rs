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
use crate::TransformType;
use bevy_ecs::prelude::*;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct ShapeTransformBuffer {
    pub buffer: TransformType,
}

#[derive(Component, Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ShapeFlagBuffer {
    pub buffer: [u32; 2],
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct ShapeXformBuffer {
    pub buffer: [[f32; 6]; 14],
}

#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct ShapeScale(f32);

impl ShapeScale {
    pub fn new(v: f32) -> Self {
        Self(v)
    }

    // Convert to dense pack for upload.
    pub fn compact(self) -> [f32; 1] {
        [self.0]
    }

    pub fn scale(&self) -> f32 {
        self.0
    }
}
