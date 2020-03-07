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

pub fn solid_angle(p: &Vector3<f64>, n: &Vector3<f64>, pts: &[&Vector3<f64>]) -> f64 {
    // compute projected solid area using Stoke's theorem from Improving Radiosity Solutions
    // through the Use of Analytically Determined Form Factors by Baum, Rushmeier, and Winget
    // (Eq. 9 on pg. 6 (or "330"))

    // integrate over edges
    let mut projarea = 0f64;
    for (i, &v) in pts.iter().enumerate() {
        let j = (i + 1) % pts.len();
        let v0 = pts[i] - p;
        let v1 = pts[j] - p;
        let mut tau = v0.cross(&v1);
        let v0 = v0.normalize();
        let v1 = v1.normalize();
        let dotp = clamp(v0.dot(&v1), -1f64, 1f64);

        let gamma = dotp.acos();
        assert!(gamma.is_finite(), "triangle gamma is infinite");

        tau.normalize();
        tau *= gamma;
        projarea -= n.dot(&tau);
    }

    return projarea / (2f64 * PI);
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
