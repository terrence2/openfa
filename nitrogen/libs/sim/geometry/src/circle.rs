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
use crate::algorithm::perpendicular_vector;
use crate::Plane;
use nalgebra::{Point3, RealField, Unit, UnitQuaternion};

#[derive(Debug, Copy, Clone)]
pub struct Circle<T: RealField> {
    plane: Plane<T>,
    center: Point3<T>,
    radius: T,
}

impl<T: RealField> Circle<T> {
    pub fn from_plane_center_and_radius(plane: &Plane<T>, center: &Point3<T>, radius: T) -> Self {
        Self {
            plane: *plane,
            center: *center,
            radius,
        }
    }

    pub fn radius(&self) -> T {
        self.radius
    }

    pub fn center(&self) -> &Point3<T> {
        &self.center
    }

    pub fn plane(&self) -> &Plane<T> {
        &self.plane
    }

    pub fn point_at_angle(&self, angle: T) -> Point3<T> {
        // Find a vector at 90 degrees to the plane normal.
        let p0 = perpendicular_vector(self.plane.normal());
        let p = p0 * self.radius;
        let q = UnitQuaternion::from_axis_angle(&Unit::new_unchecked(*self.plane.normal()), angle);
        self.center + (q * p)
    }
}
