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
use crate::{Circle, Plane, Sphere};
use nalgebra::RealField;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum PlaneSide {
    Above,
    On,
    Below,
}

impl PlaneSide {
    pub fn from_distance<T: RealField>(d: T) -> Self {
        if d < T::zero() {
            PlaneSide::Below
        } else if d > T::zero() {
            PlaneSide::Above
        } else {
            PlaneSide::On
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum SpherePlaneIntersection<T: RealField> {
    NoIntersection {
        distance: T,     // Closest distance between the sphere and the plane
        side: PlaneSide, // What side of the plane is the sphere on.
    },
    Intersection(Circle<T>),
}

impl<T: RealField> SpherePlaneIntersection<T> {
    pub fn is_intersecting(&self) -> bool {
        match self {
            Self::NoIntersection { .. } => false,
            Self::Intersection(_) => true,
        }
    }
}

pub fn sphere_vs_plane<T: RealField>(
    sphere: &Sphere<T>,
    plane: &Plane<T>,
) -> SpherePlaneIntersection<T> {
    let dist = plane.distance_to_point(sphere.center());

    if dist.abs() < sphere.radius() {
        let to_sphere = sphere.radius() - ((sphere.radius() - dist.abs()) * -dist.signum());
        let center = sphere.center() + plane.normal() * to_sphere;
        return SpherePlaneIntersection::Intersection(Circle::from_plane_center_and_radius(
            &Plane::from_point_and_normal(&center, plane.normal()),
            &center,
            (sphere.radius() * sphere.radius() - dist * dist).sqrt(),
        ));
    }
    SpherePlaneIntersection::NoIntersection {
        distance: dist.abs() - sphere.radius(),
        side: PlaneSide::from_distance(dist),
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use approx::assert_relative_eq;
    use nalgebra::{Point3, Vector3};

    #[test]
    fn test_sphere_fully_above_plane() {
        let sphere = Sphere::from_center_and_radius(&Point3::new(0f64, 0f64, 0f64), 1f64);
        let plane = Plane::from_point_and_normal(
            &Point3::new(100f64, -2f64, 100f64),
            &Vector3::new(0f64, 1f64, 0f64),
        );

        let intersect = sphere_vs_plane(&sphere, &plane);
        assert!(!intersect.is_intersecting());
        match intersect {
            SpherePlaneIntersection::NoIntersection { distance, side } => {
                assert_relative_eq!(distance, 1f64);
                assert_eq!(side, PlaneSide::Above);
            }
            SpherePlaneIntersection::Intersection(_) => panic!("intersecting?"),
        }
    }

    #[test]
    fn test_sphere_fully_below_plane() {
        let sphere = Sphere::from_center_and_radius(&Point3::new(0f64, 0f64, 0f64), 1f64);
        let plane = Plane::from_point_and_normal(
            &Point3::new(100f64, 2f64, 100f64),
            &Vector3::new(0f64, 1f64, 0f64),
        );

        let intersect = sphere_vs_plane(&sphere, &plane);
        assert!(!intersect.is_intersecting());
        match intersect {
            SpherePlaneIntersection::NoIntersection { distance, side } => {
                assert_relative_eq!(distance, 1f64);
                assert_eq!(side, PlaneSide::Below);
            }
            SpherePlaneIntersection::Intersection(_) => panic!("intersecting?"),
        }
    }

    #[test]
    fn test_sphere_on_plane() {
        let sphere = Sphere::from_center_and_radius(&Point3::new(0f64, 0f64, 0f64), 1f64);
        let plane = Plane::from_point_and_normal(
            &Point3::new(100f64, 0f64, 100f64),
            &Vector3::new(0f64, 1f64, 0f64),
        );

        let intersect = sphere_vs_plane(&sphere, &plane);
        assert!(intersect.is_intersecting());
        match intersect {
            SpherePlaneIntersection::NoIntersection { .. } => panic!("non-intersecting?"),
            SpherePlaneIntersection::Intersection(ref circle) => {
                assert_relative_eq!(circle.radius(), 1f64);
            }
        }
    }

    #[test]
    fn test_sphere_above_plane() {
        let sphere = Sphere::from_center_and_radius(&Point3::new(0f64, 0f64, 0f64), 1f64);
        let plane = Plane::from_point_and_normal(
            &Point3::new(100f64, -0.5f64, 100f64),
            &Vector3::new(0f64, 1f64, 0f64),
        );

        let intersect = sphere_vs_plane(&sphere, &plane);
        assert!(intersect.is_intersecting());
        match intersect {
            SpherePlaneIntersection::NoIntersection { .. } => panic!("non-intersecting?"),
            SpherePlaneIntersection::Intersection(ref circle) => {
                assert_relative_eq!(circle.radius(), (0.5f64 / 1.0).acos().sin());
            }
        }
    }

    #[test]
    fn test_sphere_below_plane() {
        let sphere = Sphere::from_center_and_radius(&Point3::new(0f64, 0f64, 0f64), 1f64);
        let plane = Plane::from_point_and_normal(
            &Point3::new(100f64, 0.5f64, 100f64),
            &Vector3::new(0f64, 1f64, 0f64),
        );

        let intersect = sphere_vs_plane(&sphere, &plane);
        assert!(intersect.is_intersecting());
        match intersect {
            SpherePlaneIntersection::NoIntersection { .. } => panic!("non-intersecting?"),
            SpherePlaneIntersection::Intersection(ref circle) => {
                assert_relative_eq!(circle.radius(), (0.5f64 / 1.0).acos().sin());
            }
        }
    }
}
