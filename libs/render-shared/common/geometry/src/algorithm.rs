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
use nalgebra::Vector3;
use std::f64::consts::PI;

pub fn clamp(v: f64, low: f64, high: f64) -> f64 {
    v.max(low).min(high)
}

pub fn solid_angle(
    observer_position: &Vector3<f64>,
    observer_direction: &Vector3<f64>,
    vertices: &[Vector3<f64>],
) -> f64 {
    // compute projected solid area using Stoke's theorem from Improving Radiosity Solutions
    // through the Use of Analytically Determined Form Factors by Baum, Rushmeier, and Winget
    // (Eq. 9 on pg. 6 (or "330"))
    assert!(vertices.len() > 2);

    // integrate over edges
    let mut projarea = 0f64;
    for (i, &v) in vertices.iter().enumerate() {
        let j = (i + 1) % vertices.len();
        let v0 = v - observer_position;
        let v1 = vertices[j] - observer_position;
        let mut tau = v0.cross(&v1);
        let v0 = v0.normalize();
        let v1 = v1.normalize();
        let dotp = clamp(v0.dot(&v1), -1f64, 1f64);

        let gamma = dotp.acos();
        assert!(gamma.is_finite(), "triangle gamma is infinite");

        tau.normalize_mut();
        tau *= gamma;
        projarea -= observer_direction.dot(&tau);
    }

    projarea / (2f64 * PI)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let p = Vector3::new(0f64, 0f64, 0f64);
        let n = Vector3::new(0f64, 0f64, 1f64);
        let pts = [
            Vector3::new(0f64, 0f64, 1f64),
            Vector3::new(1f64, 0f64, 1f64),
            Vector3::new(0f64, 1f64, 1f64),
        ];
        let _sa = solid_angle(&p, &n, &pts);
    }
}
