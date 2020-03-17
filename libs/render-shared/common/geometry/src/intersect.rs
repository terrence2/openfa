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
use crate::{Circle, Plane, Sphere};
use nalgebra::RealField;

pub struct SpherePlaneIntersection<T: RealField> {
    pub distance: T,                     // Above or below
    pub intersection: Option<Circle<T>>, // some if abs(dist) < sphere.radius, else none
}

pub fn sphere_vs_plane<T: RealField>(
    sphere: &Sphere<T>,
    plane: &Plane<T>,
) -> SpherePlaneIntersection<T> {
    let dist = plane.distance(sphere.center());

    let intersection = if dist.abs() < sphere.radius() {
        Some(Circle::from_plane_and_radius(
            &Plane::from_point_and_normal(
                &(sphere.center() + plane.normal() * dist),
                plane.normal(),
            ),
            (sphere.radius() * sphere.radius() + dist * dist).sqrt(),
        ))
    } else {
        None
    };

    //panic!("-> hard work goes here <-")
    SpherePlaneIntersection {
        distance: dist,
        intersection,
    }
}
