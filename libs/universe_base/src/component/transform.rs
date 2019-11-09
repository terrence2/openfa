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
use nalgebra::{Point3, UnitQuaternion};
use specs::{Component, VecStorage};

pub struct Transform {
    #[allow(dead_code)]
    pub position: Point3<f64>,
    #[allow(dead_code)]
    rotation: UnitQuaternion<f64>,
    // scale: Vector3<f64>, // we don't have an upload slot for this currently.
}

impl Component for Transform {
    type Storage = VecStorage<Self>;
}

impl Transform {
    pub fn new(position: Point3<f64>) -> Self {
        Self {
            position,
            rotation: UnitQuaternion::identity(),
        }
    }

    // Convert to dense pack for upload.
    pub fn compact(&self) -> [f32; 6] {
        [
            self.position.x as f32,
            self.position.y as f32,
            self.position.z as f32,
            0f32,
            0f32,
            0f32,
        ]
    }
}
