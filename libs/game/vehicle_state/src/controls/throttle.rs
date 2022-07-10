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

#[derive(Debug, Copy, Clone)]
pub enum ThrottlePosition {
    Military(f32),
    Afterburner,
}

impl ThrottlePosition {
    pub(crate) fn new_min_power() -> Self {
        Self::Military(0.)
    }

    pub(crate) fn military(&self) -> f32 {
        match self {
            Self::Military(m) => *m,
            Self::Afterburner => 101.,
        }
    }

    pub(crate) fn is_afterburner(&self) -> bool {
        matches!(self, Self::Afterburner)
    }
}

impl ToString for ThrottlePosition {
    fn to_string(&self) -> String {
        match self {
            Self::Afterburner => "AFT".to_owned(),
            Self::Military(m) => format!("{:0.0}%", m),
        }
    }
}

#[derive(Component, NitrousComponent, Debug, Copy, Clone)]
#[Name = "throttle"]
pub struct Throttle {
    position: ThrottlePosition,
}

#[inject_nitrous_component]
impl Throttle {
    pub fn new_min_power() -> Self {
        Throttle {
            position: ThrottlePosition::Military(0.),
        }
    }

    pub fn throttle_display(&self) -> String {
        self.position.to_string()
    }

    pub fn position(&self) -> &ThrottlePosition {
        &self.position
    }

    #[method]
    fn set_detent(&mut self, detent: i64) {
        self.position = match detent {
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
