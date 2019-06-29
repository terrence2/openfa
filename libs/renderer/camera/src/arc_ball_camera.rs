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
use nalgebra::{Isometry3, Matrix4, Perspective3, Point3, Vector3};
use std::f32::consts::PI;

pub struct ArcBallCamera {
    target: Point3<f32>,
    distance: f32,
    yaw: f32,
    pitch: f32,
    projection: Perspective3<f32>,
    fovy: f32,
    znear: f32,
    zfar: f32,
    in_rotate: bool,
    in_move: bool,
}

impl ArcBallCamera {
    pub fn new(aspect_ratio: f32, znear: f32, zfar: f32) -> Self {
        Self {
            target: Point3::new(0f32, 0f32, 0f32),
            distance: 1f32,
            yaw: PI / 2f32,
            pitch: 3f32 * PI / 4f32,
            projection: Perspective3::new(1f32 / aspect_ratio, PI / 2f32, znear, zfar),
            fovy: PI / 2f32,
            znear,
            zfar,
            in_rotate: false,
            in_move: false,
        }
    }

    pub fn get_distance(&mut self) -> f32 {
        self.distance
    }

    pub fn set_distance(&mut self, distance: f32) {
        self.distance = distance;
    }

    pub fn set_target(&mut self, x: f32, y: f32, z: f32) {
        self.target = Point3::new(x, y, z);
    }

    pub fn set_angle(&mut self, pitch: f32, yaw: f32) {
        self.pitch = pitch;
        self.yaw = yaw;
    }

    pub fn set_aspect_ratio(&mut self, aspect_ratio: f32) {
        self.projection = Perspective3::new(1f32 / aspect_ratio, self.fovy, self.znear, self.zfar)
    }

    fn eye(&self) -> Point3<f32> {
        let px = self.target.x + self.distance * self.yaw.cos() * self.pitch.sin();
        let py = self.target.y + self.distance * self.pitch.cos();
        let pz = self.target.z + self.distance * self.yaw.sin() * self.pitch.sin();
        Point3::new(px, py, pz)
    }

    fn view(&self) -> Isometry3<f32> {
        Isometry3::look_at_rh(&self.eye(), &self.target, &Vector3::y())
    }

    pub fn projection_for(&self, model: Isometry3<f32>) -> Matrix4<f32> {
        self.projection_matrix() * (model * self.view()).to_homogeneous()
    }

    pub fn on_mousemove(&mut self, x: f32, y: f32) {
        if self.in_rotate {
            self.yaw += x * 0.5 * (PI / 180f32);

            self.pitch += y * (PI / 180f32);
            self.pitch = self.pitch.min(PI - 0.001f32).max(0.001f32);
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

    pub fn on_mousescroll(&mut self, _x: f32, y: f32) {
        // up/down is y
        //   Up is negative
        //   Down is positive
        //   Works in steps of 15 for my mouse.
        self.distance *= if y > 0f32 { 1.1f32 } else { 0.9f32 };
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
        Matrix4::look_at_rh(&self.eye(), &self.target, &Vector3::y())
    }

    fn projection_matrix(&self) -> Matrix4<f32> {
        *self.projection.as_matrix()
    }

    fn inverted_projection_matrix(&self) -> Matrix4<f32> {
        self.projection.inverse()
    }

    fn inverted_view_matrix(&self) -> Matrix4<f32> {
        Isometry3::look_at_rh(&self.eye(), &self.target, &Vector3::y())
            .inverse()
            .to_homogeneous()
    }
}
