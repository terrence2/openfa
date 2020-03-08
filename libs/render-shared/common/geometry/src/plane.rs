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
use nalgebra::{Point3, RealField, Vector3, Vector4};

#[derive(Debug)]
pub struct Plane<T: RealField>(Vector4<T>);

impl<T: RealField> Plane<T> {
    pub fn from_point_and_normal(p: Point3<T>, n: Vector3<T>) -> Self {
        let d = p.coords.dot(&n);
        Self(Vector4::new(p.coords[0], p.coords[1], p.coords[2], d))
    }
}
