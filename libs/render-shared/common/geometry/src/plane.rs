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
/*
use nalgebra::{Point3, Scalar, Vector3, Vector4};
use num_traits::Float;

#[derive(Debug)]
pub struct Plane<T: Scalar>(Vector4<T>);

impl<T: Scalar> Plane<T> {
    pub fn from_point_and_normal(p: Point3<T>, n: Vector3<T>) -> Self {
        let d = p.coords.dot(&n);
        Self(Vector4::new(p, d))
    }
}
*/
pub struct Plane;
