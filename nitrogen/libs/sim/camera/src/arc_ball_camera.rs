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
use crate::Camera;
use absolute_unit::{
    degrees, meters, radians, Angle, Degrees, Kilometers, Length, LengthUnit, Meters,
};
use command::{Bindings, Command};
use failure::{ensure, Fallible};
use geodesy::{Cartesian, GeoCenter, GeoSurface, Graticule, Target};
use nalgebra::{Unit as NUnit, UnitQuaternion, Vector3};
use std::f64::consts::PI;

pub struct ArcBallCamera {
    camera: Camera,

    in_rotate: bool,
    in_move: bool,
    fov_delta: Angle<Degrees>,
    target: Graticule<GeoSurface>,
    eye: Graticule<Target>,
}

impl ArcBallCamera {
    // FIXME: push camera in from outside
    pub fn new(aspect_ratio: f64, z_near: Length<Meters>, z_far: Length<Meters>) -> Self {
        let fov_y = radians!(PI / 2f64);
        Self {
            camera: Camera::from_parameters(fov_y, aspect_ratio, z_near, z_far),
            target: Graticule::<GeoSurface>::new(radians!(0), radians!(0), meters!(0)),
            eye: Graticule::<Target>::new(
                radians!(PI / 2.0),
                radians!(3f64 * PI / 4.0),
                meters!(1),
            ),
            fov_delta: degrees!(0),
            in_rotate: false,
            in_move: false,
        }
    }

    pub fn camera(&self) -> &Camera {
        &self.camera
    }

    pub fn camera_mut(&mut self) -> &mut Camera {
        &mut self.camera
    }

    pub fn get_target(&self) -> Graticule<GeoSurface> {
        self.target
    }

    pub fn set_target(&mut self, target: Graticule<GeoSurface>) {
        self.target = target;
    }

    pub fn get_eye_relative(&self) -> Graticule<Target> {
        self.eye
    }

    pub fn set_eye_relative(&mut self, eye: Graticule<Target>) -> Fallible<()> {
        ensure!(
            eye.latitude < radians!(degrees!(90)),
            "eye coordinate past limits"
        );
        self.eye = eye;
        Ok(())
    }

    pub fn get_distance(&self) -> Length<Meters> {
        self.eye.distance
    }

    pub fn set_distance<Unit: LengthUnit>(&mut self, distance: Length<Unit>) {
        self.eye.distance = meters!(distance);
    }

    //pub fn eye_position_relative_to_tile(&self, origin: Position<GeoSurface>) -> Point3<f64> {
    /*
    pub fn eye(&self) -> Point3<f64> {
        let relative = Vector3::new(
            f64::from(self.distance * self.yaw.cos() * self.pitch.sin()),
            f64::from(self.distance * self.pitch.cos()),
            f64::from(self.distance * self.yaw.sin() * self.pitch.sin()),
        );
        let position = relative.to_homogeneous() + self.target.to_homogeneous();
        Point3::from_homogeneous(position).unwrap()
    }
    */

    pub fn on_mousemove(&mut self, command: &Command) -> Fallible<()> {
        let (x, y) = command.displacement()?;

        if self.in_rotate {
            self.eye.longitude -= degrees!(x * 0.5);

            self.eye.latitude += degrees!(y * 0.5f64);
            self.eye.latitude = self
                .eye
                .latitude
                .min(radians!(PI / 2.0 - 0.001))
                .max(radians!(-PI / 2.0 + 0.001));
        }

        if self.in_move {
            let sensitivity: f64 = f64::from(self.get_distance()) / 60_000_000.0;

            let dir = self.eye.longitude;
            let lat = f64::from(degrees!(self.target.latitude)) + dir.cos() * y * sensitivity;
            let lon = f64::from(degrees!(self.target.longitude)) + -dir.sin() * y * sensitivity;
            self.target.latitude = radians!(degrees!(lat));
            self.target.longitude = radians!(degrees!(lon));

            let dir = self.eye.longitude + degrees!(PI / 2.0);
            let lat = f64::from(degrees!(self.target.latitude)) + -dir.sin() * x * sensitivity;
            let lon = f64::from(degrees!(self.target.longitude)) + -dir.cos() * x * sensitivity;
            self.target.latitude = radians!(degrees!(lat));
            self.target.longitude = radians!(degrees!(lon));
        }

        Ok(())
    }

    pub fn on_mousescroll(&mut self, command: &Command) -> Fallible<()> {
        let y = command.displacement()?.1;

        // up/down is y
        //   Up is negative
        //   Down is positive
        //   Works in steps of 15 for my mouse.
        self.eye.distance *= if y > 0f64 { 1.1f64 } else { 0.9f64 };
        self.eye.distance = self.eye.distance.max(meters!(0.01));

        Ok(())
    }

