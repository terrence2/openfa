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
use crate::controls::ThrottlePosition::Military;
use bevy_ecs::prelude::*;
use nitrous::{inject_nitrous_component, method, NitrousComponent};
use pt::PlaneType;
use std::{num::NonZeroU32, time::Duration};

const AFTERBURNER_ENABLE_SOUND: &'static str = "&AFTBURN.11K";

#[derive(Debug, Copy, Clone)]
enum ThrottlePosition {
    Military(f32),
    Afterburner,
}

impl ThrottlePosition {
    fn military(&self) -> f32 {
        match self {
            Self::Military(m) => *m,
            Self::Afterburner => 101.,
        }
    }

    fn is_afterburner(&self) -> bool {
        matches!(self, Self::Afterburner)
    }

    fn increase(&mut self, delta: f32, max: ThrottlePosition) {
        match self {
            Self::Military(current) => {
                let next = (*current + delta).min(max.military());
                *self = if next >= 100. && max.is_afterburner() {
                    // TODO: return a new afterburner state so we can play sound?
                    Self::Afterburner
                } else {
                    Self::Military(next)
                };
            }
            Self::Afterburner => {}
        }
    }

    fn decrease(&mut self, delta: f32, min: ThrottlePosition) {
        if self.is_afterburner() {
            *self = Self::Military(100.);
        }
        if let Self::Military(current) = self {
            let next = (*current - delta).max(min.military());
            *self = Self::Military(next);
        }
    }
}

#[derive(Component, NitrousComponent, Debug, Copy, Clone)]
#[Name = "throttle"]
pub struct Throttle {
    throttle_position: ThrottlePosition,
    engine_position: ThrottlePosition,
    internal_fuel: f32,
}

#[inject_nitrous_component]
impl Throttle {
    pub fn new(pt: &PlaneType) -> Self {
        Throttle {
            throttle_position: ThrottlePosition::Military(0.),
            engine_position: ThrottlePosition::Military(0.),
            internal_fuel: pt.internal_fuel as f32,
        }
    }

    pub(crate) fn sys_tick(&mut self, dt: &Duration, pt: &PlaneType) {
        if self.engine_position.military() < self.throttle_position.military() {
            self.engine_position.increase(
                pt.throttle_acc as f32 * dt.as_secs_f32(),
                self.throttle_position,
            );
        }
        if self.engine_position.military() > self.throttle_position.military() {
            self.engine_position.decrease(
                pt.throttle_dacc as f32 * dt.as_secs_f32(),
                self.throttle_position,
            );
        }
        println!("{:?} of {:?}", self.engine_position, self.throttle_position);
    }

    #[method]
    fn set_detent(&mut self, detent: i64) {
        self.throttle_position = match detent {
            0 => ThrottlePosition::Military(0.),
            1 => ThrottlePosition::Military(25.),
            2 => ThrottlePosition::Military(50.),
            3 => ThrottlePosition::Military(75.),
            4 => ThrottlePosition::Military(100.),
            5 => ThrottlePosition::Afterburner,
            _ => ThrottlePosition::Military(100.),
        };
    }
}
