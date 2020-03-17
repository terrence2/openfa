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
use approx::relative_eq;
use nalgebra::{Point3, RealField, Vector3};

#[derive(Clone, Copy, Debug)]
pub struct Plane<T: RealField> {
    normal: Vector3<T>,
    d: T,
}

impl<T: RealField> Plane<T> {
    pub fn from_point_and_normal(p: &Point3<T>, n: &Vector3<T>) -> Self {
        Self {
            normal: n.to_owned(),
            d: p.coords.dot(n),
        }
    }

    pub fn from_normal_and_distance(normal: Vector3<T>, d: T) -> Self {
        Self { normal, d }
    }

    pub fn point_on_plane(&self, p: &Point3<T>) -> bool {
        relative_eq!(self.normal.dot(&p.coords) - self.d, T::zero())
    }

    pub fn distance(&self, p: &Point3<T>) -> T {
        self.normal.dot(&p.coords) - self.d
    }

    pub fn closest_point_on_plane(&self, p: &Point3<T>) -> Point3<T> {
        p - (self.normal * self.distance(p))
    }

    pub fn normal(&self) -> &Vector3<T> {
        &self.normal
    }

    pub fn d(&self) -> T {
        self.d
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn test_point_on_plane() {
        let plane = Plane::from_point_and_normal(
            &Point3::new(0f64, 0f64, 0f64),
            &Vector3::new(0f64, 0f64, 1f64),
        );
        assert!(plane.point_on_plane(&Point3::new(10f64, 10f64, 0f64)));
        assert!(!plane.point_on_plane(&Point3::new(10f64, 10f64, 0.1f64)));
        assert!(!plane.point_on_plane(&Point3::new(10f64, 10f64, -0.1f64)));
    }

    #[test]
    fn test_point_distance() {
        let plane = Plane::from_point_and_normal(
            &Point3::new(0f64, 0f64, 0f64),
            &Vector3::new(0f64, 0f64, 1f64),
        );

        assert_relative_eq!(-1f64, plane.distance(&Point3::new(1f64, 1f64, -1f64)));
        assert_relative_eq!(1f64, plane.distance(&Point3::new(-1f64, -1f64, 1f64)));
    }

    #[test]
    fn test_closest_point_on_plane() {
        let plane = Plane::from_point_and_normal(
            &Point3::new(0f64, 0f64, 0f64),
            &Vector3::new(0f64, 0f64, 1f64),
        );

        assert_relative_eq!(
            Point3::new(1f64, 1f64, 0f64),
            plane.closest_point_on_plane(&Point3::new(1f64, 1f64, -1f64))
        );
        assert_relative_eq!(
            Point3::new(-1f64, -1f64, 0f64),
            plane.closest_point_on_plane(&Point3::new(-1f64, -1f64, 1f64))
        );
    }
}