    /*
    pub fn projection_matrix(&self) -> Matrix4<f32> {
        convert(*self.projection.as_matrix())
    }
    */

    /*
    pub fn inverted_projection_matrix(&self) -> Matrix4<f32> {
        convert(self.projection.inverse())
    }

    pub fn inverted_view_matrix(&self) -> Matrix4<f32> {
        convert(self.view().inverse().to_homogeneous())
    }

    pub fn position(&self) -> Point3<f32> {
        convert(self.eye())
    }
    */

    pub fn think(&mut self) {
        let mut fov = degrees!(self.camera.fov_y());
        fov += self.fov_delta;
        fov = fov.min(degrees!(90)).max(degrees!(1));
        self.camera.set_fov_y(fov);

        let target = self.cartesian_target_position::<Kilometers>();
        let eye = self.cartesian_eye_position::<Kilometers>();
        let forward = (target - eye).vec64();
        let right = eye.vec64().cross(&forward);
        let up = right.cross(&forward);
        self.camera.push_frame_parameters(
            Cartesian::new(
                meters!(eye.coords[0]),
                meters!(eye.coords[1]),
                meters!(eye.coords[2]),
            ),
            forward.normalize(),
            up.normalize(),
            right.normalize(),
        );
    }

    pub fn default_bindings() -> Fallible<Bindings> {
        Ok(Bindings::new("arc_ball_camera")
            .bind("+pan-view", "mouse1")?
            .bind("+move-view", "mouse3")?
            .bind("+fov_up", "PageUp")?
            .bind("+fov_down", "PageDown")?)
    }

    pub fn handle_command(&mut self, command: &Command) -> Fallible<()> {
        match command.name.as_str() {
            "+pan-view" => self.in_rotate = true,
            "-pan-view" => self.in_rotate = false,
            "+move-view" => self.in_move = true,
            "-move-view" => self.in_move = false,
            "+fov_up" => self.fov_delta = degrees!(1),
            "-fov_up" => self.fov_delta = degrees!(0),
            "+fov_down" => self.fov_delta = degrees!(-1),
            "-fov_down" => self.fov_delta = degrees!(0),
            "mouse-move" => self.on_mousemove(command)?,
            "mouse-wheel" => self.on_mousescroll(command)?,
            _ => {}
        }
        Ok(())
    }

    fn cartesian_target_position<Unit: LengthUnit>(&self) -> Cartesian<GeoCenter, Unit> {
        Cartesian::<GeoCenter, Unit>::from(Graticule::<GeoCenter>::from(self.target))
    }

