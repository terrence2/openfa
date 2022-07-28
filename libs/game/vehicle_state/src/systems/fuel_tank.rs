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
use absolute_unit::{kilograms, Kilograms, Mass};

pub(crate) struct FuelTank {
    max: Mass<Kilograms>,
    current: Mass<Kilograms>,
}

impl FuelTank {
    pub(crate) fn full(max: Mass<Kilograms>) -> Self {
        Self { max, current: max }
    }

    pub(crate) fn is_empty(&self) -> bool {
        debug_assert!(self.current.is_finite());
        self.current < kilograms!(0.001)
    }

    pub(crate) fn current(&self) -> Mass<Kilograms> {
        self.current
    }

    pub(crate) fn consume(&mut self, mass: Mass<Kilograms>) {
        self.current -= mass;
        self.current = self.current.max(kilograms!(0_f64));
    }

    pub(crate) fn override_fuel_mass(&mut self, mass: Mass<Kilograms>) {
        self.current = mass;
    }

    pub(crate) fn refuel(&mut self) {
        self.current = self.max;
    }
}
