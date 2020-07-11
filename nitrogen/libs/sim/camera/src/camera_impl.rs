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
use absolute_unit::{radians, Angle, AngleUnit, Kilometers, Length, LengthUnit, Meters, Radians};
use geodesy::{Cartesian, GeoCenter};
use geometry::Plane;
use nalgebra::{Isometry3, Perspective3, Point3, Vector3};

pub struct Camera {
    // Camera parameters
    fov_y: Angle<Radians>,
    aspect_ratio: f64,
    z_near: Length<Meters>,
    z_far: Length<Meters>,

    // Camera view state.
    position: Cartesian<GeoCenter, Meters>,
    forward: Vector3<f64>,
    up: Vector3<f64>,
    right: Vector3<f64>,
}

impl Camera {
    // FIXME: aspect ratio is wrong. Should be 16:9 and not 9:16.
    // aspect ratio is rise over run: h / w
    pub fn from_parameters(
        fov_y: Angle<Radians>,
        aspect_ratio: f64,
        z_near: Length<Meters>,
        z_far: Length<Meters>,
    ) -> Self {
        Self {
            fov_y,
            aspect_ratio,
            z_near,
            z_far,

            position: Vector3::new(0f64, 0f64, 0f64).into(),
            forward: Vector3::new(0f64, 0f64, -1f64),
            up: Vector3::new(0f64, 1f64, 0f64),
            right: Vector3::new(1f64, 0f64, 0f64),
        }
    }

    pub(crate) fn push_frame_parameters(
        &mut self,
        position: Cartesian<GeoCenter, Meters>,
        forward: Vector3<f64>,
        up: Vector3<f64>,
        right: Vector3<f64>,
    ) {
        self.position = position;
        self.forward = forward;
        self.up = up;
        self.right = right;
    }

    pub fn fov_y(&self) -> Angle<Radians> {
        self.fov_y
    }

    pub fn set_fov_y<T: AngleUnit>(&mut self, fov: Angle<T>) {
        self.fov_y = radians!(fov);
    }

    pub fn aspect_ratio(&self) -> f64 {
        self.aspect_ratio
    }

    pub fn set_aspect_ratio(&mut self, aspect_ratio: f64) {
        self.aspect_ratio = aspect_ratio;
    }

    pub fn position<T: LengthUnit>(&self) -> Cartesian<GeoCenter, T> {
        Cartesian::<GeoCenter, T>::new(
            self.position.coords[0],
            self.position.coords[1],
            self.position.coords[2],
        )
    }

    pub fn forward(&self) -> &Vector3<f64> {
        &self.forward
    }

    pub fn up(&self) -> &Vector3<f64> {
        &self.up
    }

    pub fn right(&self) -> &Vector3<f64> {
        &self.right
    }

    pub fn projection<T: LengthUnit>(&self) -> Perspective3<f64> {
        Perspective3::new(
            1f64 / self.aspect_ratio,
            self.fov_y.into(),
            Length::<T>::from(&self.z_near).into(),
            Length::<T>::from(&self.z_far).into(),
        )
    }

    pub fn view<T: LengthUnit>(&self) -> Isometry3<f64> {
        let eye = self.position::<T>().vec64();
        Isometry3::look_at_rh(
            &Point3::from(eye),
            &Point3::from(eye + self.forward()),
            &-self.up(),
        )
    }

    pub fn world_space_frustum<T: LengthUnit>(&self) -> [Plane<f64>; 5] {
        // Taken from this paper:
        //   https://www.gamedevs.org/uploads/fast-extraction-viewing-frustum-planes-from-world-view-projection-matrix.pdf

        // FIXME: must be kilometers?
        let eye = Cartesian::<GeoCenter, Kilometers>::new(
            self.position.coords[0],
            self.position.coords[1],
            self.position.coords[2],
        )
        .vec64();
        let view = Isometry3::look_at_rh(
            &Point3::from(eye),
            &Point3::from(eye + self.forward),
            &self.up,
        );

        let m = self.projection::<T>().as_matrix() * view.to_homogeneous();

        let lp = (m.row(3) + m.row(0)).transpose();
        let lm = lp.xyz().magnitude();
        let left = Plane::from_normal_and_distance(lp.xyz() / lm, -lp[3] / lm);

        let rp = (m.row(3) - m.row(0)).transpose();
        let rm = rp.xyz().magnitude();
        let right = Plane::from_normal_and_distance(rp.xyz() / rm, -rp[3] / rm);

        let bp = (m.row(3) + m.row(1)).transpose();
        let bm = bp.xyz().magnitude();
        let bottom = Plane::from_normal_and_distance(bp.xyz() / bm, -bp[3] / bm);

        let tp = (m.row(3) - m.row(1)).transpose();
        let tm = tp.xyz().magnitude();
        let top = Plane::from_normal_and_distance(tp.xyz() / tm, -tp[3] / tm);

        let np = (m.row(3) + m.row(2)).transpose();
        let nm = np.xyz().magnitude();
        let near = Plane::from_normal_and_distance(np.xyz() / nm, -np[3] / nm);

        [left, right, bottom, top, near]
    }
}