    fn cartesian_eye_position<Unit: LengthUnit>(&self) -> Cartesian<GeoCenter, Unit> {
        let r_lon = UnitQuaternion::from_axis_angle(
            &NUnit::new_unchecked(Vector3::new(0f64, 1f64, 0f64)),
            -f64::from(self.target.longitude),
        );
        let r_lat = UnitQuaternion::from_axis_angle(
            &NUnit::new_normalize(r_lon * Vector3::new(1f64, 0f64, 0f64)),
            PI / 2.0 - f64::from(self.target.latitude),
        );
        let cart_target = self.cartesian_target_position::<Unit>();
        let cart_eye_rel_target_flat = Cartesian::<Target, Unit>::from(self.eye);
        let cart_eye_rel_target_framed =
            Cartesian::<Target, Unit>::from(r_lat * r_lon * cart_eye_rel_target_flat.vec64());
        cart_target + cart_eye_rel_target_framed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use absolute_unit::{kilometers, Kilometers};
    use approx::assert_abs_diff_eq;
    use physical_constants::EARTH_RADIUS_KM;

    #[test]
    fn it_can_compute_eye_positions_at_origin() -> Fallible<()> {
        let mut c = ArcBallCamera::new(1f64, meters!(0.1f64), meters!(1000f64));

        // Verify base target position.
        let t = c.cartesian_target_position::<Kilometers>();
        assert_abs_diff_eq!(t.coords[0], kilometers!(0));
        assert_abs_diff_eq!(t.coords[1], kilometers!(0));
        assert_abs_diff_eq!(t.coords[2], kilometers!(EARTH_RADIUS_KM));

        // Target: 0/0; at latitude of 0:
        {
            // Longitude 0 maps to south, latitude 90 to up,
            // when rotated into the surface frame.
            c.set_eye_relative(Graticule::<Target>::new(
                degrees!(0),
                degrees!(0),
                meters!(1),
            ))?;
            let e = c.cartesian_eye_position::<Kilometers>();
            assert_abs_diff_eq!(e.coords[0], kilometers!(0));
            assert_abs_diff_eq!(e.coords[1], kilometers!(-0.001));
            assert_abs_diff_eq!(e.coords[2], kilometers!(EARTH_RADIUS_KM));

            c.set_eye_relative(Graticule::<Target>::new(
                degrees!(0),
                degrees!(90),
                meters!(1),
            ))?;
            let e = c.cartesian_eye_position::<Kilometers>();
            assert_abs_diff_eq!(e.coords[0], kilometers!(-0.001));
            assert_abs_diff_eq!(e.coords[1], kilometers!(0));
            assert_abs_diff_eq!(e.coords[2], kilometers!(EARTH_RADIUS_KM));

            c.set_eye_relative(Graticule::<Target>::new(
                degrees!(0),
                degrees!(-90),
                meters!(1),
            ))?;
            let e = c.cartesian_eye_position::<Kilometers>();
            assert_abs_diff_eq!(e.coords[0], kilometers!(0.001));
            assert_abs_diff_eq!(e.coords[1], kilometers!(0));
            assert_abs_diff_eq!(e.coords[2], kilometers!(EARTH_RADIUS_KM));

            c.set_eye_relative(Graticule::<Target>::new(
                degrees!(0),
                degrees!(-180),
                meters!(1),
            ))?;
            let e = c.cartesian_eye_position::<Kilometers>();
            assert_abs_diff_eq!(e.coords[0], kilometers!(0));
            assert_abs_diff_eq!(e.coords[1], kilometers!(0.001));
            assert_abs_diff_eq!(e.coords[2], kilometers!(EARTH_RADIUS_KM));
        }

        Ok(())
    }

    #[test]
    fn it_can_compute_eye_positions_with_offset_latitude() -> Fallible<()> {
        let mut c = ArcBallCamera::new(1f64, meters!(0.1f64), meters!(1000f64));

        // Verify base target position.
        let t = c.cartesian_target_position::<Kilometers>();
        assert_abs_diff_eq!(t.coords[0], kilometers!(0));
        assert_abs_diff_eq!(t.coords[1], kilometers!(0));
        assert_abs_diff_eq!(t.coords[2], kilometers!(EARTH_RADIUS_KM));

        // Target: 0/0; at latitude of 45
        {
            c.set_eye_relative(Graticule::<Target>::new(
                degrees!(45),
                degrees!(0),
                meters!(1),
            ))?;
            let e = c.cartesian_eye_position::<Kilometers>();
            assert_abs_diff_eq!(e.coords[0], kilometers!(0));
            assert_abs_diff_eq!(e.coords[1], kilometers!(-0.000_707_106_781));
            assert_abs_diff_eq!(
                e.coords[2],
                kilometers!(EARTH_RADIUS_KM + 0.000_707_106_781)
            );

            c.set_eye_relative(Graticule::<Target>::new(
                degrees!(45),
                degrees!(90),
                meters!(1),
            ))?;
            let e = c.cartesian_eye_position::<Kilometers>();
            assert_abs_diff_eq!(e.coords[0], kilometers!(-0.000_707_106_781));
            assert_abs_diff_eq!(e.coords[1], kilometers!(0));
            assert_abs_diff_eq!(
                e.coords[2],
                kilometers!(EARTH_RADIUS_KM + 0.000_707_106_781)
            );
        }

        Ok(())
    }

    #[test]
    fn it_can_compute_eye_positions_with_offset_longitude() -> Fallible<()> {
        let mut c = ArcBallCamera::new(1f64, meters!(0.1f64), meters!(1000f64));

        // Verify base target position.
        let t = c.cartesian_target_position::<Kilometers>();
        assert_abs_diff_eq!(t.coords[0], kilometers!(0));
        assert_abs_diff_eq!(t.coords[1], kilometers!(0));
        assert_abs_diff_eq!(t.coords[2], kilometers!(EARTH_RADIUS_KM));
        // Target: 0/90; at eye latitude of 0
        {
            c.set_target(Graticule::<GeoSurface>::new(
                degrees!(0),
                degrees!(90),
                meters!(0),
            ));

            c.set_eye_relative(Graticule::<Target>::new(
                degrees!(0),
                degrees!(0),
                kilometers!(1),
            ))?;
            let e = c.cartesian_eye_position::<Kilometers>();
            assert_abs_diff_eq!(e.coords[0], kilometers!(-EARTH_RADIUS_KM));
            assert_abs_diff_eq!(e.coords[1], kilometers!(-1));
            assert_abs_diff_eq!(e.coords[2], kilometers!(0));

            c.set_eye_relative(Graticule::<Target>::new(
                degrees!(0),
                degrees!(90),
                kilometers!(1),
            ))?;
            let e = c.cartesian_eye_position::<Kilometers>();
            assert_abs_diff_eq!(e.coords[0], kilometers!(-EARTH_RADIUS_KM));
            assert_abs_diff_eq!(e.coords[1], kilometers!(0));
            assert_abs_diff_eq!(e.coords[2], kilometers!(-1));
        }

        Ok(())
    }
}
