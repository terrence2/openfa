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
use absolute_unit::{Kilograms, Mass};
use animate::TimeStep;
use anyhow::Result;
use bevy_ecs::prelude::*;
use nitrous::{inject_nitrous_component, method, HeapMut, NitrousComponent};
use runtime::{Extension, Runtime};

#[derive(Clone, Debug)]
pub struct Munition {}

#[derive(Clone, Debug)]
pub struct FuelTank {}

#[derive(Clone, Debug)]
pub struct PowerPlant {}

#[derive(Clone, Debug)]
pub struct Inertia {}

#[derive(Clone, Debug, Eq, PartialEq, Hash, SystemLabel)]
pub enum VehicleStep {
    UpdateState,
}

/// Maintain machine related properties in common between all vehicle types, independent
/// of the specific dynamics that are being applied. This is things like the fuel levels,
/// stores levels, inertial tensors, etc.
#[derive(Debug, Component, NitrousComponent)]
#[Name = "vehicle"]
pub struct VehicleState {
    // self pointer for updating markers
    id: Entity,

    // Simulation of aggregate engines for this vehicle
    power_plant: PowerPlant,

    // Aggregate current mass with all stores and fuel
    empty_mass: Mass<Kilograms>,
    current_mass: Mass<Kilograms>,

    // Stores
    munitions: Vec<Munition>,
    internal_fuel: Vec<FuelTank>,
    external_fuel: Vec<FuelTank>,

    // Inertial tensor: distribution of the masses of the above
    inertia: Inertia,
}

impl Extension for VehicleState {
    fn init(runtime: &mut Runtime) -> Result<()> {
        runtime.add_sim_system(Self::sys_update_state.label(VehicleStep::UpdateState));
        Ok(())
    }
}

#[inject_nitrous_component]
impl VehicleState {
    fn sys_update_state(timestep: Res<TimeStep>, mut query: Query<(&mut VehicleState)>) {
        for vehicle in query.iter() {
            println!("Consume!")
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
