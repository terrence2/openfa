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
use nitrous::{inject_nitrous_component, NitrousComponent};

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

#[derive(NitrousComponent, Clone, Copy, Debug, PartialEq)]
#[Name = "scale"]
pub struct ShapeScale {
    #[property]
    scale: f64,
}

#[inject_nitrous_component]
impl ShapeScale {
    pub fn new(scale: f64) -> Self {
        Self { scale }
    }

    // Convert to dense pack for upload.
    pub fn compact(self) -> [f32; 1] {
        [self.scale as f32]
    }

    pub fn scale(&self) -> f32 {
        self.scale as f32
    }
}
