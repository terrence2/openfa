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
mod controls;
mod inertia;
mod systems;

pub use crate::{
    controls::throttle::Throttle,
    inertia::{Inertia, InertiaTensor},
};

use crate::systems::{fuel_tank::FuelTank, power_plant::PowerPlant};
use absolute_unit::{kilograms, scalar, Kilograms, Mass, PoundsMass};
use animate::TimeStep;
use anyhow::Result;
use bevy_ecs::prelude::*;
use nitrous::{inject_nitrous_component, method, HeapMut, NitrousComponent};
use runtime::{Extension, Runtime};
use shape::{DrawState, ShapeBuffer, ShapeId};
use xt::TypeRef;

#[derive(Clone, Debug)]
pub struct Munition {}

#[derive(Clone, Debug, Eq, PartialEq, Hash, SystemLabel)]
pub enum VehicleStep {
    UpdateState,
}

/// Maintain machine related properties in common between all vehicle types, independent
/// of the specific dynamics that are being applied. This is things like the fuel levels,
/// stores levels, inertial tensors, etc.
#[derive(Component, NitrousComponent)]
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
    internal_fuel: Option<FuelTank>,
    // external_fuel: Vec<FuelTank>,
    // munitions: Vec<Munition>,

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
    pub fn new(id: Entity, xt: &TypeRef, mut heap: HeapMut) -> Self {
        let empty_mass = kilograms!(xt.ot().empty_weight);
        let internal_fuel = xt
            .pt()
            .map(|pt| FuelTank::full(kilograms!(pt.internal_fuel)));
        // TODO: fuel weight
        let current_mass = empty_mass;
        let power_plant = PowerPlant::new_min_power(xt, &mut heap.get_mut::<DrawState>(id));
        let inertia = Inertia::from_extent(
            xt.ot(),
            heap.resource::<ShapeBuffer>()
                .metadata(*heap.get::<ShapeId>(id))
                .read()
                .extent(),
        );
        Self {
            id,
            power_plant,
            empty_mass,
            current_mass,
            internal_fuel,
            // external_fuel: Vec::new(),
            // munitions: Vec::new(),
            inertia,
        }
    }

    pub fn install_on(id: Entity, xt: &TypeRef, mut heap: HeapMut) -> Result<()> {
        let throttle = Throttle::new_min_power();
        let vehicle = VehicleState::new(id, xt, heap.as_mut());
        heap.named_entity_mut(id)
            .insert_named(throttle)?
            .insert_named(vehicle)?;
        Ok(())
    }

    pub fn set_internal_fuel_lbs(&mut self, fuel: Mass<PoundsMass>) {
        if let Some(tank) = self.internal_fuel.as_mut() {
            tank.override_fuel_mass(kilograms!(fuel));
        }
    }

    pub fn current_mass(&self) -> Mass<Kilograms> {
        self.current_mass
    }

    #[method]
    pub fn is_out_of_fuel(&self) -> bool {
        self.internal_fuel
            .as_ref()
            .map(|tank| tank.is_empty())
            .unwrap_or(false)
    }

    #[method]
    pub fn refuel(&mut self) {
        if let Some(tank) = self.internal_fuel.as_mut() {
            tank.refuel();
        }
    }

    pub fn power_plant(&self) -> &PowerPlant {
        &self.power_plant
    }

    pub fn inertia(&self) -> &Inertia {
        &self.inertia
    }

    pub fn inertia_tensor(&self) -> InertiaTensor {
        self.inertia.recompute_tensor(
            self.internal_fuel
                .as_ref()
                .map(|tank| tank.current())
                .unwrap_or_else(|| kilograms!(0_f64)),
        )
    }

    fn sys_update_state(
        timestep: Res<TimeStep>,
        mut query: Query<(&mut VehicleState, &mut DrawState, &Throttle)>,
    ) {
        for (mut vehicle, mut draw_state, throttle) in query.iter_mut() {
            // Perform engine spooling to move towards target power
            vehicle
                .power_plant
                .sys_tick(throttle, timestep.step(), &mut draw_state);

            // Consume fuel
            // TODO: drop tanks
            let fuel_used = vehicle.power_plant.fuel_consumption(timestep.step());
            let (fuel_mass, is_empty) = if let Some(tank) = vehicle.internal_fuel.as_mut() {
                tank.consume(fuel_used);
                (tank.current(), tank.is_empty())
            } else {
                (kilograms!(0_f64), false)
            };
            if is_empty {
                vehicle.power_plant.set_out_of_fuel();
            }
            vehicle.current_mass = vehicle.empty_mass + fuel_mass;
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
