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
use absolute_unit::{degrees, meters, radians, scalar, LengthUnit, Meters};
use geodesy::{Cartesian, GeoCenter, Graticule, Target};
use measure::WorldSpaceFrame;
use nalgebra::{Unit as NUnit, UnitQuaternion, Vector3};
use std::f64::consts::PI;

#[derive(Debug)]
struct InputState {
    in_rotate: bool,
}

/// The external camera is a fixed-upwards-orientation arc-ball around.
/// It tries to duplicate FA's kludgy keyboard controls while also offering
/// nice modern mouse control for those who can bother to reach for one.
#[derive(Debug)]
pub struct ExternalCameraController {
    eye: Graticule<Target>,
    input: InputState,
}

impl Default for ExternalCameraController {
    fn default() -> Self {
        Self {
            input: InputState { in_rotate: false },
            eye: Graticule::<Target>::new(
                radians!(degrees!(10.)),
                radians!(degrees!(25.)),
                meters!(10.),
            ),
        }
    }
}

impl ExternalCameraController {
    pub fn get_frame(&self, player_frame: &WorldSpaceFrame) -> WorldSpaceFrame {
        let eye = self.cartesian_eye_position(player_frame);
        let forward = (*player_frame.position() - eye).vec64();
        WorldSpaceFrame::new(eye, forward)

        // /// FIXME: use arcball somehow
        // let player_pos = player_frame.position().vec64();
        // let basis = player_frame.basis();
        // let mut pos = player_pos + (basis.forward * 100.);
        // pos = pos + (basis.up * 20.);
        //
        // WorldSpaceFrame::from_quaternion(
        //     Cartesian::<GeoCenter, Meters>::from(pos),
        //     UnitQuaternion::face_towards(&(player_pos - pos), &basis.up),
        // )
    }

    fn cartesian_eye_position(
        &self,
        player_frame: &WorldSpaceFrame,
    ) -> Cartesian<GeoCenter, Meters> {
        let target = Graticule::<GeoCenter>::from(player_frame.position());
        let r_lon = UnitQuaternion::from_axis_angle(
            &NUnit::new_unchecked(Vector3::new(0f64, 1f64, 0f64)),
            -f64::from(target.longitude),
        );
        let r_lat = UnitQuaternion::from_axis_angle(
            &NUnit::new_normalize(r_lon * Vector3::new(1f64, 0f64, 0f64)),
            PI / 2.0 - f64::from(target.latitude),
        );
        let cart_target = *player_frame.position();
        let cart_eye_rel_target_flat = Cartesian::<Target, Meters>::from(self.eye);
        let cart_eye_rel_target_framed =
            Cartesian::<Target, Meters>::from(r_lat * r_lon * cart_eye_rel_target_flat.vec64());
        cart_target + cart_eye_rel_target_framed
    }

    pub fn set_pan_view(&mut self, pressed: bool) {
        self.input.in_rotate = pressed;
    }

    pub fn handle_mousemotion(&mut self, dx: f64, dy: f64) {
        if self.input.in_rotate {
            self.eye.longitude -= degrees!(dx * 0.5);

            self.eye.latitude += degrees!(dy * 0.5f64);
            self.eye.latitude = self
                .eye
                .latitude
                .min(radians!(PI / 2.0 - 0.001))
                .max(radians!(-PI / 2.0 + 0.001));
        }
    }

    pub fn handle_mousewheel(&mut self, delta: f64) {
        self.eye.distance *= scalar!(if delta > 0f64 { 1.1f64 } else { 0.9f64 });
        self.eye.distance = self.eye.distance.max(meters!(0.01f64));
    }
}
