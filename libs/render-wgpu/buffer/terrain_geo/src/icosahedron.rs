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
#![allow(unused)]

use nalgebra::{convert, RealField, Vector3};

pub struct Face<T: RealField> {
    pub index0: u32,
    pub index1: u32,
    pub index2: u32,
    pub normal: Vector3<T>,
    pub siblings: [usize; 3], // 0-1, 1-2, 2-0
}

impl<T: RealField> Face<T> {
    pub fn new(
        i0: u32,
        i1: u32,
        i2: u32,
        verts: &[Vector3<T>],
        sib01: usize,
        sib12: usize,
        sib20: usize,
    ) -> Self {
        let v0 = &verts[i0 as usize];
        let v1 = &verts[i1 as usize];
        let v2 = &verts[i2 as usize];
        let normal = (v1 - v0).cross(&(v2 - v0)).normalize();
        Face {
            index0: i0,
            index1: i1,
            index2: i2,
            normal,
            siblings: [sib01, sib12, sib20],
        }
    }

    pub fn i0(&self) -> usize {
        self.index0 as usize
    }

    pub fn i1(&self) -> usize {
        self.index1 as usize
    }

    pub fn i2(&self) -> usize {
        self.index2 as usize
    }
}

pub struct Icosahedron<T: RealField> {
    pub verts: Vec<Vector3<T>>,
    pub faces: Vec<Face<T>>,
}

impl<T: RealField> Icosahedron<T> {
    pub fn new() -> Self {
        let t = (T::one() + convert::<f64, T>(5.0).sqrt()) / convert::<f64, T>(2.0);

        // The bones of the d12 are 3 orthogonal quads at the origin.
        let mut verts = vec![
            Vector3::new(-T::one(), t, T::zero()).normalize(),
            Vector3::new(T::one(), t, T::zero()).normalize(),
            Vector3::new(-T::one(), -t, T::zero()).normalize(),
            Vector3::new(T::one(), -t, T::zero()).normalize(),
            Vector3::new(T::zero(), -T::one(), t).normalize(),
            Vector3::new(T::zero(), T::one(), t).normalize(),
            Vector3::new(T::zero(), -T::one(), -t).normalize(),
            Vector3::new(T::zero(), T::one(), -t).normalize(),
            Vector3::new(t, T::zero(), -T::one()).normalize(),
            Vector3::new(t, T::zero(), T::one()).normalize(),
            Vector3::new(-t, T::zero(), -T::one()).normalize(),
            Vector3::new(-t, T::zero(), T::one()).normalize(),
        ];

        let mut faces = vec![
            // -- 5 faces around point 0
            /* 0 */
            Face::new(0, 11, 5, &verts, 4, 6, 1),
            /* 1 */ Face::new(0, 5, 1, &verts, 0, 5, 2),
            /* 2 */ Face::new(0, 1, 7, &verts, 1, 9, 3),
            /* 3 */ Face::new(0, 7, 10, &verts, 2, 8, 4),
            /* 4 */ Face::new(0, 10, 11, &verts, 3, 7, 0),
            // -- 5 adjacent faces
            /* 5 */ Face::new(1, 5, 9, &verts, 1, 15, 19),
            /* 6 */ Face::new(5, 11, 4, &verts, 0, 16, 15),
            /* 7 */ Face::new(11, 10, 2, &verts, 4, 17, 16),
            /* 8 */ Face::new(10, 7, 6, &verts, 3, 18, 17),
            /* 9 */ Face::new(7, 1, 8, &verts, 2, 19, 18),
            // -- 5 faces around point 3
            /* 10 */
            Face::new(3, 9, 4, &verts, 14, 15, 11),
            /* 11 */ Face::new(3, 4, 2, &verts, 10, 16, 12),
            /* 12 */ Face::new(3, 2, 6, &verts, 11, 17, 13),
            /* 13 */ Face::new(3, 6, 8, &verts, 12, 18, 14),
            /* 14 */ Face::new(3, 8, 9, &verts, 13, 19, 10),
            // -- 5 adjacent faces
            /* 15 */ Face::new(4, 9, 5, &verts, 10, 5, 6),
            /* 16 */ Face::new(2, 4, 11, &verts, 11, 6, 7),
            /* 17 */ Face::new(6, 2, 10, &verts, 12, 7, 8),
            /* 18 */ Face::new(8, 6, 7, &verts, 13, 8, 9),
            /* 19 */ Face::new(9, 8, 1, &verts, 14, 9, 5),
        ];

        Self { verts, faces }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn it_can_be_created() {
        let ico = Icosahedron::<f64>::new();
        assert_eq!(ico.verts.len(), 12);
        assert_eq!(ico.faces.len(), 20);
    }
}
