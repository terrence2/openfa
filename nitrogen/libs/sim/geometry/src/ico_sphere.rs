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
use crate::algorithm::bisect_edge;
use nalgebra::Vector3;

pub struct Face {
    pub index0: u32,
    pub index1: u32,
    pub index2: u32,
    pub normal: Vector3<f64>,
}

impl Face {
    pub fn new(i0: u32, i1: u32, i2: u32, verts: &[Vector3<f64>]) -> Self {
        let v0 = &verts[i0 as usize];
        let v1 = &verts[i1 as usize];
        let v2 = &verts[i2 as usize];
        let normal = (v1 - v0).cross(&(v2 - v0)).normalize();
        Face {
            index0: i0,
            index1: i1,
            index2: i2,
            normal,
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

pub struct IcoSphere {
    pub verts: Vec<Vector3<f64>>,
    pub faces: Vec<Face>,
}

impl IcoSphere {
    pub fn new(iterations: usize) -> Self {
        let t = (1f64 + 5f64.sqrt()) / 2f64;

        // The bones of the d12 are 3 orthogonal quads at the origin.
        let mut verts = vec![
            Vector3::new(-1f64, t, 0f64).normalize(),
            Vector3::new(1f64, t, 0f64).normalize(),
            Vector3::new(-1f64, -t, 0f64).normalize(),
            Vector3::new(1f64, -t, 0f64).normalize(),
            Vector3::new(0f64, -1f64, t).normalize(),
            Vector3::new(0f64, 1f64, t).normalize(),
            Vector3::new(0f64, -1f64, -t).normalize(),
            Vector3::new(0f64, 1f64, -t).normalize(),
            Vector3::new(t, 0f64, -1f64).normalize(),
            Vector3::new(t, 0f64, 1f64).normalize(),
            Vector3::new(-t, 0f64, -1f64).normalize(),
            Vector3::new(-t, 0f64, 1f64).normalize(),
        ];

        let mut faces = vec![
            // 5 faces around point 0
            Face::new(0, 11, 5, &verts),
            Face::new(0, 5, 1, &verts),
            Face::new(0, 1, 7, &verts),
            Face::new(0, 7, 10, &verts),
            Face::new(0, 10, 11, &verts),
            // 5 adjacent faces
            Face::new(1, 5, 9, &verts),
            Face::new(5, 11, 4, &verts),
            Face::new(11, 10, 2, &verts),
            Face::new(10, 7, 6, &verts),
            Face::new(7, 1, 8, &verts),
            // 5 faces around point 3
            Face::new(3, 9, 4, &verts),
            Face::new(3, 4, 2, &verts),
            Face::new(3, 2, 6, &verts),
            Face::new(3, 6, 8, &verts),
            Face::new(3, 8, 9, &verts),
            // 5 adjacent faces
            Face::new(4, 9, 5, &verts),
            Face::new(2, 4, 11, &verts),
            Face::new(6, 2, 10, &verts),
            Face::new(8, 6, 7, &verts),
            Face::new(9, 8, 1, &verts),
        ];

        // Subdivide repeatedly to get a spherical object.
        for _ in 0..iterations {
            let mut next_faces = Vec::new();
            for face in &faces {
                let a = bisect_edge(&verts[face.i0()], &verts[face.i1()]).normalize();
                let b = bisect_edge(&verts[face.i1()], &verts[face.i2()]).normalize();
                let c = bisect_edge(&verts[face.i2()], &verts[face.i0()]).normalize();

                let ia = verts.len() as u32;
                verts.push(a);
                let ib = verts.len() as u32;
                verts.push(b);
                let ic = verts.len() as u32;
                verts.push(c);

                next_faces.push(Face::new(face.index0, ia, ic, &verts));
                next_faces.push(Face::new(face.index1, ib, ia, &verts));
                next_faces.push(Face::new(face.index2, ic, ib, &verts));
                next_faces.push(Face::new(ia, ib, ic, &verts));
            }
            faces = next_faces;
        }

        IcoSphere { verts, faces }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn it_can_be_created() {
        let ico = IcoSphere::new(0);
        assert_eq!(ico.verts.len(), 12);
        assert_eq!(ico.faces.len(), 20);
    }

    #[test]
    fn it_can_create_spheres() {
        let ico = IcoSphere::new(2);
        assert!(ico.verts.len() > 12);
        assert!(ico.faces.len() > 20);
    }
}
