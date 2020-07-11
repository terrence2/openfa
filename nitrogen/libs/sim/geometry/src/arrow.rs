// This file is part of Nitrogen.
//
// Nitrogen is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Nitrogen is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with Nitrogen.  If not, see <http://www.gnu.org/licenses/>.
use nalgebra::{Point3, Vector3};

pub struct Face {
    pub index0: u32,
    pub index1: u32,
    pub index2: u32,
}

impl Face {
    pub fn new(index0: u32, index1: u32, index2: u32) -> Self {
        Self {
            index0,
            index1,
            index2,
        }
    }
}

pub struct Arrow {
    pub verts: Vec<Point3<f32>>,
    pub faces: Vec<Face>,
}

impl Arrow {
    pub fn new(base: Point3<f32>, dir: Vector3<f32>) -> Self {
        // Cross with any random vector to get something perpendicular.
        // Then cross again to get a 90 degree angle to first perpendicular.
        let tmp0 = if dir.y > dir.z {
            Vector3::new(0f32, 0f32, 1f32)
        } else {
            Vector3::new(0f32, 1f32, 0f32)
        };
        let off0 = dir.cross(&tmp0).normalize() * 0.5;
        let off1 = dir.cross(&off0).normalize() * 0.5;

        let verts = vec![
            base - (off0 * 0.5),
            base + (dir * 0.75),
            base + (off0 * 0.5),
            base - (off1 * 0.5),
            base + (dir * 0.75),
            base + (off1 * 0.5),
            base + (dir * 0.75) - off0,
            base + dir,
            base + (dir * 0.75) + off0,
            base + (dir * 0.75) - off1,
            base + dir,
            base + (dir * 0.75) + off1,
        ];

        let faces = vec![
            Face::new(0, 1, 2),
            Face::new(2, 1, 0),
            Face::new(3, 4, 5),
            Face::new(5, 4, 3),
            Face::new(6, 7, 8),
            Face::new(8, 7, 6),
            Face::new(9, 10, 11),
            Face::new(11, 10, 9),
        ];

        Self { verts, faces }
    }
}
