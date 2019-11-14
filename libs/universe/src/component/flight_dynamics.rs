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
use nalgebra::Vector3;
use specs::{Component, VecStorage};

pub struct FlightDynamics {
    #[allow(dead_code)]
    velocity: Vector3<f64>,
}

impl Component for FlightDynamics {
    type Storage = VecStorage<Self>;
}

impl FlightDynamics {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            velocity: Vector3::zeros(),
        }
    }
}
