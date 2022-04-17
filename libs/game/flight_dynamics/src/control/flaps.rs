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
use pt::PlaneType;
use shape::DrawState;

#[derive(Component, NitrousComponent, Debug, Copy, Clone)]
#[Name = "flaps"]
pub struct Flaps {
    extended: bool,
}

#[inject_nitrous_component]
impl Flaps {
    pub fn new(draw_state: &mut DrawState) -> Self {
        draw_state.set_flaps(false);
        Flaps { extended: false }
    }

    pub(crate) fn sys_tick(&self, draw_state: &mut DrawState) {
        draw_state.set_flaps(self.extended)
    }

    pub fn coefficient_of_drag(&self, pt: &PlaneType) -> f32 {
        // While the coefficient would vary while flaps are being changed,
        // we don't model that as it happens very quickly (at least for
        // aircraft we care about primarily in this simulation).
        (pt.flaps_drag * self.extended as i16) as f32
    }

    #[method]
    fn toggle(&mut self) {
        self.extended = !self.extended;
    }
}
