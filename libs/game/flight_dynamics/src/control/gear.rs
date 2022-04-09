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
use std::{num::NonZeroU32, time::Duration};

#[derive(Clone, Copy, Debug)]
enum GearPosition {
    Open,
    Closed,
    Opening(f32),
    Closing(f32),
}

#[derive(Component, NitrousComponent, Debug, Copy, Clone)]
#[Name = "gear"]
pub struct Gear {
    position: GearPosition,
    open_speed: f32,
    close_speed: f32,
}

#[inject_nitrous_component]
impl Gear {
    pub fn new(draw_state: &mut DrawState) -> Self {
        draw_state.set_gear_visible(false);
        draw_state.set_gear_position(0.);
        Gear {
            position: GearPosition::Closed,
            // TODO: is this specified?
            open_speed: 1. / 5.,
            close_speed: 1. / 5.,
        }
    }

    pub(crate) fn sys_tick(&mut self, dt: &Duration, draw_state: &mut DrawState) {
        self.position = match self.position {
            GearPosition::Opening(f0) => {
                let f = f0 + self.open_speed * dt.as_secs_f32();
                draw_state.set_gear_visible(true);
                if f >= 1. {
                    draw_state.set_gear_position(1.);
                    GearPosition::Open
                } else {
                    draw_state.set_gear_position(f);
                    GearPosition::Opening(f)
                }
            }
            GearPosition::Closing(f0) => {
                let f = f0 - self.close_speed * dt.as_secs_f32();
                if f <= 0. {
                    draw_state.set_gear_visible(false);
                    draw_state.set_gear_position(0.);
                    GearPosition::Closed
                } else {
                    draw_state.set_gear_visible(true);
                    draw_state.set_gear_position(f);
                    GearPosition::Closing(f)
                }
            }
            p => p,
        }
    }

    #[method]
    fn toggle(&mut self) {
        self.position = match self.position {
            GearPosition::Open => GearPosition::Closing(1.),
            GearPosition::Closed => GearPosition::Opening(0.),
            GearPosition::Opening(f) => GearPosition::Closing(f),
            GearPosition::Closing(f) => GearPosition::Opening(f),
        }
    }
}
