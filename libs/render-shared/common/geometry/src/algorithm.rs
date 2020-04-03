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
use nalgebra::{clamp, convert, Point3, RealField, Vector3};

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works_64() {
        let p = Point3::new(0f64, 0f64, 0f64);
        let n = Vector3::new(0f64, 0f64, 1f64);
        let pts = [
            Point3::new(0f64, 0f64, 1f64),
            Point3::new(1f64, 0f64, 1f64),
            Point3::new(0f64, 1f64, 1f64),
        ];
        let _sa = solid_angle(&p, &n, &pts);
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
