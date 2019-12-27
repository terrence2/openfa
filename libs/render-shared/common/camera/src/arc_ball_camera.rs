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
use absolute_unit::{degrees, meters, radians, Angle, Length, LengthUnit, Meters, Radians};
use command::{Bindings, Command};
use failure::Fallible;
use nalgebra::{convert, Isometry3, Matrix4, Perspective3, Point3, Vector3};
use std::f64::consts::PI;

pub struct ArcBallCamera {
    fov_y: Angle<Radians>,
    z_near: Length<Meters>,
    z_far: Length<Meters>,
    in_rotate: bool,
    in_move: bool,

    target: Point3<f64>,
    //target: Position<GeoSurface>,
    distance: Length<Meters>,
    yaw: Angle<Radians>,
    pitch: Angle<Radians>,

    pub up: Vector3<f64>,
    projection: Perspective3<f64>,
}

impl ArcBallCamera {
    pub fn new(aspect_ratio: f64, z_near: Length<Meters>, z_far: Length<Meters>) -> Self {
        let fov_y = radians!(PI / 2f64);
        Self {
            //target: Position::<GeoSurface>::new(radians!(0), radians!(0), meters!(0)),
            target: Point3::new(0.0, 0.0, 0.0),
            distance: meters!(1),
            yaw: radians!(PI / 2f64),
            pitch: radians!(3f64 * PI / 4f64),
            up: Vector3::y(),
            projection: Perspective3::new(
                1f64 / aspect_ratio,
                fov_y.into(),
                z_near.into(),
                z_far.into(),
            ),
            fov_y,
            z_near,
            z_far,
            in_rotate: false,
            in_move: false,
        }
    }

    pub fn get_distance(&self) -> Length<Meters> {
        self.distance
    }

    pub fn set_distance<Unit: LengthUnit>(&mut self, distance: Length<Unit>) {
        self.distance = meters!(distance);
    }

    pub fn set_target(&mut self, x: f64, y: f64, z: f64) {
        self.target = Point3::new(x, y, z);
    }

    pub fn set_target_point(&mut self, p: &Point3<f64>) {
        self.target = *p;
    }

    pub fn get_target(&self) -> Point3<f64> {
        self.target
    }

    //    pub fn get_target(&self) -> Position<GeoSurface> {
    //        self.target
    //    }

    /*
    pub fn set_up(&mut self, up: Vector3<f64>) {
        self.up = up;
    }

    pub fn set_angle(&mut self, pitch: Angle<Radians>, yaw: Angle<Radians>) {
        self.pitch = pitch;
        self.yaw = yaw;
    }
    */

    pub fn set_aspect_ratio(&mut self, aspect_ratio: f64) {
        self.projection = Perspective3::new(
            1f64 / aspect_ratio,
            self.fov_y.into(),
            self.z_near.into(),
            self.z_far.into(),
        )
    }

    //pub fn eye_position_relative_to_tile(&self, origin: Position<GeoSurface>) -> Point3<f64> {
    pub fn eye(&self) -> Point3<f64> {
        let relative = Vector3::new(
            f64::from(self.distance * self.yaw.cos() * self.pitch.sin()),
            f64::from(self.distance * self.pitch.cos()),
            f64::from(self.distance * self.yaw.sin() * self.pitch.sin()),
        );
        let position = relative.to_homogeneous() + self.target.to_homogeneous();
        Point3::from_homogeneous(position).unwrap()
    }

    pub fn projection_for(&self, model: Isometry3<f32>) -> Matrix4<f64> {
        convert(self.projection_matrix() * (model * self.view()).to_homogeneous())
    }

    pub fn on_mousemove(&mut self, command: &Command) -> Fallible<()> {
        let (x, y) = command.displacement()?;

        if self.in_rotate {
            self.yaw += degrees!(x * 0.5);

            self.pitch += degrees!(y);
            self.pitch = self.pitch.min(radians!(PI - 0.001)).max(radians!(0.001));
        }

        if self.in_move {
            let eye = self.eye();
            let dir = (self.target - eye).normalize();
            let tangent = Vector3::y().cross(&dir).normalize();
            let bitangent = dir.cross(&tangent);
            let mult = f64::from(self.distance / 1000f64).max(0.01);
            self.target = self.target + tangent * (x * mult) + bitangent * (-y * mult);
        }

        Ok(())
    }

    pub fn on_mousescroll(&mut self, command: &Command) -> Fallible<()> {
        let y = command.displacement()?.1;

        // up/down is y
        //   Up is negative
        //   Down is positive
        //   Works in steps of 15 for my mouse.
        self.distance *= if y > 0f64 { 1.1f64 } else { 0.9f64 };
        self.distance = self.distance.max(meters!(0.01));

        Ok(())
    }

    pub fn view(&self) -> Isometry3<f32> {
        convert(Isometry3::look_at_rh(&self.eye(), &self.target, &self.up))
    }

    pub fn projection(&self) -> Perspective3<f64> {
        self.projection
    }

    pub fn view_matrix(&self) -> Matrix4<f32> {
        convert(self.view())
    }

    pub fn projection_matrix(&self) -> Matrix4<f32> {
        convert(*self.projection.as_matrix())
    }

    pub fn inverted_projection_matrix(&self) -> Matrix4<f32> {
        convert(self.projection.inverse())
    }

    pub fn inverted_view_matrix(&self) -> Matrix4<f32> {
        convert(self.view().inverse().to_homogeneous())
    }

    pub fn position(&self) -> Point3<f32> {
        convert(self.eye())
    }

    pub fn think(&mut self) {}

    pub fn default_bindings() -> Fallible<Bindings> {
        Ok(Bindings::new("arc_ball_camera")
            .bind("+pan-view", "mouse1")?
            .bind("+move-view", "mouse3")?)
    }

    pub fn handle_command(&mut self, command: &Command) -> Fallible<()> {
        match command.name.as_str() {
            "+pan-view" => self.in_rotate = true,
            "-pan-view" => self.in_rotate = false,
            "+move-view" => self.in_move = true,
            "-move-view" => self.in_move = false,
            "mouse-move" => self.on_mousemove(command)?,
            "mouse-wheel" => self.on_mousescroll(command)?,
            _ => {}
        }
        Ok(())
    }
}
