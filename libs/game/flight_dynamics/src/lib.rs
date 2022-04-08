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

pub use crate::controls::Throttle;

use animate::TimeStep;
use anyhow::Result;
use bevy_ecs::prelude::*;
use nitrous::{inject_nitrous_component, method, NamedEntityMut, NitrousComponent};
use pt::PlaneType;
use runtime::{Extension, Runtime};
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
    pub fn install_on(mut entity: NamedEntityMut, pt: &PlaneType) {
        entity.insert_named(Throttle::new(pt));
    }

    fn sys_animate_throttle(timestep: Res<TimeStep>, mut query: Query<(&TypeRef, &mut Throttle)>) {
        for (xt, mut throttle) in query.iter_mut() {
            throttle.sys_tick(timestep.step(), xt.pt().expect("PT"));
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
