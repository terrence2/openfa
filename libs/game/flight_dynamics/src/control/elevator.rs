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
use bevy_ecs::prelude::*;
use nitrous::{inject_nitrous_component, method, NitrousComponent};
use shape::DrawState;

#[derive(Component, NitrousComponent, Debug, Default, Copy, Clone)]
#[Name = "elevator"]
pub struct Elevator {
    position: f64, // [-1, 1]
}

#[inject_nitrous_component]
impl Elevator {
    #[method]
    pub fn position(&self) -> f64 {
        self.position as f64
    }

    #[allow(unused)]
    pub(crate) fn update_position(&mut self, position: f64, draw_state: &mut DrawState) {
        self.position = position;
        if self.position > 0.1 {
            draw_state.move_elevator_up();
        } else if self.position < 0.1 {
            draw_state.move_elevator_down();
        } else {
            draw_state.move_elevator_center();
        }
    }
}
