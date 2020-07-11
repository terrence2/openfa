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
use nalgebra::{clamp, convert, Point3, RealField, Vector3};

// Note: this expects left-handed (e.g. clockwise winding).
pub fn solid_angle<T: RealField>(
    observer_position: &Point3<T>,
    observer_direction: &Vector3<T>,
    vertices: &[Point3<T>],
) -> T {
    // compute projected solid area using Stoke's theorem from Improving Radiosity Solutions
    // through the Use of Analytically Determined Form Factors by Baum, Rushmeier, and Winget
    // (Eq. 9 on pg. 6 (or "330"))
    assert!(vertices.len() > 2);

    // integrate over edges
    let mut projarea = T::zero();
    for (i, &v) in vertices.iter().enumerate() {
        let j = (i + 1) % vertices.len();
        let v0 = v - observer_position;
        let v1 = vertices[j] - observer_position;
        let mut tau = v0.cross(&v1);
        let v0 = v0.normalize();
        let v1 = v1.normalize();
        let dotp = clamp(v0.dot(&v1), convert(-1.0), convert(1.0));

        let gamma = dotp.acos();
        assert!(gamma.is_finite(), "triangle gamma is infinite");

        tau.normalize_mut();
        tau *= gamma;
        projarea -= observer_direction.dot(&tau);
    }

    projarea / T::two_pi()
}

// Note: Identical to above, but allows unrolling the vertices loop, shaving ~10% off the
// total execution time for the common case.
pub fn solid_angle_tri<T: RealField>(
    observer_position: &Point3<T>,
    observer_direction: &Vector3<T>,
    vertices: &[Point3<T>; 3],
) -> T {
    // compute projected solid area using Stoke's theorem from Improving Radiosity Solutions
    // through the Use of Analytically Determined Form Factors by Baum, Rushmeier, and Winget
    // (Eq. 9 on pg. 6 (or "330"))

    // integrate over edges
    let mut projarea = T::zero();
    for (i, &v) in vertices.iter().enumerate() {
        let j = (i + 1) % vertices.len();
        let v0 = v - observer_position;
        let v1 = vertices[j] - observer_position;
        let mut tau = v0.cross(&v1);
        let v0 = v0.normalize();
        let v1 = v1.normalize();
        let dotp = clamp(v0.dot(&v1), convert(-1.0), convert(1.0));

        let gamma = dotp.acos();
        assert!(gamma.is_finite(), "triangle gamma is infinite");

        tau.normalize_mut();
        tau *= gamma;
        projarea -= observer_direction.dot(&tau);
    }

    projarea / T::two_pi()
}

pub fn perpendicular_vector<T: RealField>(v: &Vector3<T>) -> Vector3<T> {
    let n = v.normalize();
    if n[2].abs() > T::from_f64(0.5).unwrap() {
        n.cross(&Vector3::new(T::zero(), T::one(), T::zero()))
            .normalize()
    } else {
        n.cross(&Vector3::new(T::zero(), T::zero(), T::one()))
            .normalize()
    }
}

pub fn compute_normal<T: RealField>(p0: &Point3<T>, p1: &Point3<T>, p2: &Point3<T>) -> Vector3<T> {
    (p1.coords - p0.coords)
        .cross(&(p2.coords - p0.coords))
        .normalize()
}

pub fn bisect_edge<T: RealField>(v0: &Vector3<T>, v1: &Vector3<T>) -> Vector3<T> {
    v0 + ((v1 - v0) / convert::<f64, T>(2f64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solid_angle_produces_proper_signs_per_winding_and_normal() {
        let p = Point3::new(0f64, 0f64, 0f64);
        let n = Vector3::new(0f64, 0f64, 1f64);
        let pts = [
            Point3::new(0f64, 1f64, 1f64),
            Point3::new(1f64, 0f64, 1f64),
            Point3::new(0f64, 0f64, 1f64),
        ];
        let sa = solid_angle(&p, &n, &pts);
        assert!(sa > 0f64);

        // Flipping the vector means we're pointing away from the face, so the SA should be negative.
        let n = Vector3::new(0f64, 0f64, -1f64);
        let sa = solid_angle(&p, &n, &pts);
        assert!(sa < 0f64);

        // Similarly, flipping the winding will also flip the sign, back to positive, event
        // though we're still facing away.
        let pts = [
            Point3::new(0f64, 0f64, 1f64),
            Point3::new(1f64, 0f64, 1f64),
            Point3::new(0f64, 1f64, 1f64),
        ];
        let sa = solid_angle(&p, &n, &pts);
        assert!(sa > 0f64);

        // But if we're actually pointing towards the face again, it is negative, as the face
        // is pointing away.
        let n = Vector3::new(0f64, 0f64, 1f64);
        let sa = solid_angle(&p, &n, &pts);
        assert!(sa < 0f64);
    }

    #[test]
    fn solid_angle_reduces_with_distance() {
        let p = Point3::new(0f64, 0f64, 0f64);
        let n = Vector3::new(0f64, 0f64, 1f64);
        let pts = [
            Point3::new(0f64, 1f64, 1f64),
            Point3::new(1f64, 0f64, 1f64),
            Point3::new(0f64, 0f64, 1f64),
        ];
        let sa0 = solid_angle(&p, &n, &pts);

        let p = Point3::new(0f64, 0f64, 0.5f64);
        let sa1 = solid_angle(&p, &n, &pts);

        assert!(sa0 < sa1);
    }

    #[test]
    fn solid_angle_scales_with_triangle_area_given_fixed_observer() {
        let p = Point3::new(0f64, 0f64, 0f64);
        let n = Vector3::new(0f64, 0f64, 1f64);
        let pts = [
            Point3::new(0f64, 1f64, 1f64),
            Point3::new(1f64, 0f64, 1f64),
            Point3::new(0f64, 0f64, 1f64),
        ];
        let sa0 = solid_angle(&p, &n, &pts);

        let pts = [
            Point3::new(0f64, 2f64, 1f64),
            Point3::new(2f64, 0f64, 1f64),
            Point3::new(0f64, 0f64, 1f64),
        ];
        let sa1 = solid_angle(&p, &n, &pts);

        assert!(sa0 < sa1);
    }

    #[test]
    fn it_works_32() {
        let p = Point3::new(0f32, 0f32, 0f32);
        let n = Vector3::new(0f32, 0f32, 1f32);
        let pts = [
            Point3::new(0f32, 0f32, 1f32),
            Point3::new(1f32, 0f32, 1f32),
            Point3::new(0f32, 1f32, 1f32),
        ];
        let _sa = solid_angle(&p, &n, &pts);
    }
}
