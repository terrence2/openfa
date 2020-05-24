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
use nalgebra::{convert, RealField, Vector3};

pub struct Face<T: RealField> {
    pub indices: [u8; 3],
    pub normal: Vector3<T>,
    pub siblings: [[u8; 2]; 3], // 0-1, 1-2, 2-0
}

impl<T: RealField> Face<T> {
    pub fn new(indices: [u8; 3], verts: &[Vector3<T>], siblings: [[u8; 2]; 3]) -> Self {
        let v0 = &verts[indices[0] as usize];
        let v1 = &verts[indices[1] as usize];
        let v2 = &verts[indices[2] as usize];
        let normal = (v1 - v0).cross(&(v2 - v0)).normalize();
        Face {
            indices,
            normal,
            siblings,
        }
    }

    pub fn i0(&self) -> usize {
        self.indices[0] as usize
    }

    pub fn i1(&self) -> usize {
        self.indices[1] as usize
    }

    pub fn i2(&self) -> usize {
        self.indices[2] as usize
    }

    #[allow(unused)]
    pub fn edge(&self, i: usize) -> [usize; 2] {
        match i {
            0 => [self.i0(), self.i1()],
            1 => [self.i1(), self.i2()],
            2 => [self.i2(), self.i0()],
            _ => unreachable!(),
        }
    }

    pub fn sibling(&self, i: usize) -> (usize, u8) {
        (self.siblings[i][0] as usize, self.siblings[i][1])
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
        let verts = vec![
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

        let faces = vec![
            // -- 5 faces around point 0
            /* 0 */
            Face::new([0, 11, 5], &verts, [[4, 2], [6, 0], [1, 0]]),
            /* 1 */ Face::new([0, 5, 1], &verts, [[0, 2], [5, 0], [2, 0]]),
            /* 2 */ Face::new([0, 1, 7], &verts, [[1, 2], [9, 0], [3, 0]]),
            /* 3 */ Face::new([0, 7, 10], &verts, [[2, 2], [8, 0], [4, 0]]),
            /* 4 */ Face::new([0, 10, 11], &verts, [[3, 2], [7, 0], [0, 0]]),
            // -- 5 adjacent faces
            /* 5 */
            Face::new([1, 5, 9], &verts, [[1, 1], [15, 1], [19, 2]]),
            /* 6 */ Face::new([5, 11, 4], &verts, [[0, 1], [16, 1], [15, 2]]),
            /* 7 */ Face::new([11, 10, 2], &verts, [[4, 1], [17, 1], [16, 2]]),
            /* 8 */ Face::new([10, 7, 6], &verts, [[3, 1], [18, 1], [17, 2]]),
            /* 9 */ Face::new([7, 1, 8], &verts, [[2, 1], [19, 1], [18, 2]]),
            // -- 5 faces around point 3
            /* 10 */
            Face::new([3, 9, 4], &verts, [[14, 2], [15, 0], [11, 0]]),
            /* 11 */ Face::new([3, 4, 2], &verts, [[10, 2], [16, 0], [12, 0]]),
            /* 12 */ Face::new([3, 2, 6], &verts, [[11, 2], [17, 0], [13, 0]]),
            /* 13 */ Face::new([3, 6, 8], &verts, [[12, 2], [18, 0], [14, 0]]),
            /* 14 */ Face::new([3, 8, 9], &verts, [[13, 2], [19, 0], [10, 0]]),
            // -- 5 adjacent faces
            /* 15 */
            Face::new([4, 9, 5], &verts, [[10, 1], [5, 1], [6, 2]]),
            /* 16 */ Face::new([2, 4, 11], &verts, [[11, 1], [6, 1], [7, 2]]),
            /* 17 */ Face::new([6, 2, 10], &verts, [[12, 1], [7, 1], [8, 2]]),
            /* 18 */ Face::new([8, 6, 7], &verts, [[13, 1], [8, 1], [9, 2]]),
            /* 19 */ Face::new([9, 8, 1], &verts, [[14, 1], [9, 1], [5, 2]]),
        ];

        Self { verts, faces }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn icosahedron_peer_linkage() {
        let ico = Icosahedron::<f64>::new();
        assert_eq!(ico.verts.len(), 12);
        assert_eq!(ico.faces.len(), 20);

        for (i, face) in ico.faces.iter().enumerate() {
            println!("at face: {:?}", i);
            for (j, [sib, peer_edge, ..]) in face.siblings.iter().enumerate() {
                assert_eq!(
                    face.edge(j)[0],
                    ico.faces[*sib as usize].edge(*peer_edge as usize)[1]
                );
                assert_eq!(
                    face.edge(j)[1],
                    ico.faces[*sib as usize].edge(*peer_edge as usize)[0]
                );
            }
        }
    }
}
