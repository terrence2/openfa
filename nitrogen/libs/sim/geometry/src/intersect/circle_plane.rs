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
use crate::{Circle, Plane};
use approx::relative_eq;
use nalgebra::{Point3, RealField};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CirclePlaneIntersection<T: RealField> {
    Parallel,
    InFrontOfPlane,
    BehindPlane,
    Intersection(Point3<T>, Point3<T>),
    Tangent(Point3<T>),
}

pub fn circle_vs_plane<T: RealField>(
    circle: &Circle<T>,
    plane: &Plane<T>,
    sidedness_offset: T,
) -> CirclePlaneIntersection<T> {
    // We can get the direction by crossing normals.
    let d = circle.plane().normal().cross(&plane.normal());

    // Detect and reject the parallel case: e.g. direction is ~0.
    if relative_eq!(d.dot(&d), T::zero()) {
        return CirclePlaneIntersection::<T>::Parallel;
    }
    let d = d.normalize();

    // Find the line: the line is orthogonal to both normals and has direction d.
    // Taken from the clever code here:
    //   https://stackoverflow.com/questions/6408670/line-of-intersection-between-two-planes
    let p = (d.cross(plane.normal()) * circle.plane().d())
        + (circle.plane().normal().cross(&d) * plane.d());

    // Project circle center onto new line.
    let t = (circle.center() - p).coords.dot(&d);
    let p_closest = Point3::from(p + d * t);
    let closest_distance = (circle.center() - p_closest).magnitude();
    if closest_distance > circle.radius() {
        return if plane.point_is_in_front_with_offset(circle.center(), sidedness_offset) {
            CirclePlaneIntersection::InFrontOfPlane
        } else {
            CirclePlaneIntersection::BehindPlane
        };
    }
    if relative_eq!(closest_distance, circle.radius()) {
        return CirclePlaneIntersection::Tangent(p_closest);
    }

    // Apply pythagoras to get the distance from p_closest to our two roots.
    let t1 = (circle.radius() * circle.radius() - closest_distance * closest_distance).sqrt();
    CirclePlaneIntersection::Intersection(p_closest + d * t1, p_closest - d * t1)
}

#[cfg(test)]
mod test {
    use super::*;
    use nalgebra::{Point3, Vector3};

    #[test]
    fn it_can_handle_two_points() {
        let c = Circle::from_plane_center_and_radius(
            &Plane::from_point_and_normal(
                &Point3::new(0f64, 0f64, 0f64),
                &Vector3::new(0f64, 1f64, 0f64), // facing up
            ),
            &Point3::new(0f64, 0f64, 0f64), // center at origin
            1f64,
        );
        let p = Plane::from_point_and_normal(
            &Point3::new(-0.5f64, 0f64, 0f64), // offset 1/2 of radius
            &Vector3::new(-1f64, 0f64, 0f64).normalize(), // facing left
        );

        // From top down:
        //   _
        // /_|  .
        // \ | /|
        //   - -- <- ?? on z axis
        //   /\ -0.5 on x axis

        // -0.5**2 + ??**2 = 1
        // 1 - 0.25 = ??**2
        // sqrt(0.75) = ??
        // = 0.866

        let i = circle_vs_plane(&c, &p, 0f64);
        println!("i: {:?}", i);
        assert_eq!(
            i,
            CirclePlaneIntersection::Intersection(
                Point3::new(-0.5f64, 0f64, 0.75f64.sqrt()),
                Point3::new(-0.5f64, 0f64, -(0.75f64.sqrt()))
            )
        );
    }

    #[test]
    fn it_can_handle_incident_points() {
        let c = Circle::from_plane_center_and_radius(
            &Plane::from_point_and_normal(
                &Point3::new(0f64, 0f64, 0f64),
                &Vector3::new(0f64, 1f64, 0f64),
            ),
            &Point3::new(0f64, 0f64, 0f64),
            1f64,
        );
        let p = Plane::from_point_and_normal(
            &Point3::new(1f64, 0f64, 0f64),
            &Vector3::new(-1f64, 0f64, 0f64),
        );

        let i = circle_vs_plane(&c, &p, 0f64);
        assert_eq!(
            i,
            CirclePlaneIntersection::Tangent(Point3::new(1f64, 0f64, 0f64))
        );
    }

    #[test]
    fn it_can_handle_zero_points() {
        let c = Circle::from_plane_center_and_radius(
            &Plane::from_point_and_normal(
                &Point3::new(0f64, 0f64, 0f64),
                &Vector3::new(0f64, 1f64, 0f64),
            ),
            &Point3::new(0f64, 0f64, 0f64),
            1f64,
        );
        let p = Plane::from_point_and_normal(
            &Point3::new(10f64, 0f64, 0f64),
            &Vector3::new(1f64, 0f64, 0f64).normalize(),
        );
        let i = circle_vs_plane(&c, &p, 0f64);
        assert_eq!(i, CirclePlaneIntersection::BehindPlane);

        let p = Plane::from_point_and_normal(
            &Point3::new(10f64, 0f64, 0f64),
            &Vector3::new(-1f64, 0f64, 0f64).normalize(),
        );
        let i = circle_vs_plane(&c, &p, 0f64);
        assert_eq!(i, CirclePlaneIntersection::InFrontOfPlane);
    }

    #[test]
    fn it_can_handle_parallel_planes() {
        let c = Circle::from_plane_center_and_radius(
            &Plane::from_point_and_normal(
                &Point3::new(0f64, 0f64, 0f64),
                &Vector3::new(0f64, 1f64, 0f64),
            ),
            &Point3::new(0f64, 0f64, 0f64),
            1f64,
        );
        let p = Plane::from_point_and_normal(
            &Point3::new(0f64, 1f64, 0f64),
            &Vector3::new(0f64, 1f64, 0f64),
        );
        // Point is a 1 up, why is d -1?

        let i = circle_vs_plane(&c, &p, 0f64);
        assert_eq!(i, CirclePlaneIntersection::Parallel);
    }
}
