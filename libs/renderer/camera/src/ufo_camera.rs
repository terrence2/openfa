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
use crate::CameraAbstract;
use log::trace;
use nalgebra::{
    Isometry3, Matrix4, Perspective3, Point3, Translation3, Unit, UnitQuaternion, Vector3,
};
use std::f64::consts::PI;

pub struct UfoCamera {
    position: Translation3<f64>,
    rotation: UnitQuaternion<f64>,
    fovy: f64,
    projection: Perspective3<f64>,
    znear: f64,
    zfar: f64,

    move_vector: Vector3<f64>,
}

impl UfoCamera {
    pub fn new(aspect_ratio: f64, znear: f64, zfar: f64) -> Self {
        Self {
            //            distance: 1f32,
            //            yaw: PI / 2f32,
            //            pitch: 3f32 * PI / 4f32,
            position: Translation3::new(0f64, 0f64, 0f64),
            rotation: UnitQuaternion::from_axis_angle(
                &Unit::new_normalize(Vector3::new(0f64, -1f64, 0f64)),
                0f64,
            ),
            fovy: PI / 2f64,
            projection: Perspective3::new(1f64 / aspect_ratio, PI / 2f64, znear, zfar),
            znear,
            zfar,
            //            in_rotate: false,
            //            in_move: false,
            move_vector: Vector3::new(0f64, 0f64, 0f64),
        }
    }

    pub fn set_aspect_ratio(&mut self, aspect_ratio: f64) {
        self.projection = Perspective3::new(1f64 / aspect_ratio, self.fovy, self.znear, self.zfar)
    }

    pub fn plus_move_up(&mut self) {
        self.move_vector.y = -1f64;
    }

    pub fn minus_move_up(&mut self) {
        self.move_vector.y = 0f64;
    }

    pub fn plus_move_down(&mut self) {
        self.move_vector.y = 1f64;
    }

    pub fn minus_move_down(&mut self) {
        self.move_vector.y = 0f64;
    }

    pub fn plus_move_right(&mut self) {
        self.move_vector.x = 1f64;
    }

    pub fn minus_move_right(&mut self) {
        self.move_vector.x = 0f64;
    }

    pub fn plus_move_left(&mut self) {
        self.move_vector.x = -1f64;
    }

    pub fn minus_move_left(&mut self) {
        self.move_vector.x = 0f64;
    }

    pub fn plus_move_forward(&mut self) {
        self.move_vector.z = -1f64;
    }

    pub fn minus_move_forward(&mut self) {
        self.move_vector.z = 0f64;
    }

    pub fn plus_move_backward(&mut self) {
        self.move_vector.z = 1f64;
    }

    pub fn minus_move_backward(&mut self) {
        self.move_vector.z = 0f64;
    }
}

impl CameraAbstract for UfoCamera {
    fn view_matrix(&self) -> Matrix4<f32> {
        let iso = Isometry3::from_parts(self.position.clone(), self.rotation.clone());
        nalgebra::convert(iso.to_homogeneous())
    }

    fn projection_matrix(&self) -> Matrix4<f32> {
        nalgebra::convert(*self.projection.as_matrix())
    }

    fn inverted_projection_matrix(&self) -> Matrix4<f32> {
        nalgebra::convert(self.projection.inverse())
    }

    fn inverted_view_matrix(&self) -> Matrix4<f32> {
        let iso = Isometry3::from_parts(self.position.clone(), self.rotation.clone());
        nalgebra::convert(iso.inverse().to_homogeneous())
    }
}
