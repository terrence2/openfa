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
mod control;

use crate::control::{
    airbrake::Airbrake, bay::Bay, flaps::Flaps, gear::Gear, hook::Hook, throttle::Throttle,
};
use animate::TimeStep;
use anyhow::Result;
use bevy_ecs::prelude::*;
use nitrous::{inject_nitrous_component, method, HeapMut, NamedEntityMut, NitrousComponent};
use pt::PlaneType;
use runtime::{Extension, Runtime};
use shape::DrawState;
use xt::TypeRef;

#[derive(Clone, Debug, Eq, PartialEq, Hash, SystemLabel)]
pub enum FlightStep {
    SimThrottle,
    Simulate,
}

#[derive(Debug, Component, NitrousComponent)]
pub struct FlightDynamics {}

impl Extension for FlightDynamics {
    fn init(runtime: &mut Runtime) -> Result<()> {
        runtime.add_sim_system(Self::sys_animate_throttle.label(FlightStep::SimThrottle));

        runtime.add_sim_system(
            Self::sys_simulate_flight
                .label(FlightStep::Simulate)
                .after(FlightStep::SimThrottle),
        );

        Ok(())
    }
}

#[inject_nitrous_component]
impl FlightDynamics {
    pub fn install_on(id: Entity, pt: &PlaneType, mut heap: HeapMut) -> Result<()> {
        let airbrake = Airbrake::new(&mut heap.get_mut::<DrawState>(id));
        let flaps = Flaps::new(&mut heap.get_mut::<DrawState>(id));
        let hook = Hook::new(&mut heap.get_mut::<DrawState>(id));
        let bay = Bay::new(&mut heap.get_mut::<DrawState>(id));
        let gear = Gear::new(&mut heap.get_mut::<DrawState>(id));
        let throttle = Throttle::new(pt, &mut heap.get_mut::<DrawState>(id));
        heap.named_entity_mut(id)
            .insert_named(airbrake)?
            .insert_named(flaps)?
            .insert_named(hook)?
            .insert_named(gear)?
            .insert_named(bay)?
            .insert_named(throttle)?;
        Ok(())
    }

    fn sys_animate_throttle(
        timestep: Res<TimeStep>,
        mut query: Query<(
            &TypeRef,
            &Airbrake,
            &Flaps,
            &Hook,
            &mut Bay,
            &mut Gear,
            &mut Throttle,
            &mut DrawState,
        )>,
    ) {
        for (xt, airbrake, flaps, hook, mut bay, mut gear, mut throttle, mut draw_state) in
            query.iter_mut()
        {
            airbrake.sys_tick(&mut draw_state);
            flaps.sys_tick(&mut draw_state);
            hook.sys_tick(&mut draw_state);
            bay.sys_tick(timestep.step(), &mut draw_state);
            gear.sys_tick(timestep.step(), &mut draw_state);
            throttle.sys_tick(timestep.step(), xt.pt().expect("PT"), &mut draw_state);
        }
    }

    fn sys_simulate_flight(query: Query<(&FlightDynamics, &Throttle)>) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {}
}
