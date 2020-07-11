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
use nalgebra::{Point3, RealField};

#[derive(Clone, Copy, Debug)]
pub struct Sphere<T: RealField> {
    center: Point3<T>,
    radius: T,
}

impl<T: RealField> Sphere<T> {
    pub fn from_center_and_radius(center: &Point3<T>, radius: T) -> Self {
        Self {
            center: *center,
            radius,
        }
    }

    pub fn center(&self) -> &Point3<T> {
        &self.center
    }

    pub fn radius(&self) -> T {
        self.radius
    }
}
