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
use crate::control::surface_position_tick;
use bevy_ecs::prelude::*;
use nitrous::{inject_nitrous_component, method, NitrousComponent};
use shape::DrawState;
use std::time::Duration;

#[derive(Component, NitrousComponent, Debug, Default, Copy, Clone)]
#[Name = "rudder"]
pub struct Rudder {
    position: f64,        // [-1, 1]
    key_move_target: f64, // target of move, depending on what key is held
}

#[inject_nitrous_component]
impl Rudder {
    #[method]
    pub fn move_pedals_left(&mut self, pressed: bool) {
        self.key_move_target = if pressed { -1. } else { 0. };
    }

    #[method]
    pub fn move_pedals_right(&mut self, pressed: bool) {
        self.key_move_target = if pressed { 1. } else { 0. };
    }

    #[method]
    pub fn position(&self) -> f64 {
        self.position as f64
    }

    pub(crate) fn sys_tick(&mut self, dt: &Duration, draw_state: &mut DrawState) {
        self.position =
            surface_position_tick(self.key_move_target, dt.as_secs_f64(), self.position);

        if self.position > 0.1 {
            draw_state.move_rudder_right();
        } else if self.position < -0.1 {
            draw_state.move_rudder_left();
        } else {
            draw_state.move_rudder_center();
        }
    }
}
