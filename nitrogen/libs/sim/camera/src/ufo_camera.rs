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
use command::{Bindings, Command};
use failure::Fallible;
use nalgebra::{
    Isometry3, Matrix4, Perspective3, Point3, Similarity3, Translation3, Unit, UnitQuaternion,
    Vector3,
};
use std::f64::consts::PI;

pub struct UfoCamera {
    position: Translation3<f64>,
    rotation: UnitQuaternion<f64>,
    fov_y: f64,
    aspect_ratio: f64,
    projection: Perspective3<f64>,
    z_near: f64,
    z_far: f64,

    pub speed: f64,
    pub sensitivity: f64,
    move_vector: Vector3<f64>,
    rot_vector: Vector3<f64>,
}

impl UfoCamera {
    pub fn new(aspect_ratio: f64, z_near: f64, z_far: f64) -> Self {
        Self {
            position: Translation3::new(0f64, 0f64, 0f64),
            rotation: UnitQuaternion::from_axis_angle(
                &Unit::new_normalize(Vector3::new(0f64, -1f64, 0f64)),
                0f64,
            ),
            fov_y: PI / 2f64,
            aspect_ratio,
            projection: Perspective3::new(1f64 / aspect_ratio, PI / 2f64, z_near, z_far),
            z_near,
            z_far,
            speed: 1.0,
            sensitivity: 0.2,
            move_vector: nalgebra::zero(),
            rot_vector: nalgebra::zero(),
        }
    }

    pub fn set_position(&mut self, x: f64, y: f64, z: f64) {
        self.position = Translation3::new(x, y, z);
    }

    pub fn eye(&self) -> Point3<f64> {
        self.position.transform_point(&Point3::new(0.0, 0.0, 0.0))
    }

    pub fn up(&self) -> Vector3<f64> {
        self.rotation * Vector3::new(0.0, -1.0, 0.0)
    }

    pub fn set_rotation(&mut self, v: &Vector3<f64>, ang: f64) {
        self.rotation = UnitQuaternion::from_axis_angle(&Unit::new_normalize(*v), ang);
    }

    pub fn apply_rotation(&mut self, v: &Vector3<f64>, ang: f64) {
        let quat = UnitQuaternion::from_axis_angle(&Unit::new_normalize(*v), ang);
        self.rotation *= quat;
    }

    pub fn set_aspect_ratio(&mut self, aspect_ratio: f64) {
        self.aspect_ratio = aspect_ratio;
        self.projection =
            Perspective3::new(1f64 / aspect_ratio, self.fov_y, self.z_near, self.z_far)
    }

    pub fn target(&self) -> Point3<f64> {
        let forward = self.rotation * Vector3::new(0.0, 0.0, 1.0);
        self.position.transform_point(&Point3::from(forward))
    }

    pub fn zoom_in(&mut self) {
        self.fov_y -= 5.0 * PI / 180.0;
        self.fov_y = self.fov_y.min(10.0 * PI / 180.0);
        self.projection = Perspective3::new(
            1f64 / self.aspect_ratio,
            self.fov_y,
            self.z_near,
            self.z_far,
        )
    }

    pub fn zoom_out(&mut self) {
        self.fov_y += 5.0 * PI / 180.0;
        self.fov_y = self.fov_y.max(90.0 * PI / 180.0);
        self.projection = Perspective3::new(
            1f64 / self.aspect_ratio,
            self.fov_y,
            self.z_near,
            self.z_far,
        )
    }

    pub fn think(&mut self) {
        let forward = self.rotation * Vector3::new(0.0, 0.0, 1.0);
        let right = self.rotation * Vector3::new(1.0, 0.0, 0.0);
        let up = self.rotation * Vector3::new(0.0, -1.0, 0.0);

        let pitch_rot = UnitQuaternion::from_axis_angle(
            &Unit::new_unchecked(right),
            self.rot_vector.y * self.sensitivity * PI / 180.0,
        );
        let yaw_rot = UnitQuaternion::from_axis_angle(
            &Unit::new_unchecked(up),
            self.rot_vector.x * self.sensitivity * PI / 180.0,
        );
        let roll_rot = UnitQuaternion::from_axis_angle(
            &Unit::new_unchecked(forward),
            self.rot_vector.z / 50.0,
        );
        self.rot_vector.x = 0.0;
        self.rot_vector.y = 0.0;

        self.rotation = yaw_rot * self.rotation;
        self.rotation = pitch_rot * self.rotation;
        self.rotation = roll_rot * self.rotation;

        if self.move_vector.norm_squared() > 0.0 {
            let mv = (self.rotation * self.move_vector.normalize()) * self.speed;
            self.position.x += mv.x;
            self.position.y += mv.y;
            self.position.z += mv.z;
        }
    }

    pub fn on_mousemove(&mut self, x: f64, y: f64) {
        self.rot_vector.x = x;
        self.rot_vector.y = y;
    }

    pub fn plus_rotate_right(&mut self) {
        self.rot_vector.z = 1.0;
    }

    pub fn minus_rotate_right(&mut self) {
        self.rot_vector.z = 0.0;
    }

    pub fn plus_rotate_left(&mut self) {
        self.rot_vector.z = -1.0;
    }

