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
use nalgebra::{convert, Isometry3, Matrix4, Perspective3, Point3, Unit, UnitQuaternion, Vector3};
use std::f64::consts::PI;

pub struct ArcBallCamera {
    target: Point3<f64>,
    distance: f64,
    yaw: f64,
    pitch: f64,
    up: Vector3<f64>,
    rotation: UnitQuaternion<f64>,
    projection: Perspective3<f64>,
    fov_y: f64,
    z_near: f64,
    z_far: f64,
    in_rotate: bool,
    in_move: bool,
}

impl ArcBallCamera {
    pub fn new(aspect_ratio: f64, z_near: f64, z_far: f64) -> Self {
        Self {
            target: Point3::new(0f64, 0f64, 0f64),
            distance: 1f64,
            yaw: PI / 2f64,
            pitch: 3f64 * PI / 4f64,
            up: Vector3::y(),
            rotation: UnitQuaternion::from_axis_angle(&Unit::new_normalize(Vector3::z()), 0.0),
            projection: Perspective3::new(1f64 / aspect_ratio, PI / 2f64, z_near, z_far),
            fov_y: PI / 2f64,
            z_near,
            z_far,
            in_rotate: false,
            in_move: false,
        }
    }

    pub fn get_distance(&self) -> f64 {
        self.distance
    }

    pub fn set_distance(&mut self, distance: f64) {
        self.distance = distance;
    }

    pub fn get_target(&self) -> Point3<f64> {
        self.target
    }

    pub fn set_target(&mut self, x: f64, y: f64, z: f64) {
        self.target = Point3::new(x, y, z);
    }

    pub fn set_target_point(&mut self, p: &Point3<f64>) {
        self.target = *p;
    }

    pub fn set_up(&mut self, up: Vector3<f64>) {
        self.up = up;
    }

    pub fn set_rotation(&mut self, rotation: UnitQuaternion<f64>) {
        self.rotation = rotation;
    }

    pub fn set_angle(&mut self, pitch: f64, yaw: f64) {
        self.pitch = pitch;
        self.yaw = yaw;
    }

    pub fn set_aspect_ratio(&mut self, aspect_ratio: f64) {
        self.projection =
            Perspective3::new(1f64 / aspect_ratio, self.fov_y, self.z_near, self.z_far)
    }

    fn eye(&self) -> Point3<f64> {
        let relative = Vector3::new(
            self.distance * self.yaw.cos() * self.pitch.sin(),
            self.distance * self.pitch.cos(),
            self.distance * self.yaw.sin() * self.pitch.sin(),
        );
        //        let rotation =
        //            UnitQuaternion::from_axis_angle(&Unit::new_normalize(Vector3::z()), PI / 2.0);
        let position = (self.rotation * relative).to_homogeneous() + self.target.to_homogeneous();
        Point3::from_homogeneous(position).unwrap()
    }

    fn view(&self) -> Isometry3<f64> {
        Isometry3::look_at_rh(&self.eye(), &self.target, &self.up)
    }

    pub fn projection_for(&self, model: Isometry3<f64>) -> Matrix4<f64> {
        convert(
            convert::<Matrix4<f32>, Matrix4<f64>>(self.projection_matrix())
                * (model * self.view()).to_homogeneous(),
        )
    }

    pub fn on_mousemove(&mut self, x: f64, y: f64) {
        if self.in_rotate {
            self.yaw += x * 0.5 * (PI / 180f64);

            self.pitch += y * (PI / 180f64);
            self.pitch = self.pitch.min(PI - 0.001f64).max(0.001f64);
        }

        if self.in_move {
            let eye = self.eye();
            let dir = (self.target - eye).normalize();
            let tangent = Vector3::y().cross(&dir).normalize();
            let bitangent = dir.cross(&tangent);
            let mult = (self.distance / 1000.0).max(0.01);
            self.target = self.target + tangent * (x * mult) + bitangent * (-y * mult);
        }
    }

    pub fn on_mousescroll(&mut self, _x: f64, y: f64) {
        // up/down is y
        //   Up is negative
        //   Down is positive
        //   Works in steps of 15 for my mouse.
        self.distance *= if y > 0f64 { 1.1f64 } else { 0.9f64 };
        self.distance = self.distance.max(0.01);
    }

    pub fn on_mousebutton_down(&mut self, id: u32) {
        match id {
            1 => self.in_rotate = true,
            3 => self.in_move = true,
            _ => trace!("button down: {}", id),
        }
    }

    pub fn on_mousebutton_up(&mut self, id: u32) {
        match id {
            1 => self.in_rotate = false,
            3 => self.in_move = false,
            _ => trace!("button up: {}", id),
        }
    }
}

impl CameraAbstract for ArcBallCamera {
    fn view_matrix(&self) -> Matrix4<f32> {
        convert(self.view())
    }

    fn projection_matrix(&self) -> Matrix4<f32> {
        convert(*self.projection.as_matrix())
    }

    fn inverted_projection_matrix(&self) -> Matrix4<f32> {
        convert(self.projection.inverse())
    }

    fn inverted_view_matrix(&self) -> Matrix4<f32> {
        convert(self.view().inverse().to_homogeneous())
    }

    fn position(&self) -> Point3<f32> {
        convert(self.eye())
    }
}