    pub fn minus_rotate_left(&mut self) {
        self.rot_vector.z = 0.0;
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
        // n.b. flipped depth
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

    pub fn view(&self) -> Isometry3<f32> {
        // FIXME:
        Isometry3::identity()
    }

    pub fn projection(&self) -> Perspective3<f64> {
        self.projection
    }

    pub fn view_matrix(&self) -> Matrix4<f32> {
        let simi = Similarity3::from_parts(self.position, self.rotation, 1.0);
        nalgebra::convert(simi.inverse().to_homogeneous())
    }

    pub fn projection_matrix(&self) -> Matrix4<f32> {
        nalgebra::convert(*self.projection.as_matrix())
    }

    pub fn inverted_projection_matrix(&self) -> Matrix4<f32> {
        nalgebra::convert(self.projection.inverse())
    }

    pub fn inverted_view_matrix(&self) -> Matrix4<f32> {
        let simi = Similarity3::from_parts(self.position, self.rotation, 1.0);
        nalgebra::convert(simi.to_homogeneous())
    }

    pub fn position(&self) -> Point3<f32> {
        let down: Translation3<f32> = nalgebra::convert(self.position);
        Point3::new(down.vector[0], down.vector[1], down.vector[2])
    }

    pub fn default_bindings() -> Fallible<Bindings> {
        Ok(Bindings::new("shape")
            .bind("zoom-in", "Equals")?
            .bind("zoom-out", "Subtract")?
            .bind("+rotate-right", "c")?
            .bind("+rotate-left", "z")?
            .bind("+move-left", "a")?
            .bind("+move-right", "d")?
            .bind("+move-forward", "w")?
            .bind("+move-backward", "s")?
            .bind("+move-up", "space")?
            .bind("+move-down", "Control")?)
    }

    pub fn handle_command(&mut self, command: &Command) -> Fallible<()> {
        match command.name.as_str() {
            "mouse-move" => self.on_mousemove(command.displacement()?.0, command.displacement()?.1),
            "mouse-wheel" => {
                if command.displacement()?.1 > 0.0 {
                    self.speed *= 0.8;
                } else {
                    self.speed *= 1.2;
                }
            }
            "zoom-in" => self.zoom_in(),
            "zoom-out" => self.zoom_out(),
            "+rotate-right" => self.plus_rotate_right(),
            "-rotate-right" => self.minus_rotate_right(),
            "+rotate-left" => self.plus_rotate_left(),
            "-rotate-left" => self.minus_rotate_left(),
            "+move-up" => self.plus_move_up(),
            "-move-up" => self.minus_move_up(),
            "+move-down" => self.plus_move_down(),
            "-move-down" => self.minus_move_down(),
            "+move-left" => self.plus_move_left(),
            "-move-left" => self.minus_move_left(),
            "+move-right" => self.plus_move_right(),
            "-move-right" => self.minus_move_right(),
            "+move-backward" => self.plus_move_backward(),
            "-move-backward" => self.minus_move_backward(),
            "+move-forward" => self.plus_move_forward(),
            "-move-forward" => self.minus_move_forward(),
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn test_move() {
        let mut camera = UfoCamera::new(1.0, 1.0, 10.0);
        camera.plus_move_right();
        camera.think();
        assert_relative_eq!(camera.position.x, camera.speed);
        camera.minus_move_right();
        camera.plus_move_left();
        camera.think();
        camera.think();
        assert_relative_eq!(camera.position.x, -camera.speed);
        assert_relative_eq!(camera.position.y, 0.0);
        assert_relative_eq!(camera.position.z, 0.0);
    }

    /*
    #[test]
    fn test_rotate() {
        let mut camera = UfoCamera::new(1.0, 1.0, 10.0);
        camera.sensitivity = 1.0;
        camera.rot_vector.x = 90.0;
        camera.think();
        camera.plus_move_right();
        camera.think();
        assert_relative_eq!(camera.position.z, -1.0);

        let mut camera = UfoCamera::new(1.0, 1.0, 10.0);
        camera.sensitivity = 1.0;
        camera.rot_vector.x = -45.0;
        camera.think();
        camera.plus_move_right();
        camera.think();
        assert_relative_eq!(camera.position.x, 2f64.sqrt() / 2.0);
        assert_relative_eq!(camera.position.z, 2f64.sqrt() / 2.0);
        camera.minus_move_right();
        camera.plus_move_up();
        camera.think();
        assert_relative_eq!(camera.position.x, 2f64.sqrt() / 2.0);
        assert_relative_eq!(camera.position.y, -1.0);
        assert_relative_eq!(camera.position.z, 2f64.sqrt() / 2.0);

        let mut camera = UfoCamera::new(1.0, 1.0, 10.0);
        camera.sensitivity = 1.0;
        camera.rot_vector.y = 45.0;
        camera.think();
        camera.plus_move_up();
        camera.think();
        assert_relative_eq!(camera.position.y, -(2f64.sqrt()) / 2.0);
        assert_relative_eq!(camera.position.z, 2f64.sqrt() / 2.0);

        let mut camera = UfoCamera::new(1.0, 1.0, 10.0);
        camera.sensitivity = 1.0;
        camera.rot_vector.x = 45.0;
        camera.think();
        camera.plus_move_right();
        camera.think();
        assert_relative_eq!(camera.position.x, 2f64.sqrt() / 2.0);
        assert_relative_eq!(camera.position.z, -(2f64.sqrt()) / 2.0);
    }
    */
}
