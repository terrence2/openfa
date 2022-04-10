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

pub use crate::control::{
    airbrake::Airbrake, bay::Bay, flaps::Flaps, gear::Gear, hook::Hook, throttle::Throttle,
};
use animate::TimeStep;
use anyhow::Result;
use bevy_ecs::prelude::*;
use measure::{LocalMotion, WorldSpaceFrame};
use nitrous::{inject_nitrous_component, method, HeapMut, NamedEntityMut, NitrousComponent};
use pt::PlaneType;
use runtime::{Extension, Runtime};
use shape::{DrawState, ShapeStep};
use xt::TypeRef;

// FIXME: find a common location
const FEET_TO_METERS: f32 = 1. / 3.28084;

#[derive(Clone, Debug, Eq, PartialEq, Hash, SystemLabel)]
pub enum FlightStep {
    Simulate,
}

#[derive(Debug, Default, Component, NitrousComponent)]
#[Name = "dynamics"]
pub struct FlightDynamics {
    // Aggregate current weight with all stores and fuel
    weight_lbs: f32,
}

impl Extension for FlightDynamics {
    fn init(runtime: &mut Runtime) -> Result<()> {
        runtime.add_sim_system(Self::sys_simulate.label(FlightStep::Simulate));

        Ok(())
    }
}

#[inject_nitrous_component]
impl FlightDynamics {
    pub fn install_on(
        id: Entity,
        pt: &PlaneType,
        on_ground: bool,
        mut heap: HeapMut,
    ) -> Result<()> {
        let airbrake = Airbrake::new(&mut heap.get_mut::<DrawState>(id));
        let flaps = Flaps::new(&mut heap.get_mut::<DrawState>(id));
        let hook = Hook::new(&mut heap.get_mut::<DrawState>(id));
        let bay = Bay::new(&mut heap.get_mut::<DrawState>(id));
        let gear = Gear::new(on_ground, &mut heap.get_mut::<DrawState>(id));
        let throttle = Throttle::new(pt, &mut heap.get_mut::<DrawState>(id));
        heap.named_entity_mut(id)
            .insert_named(airbrake)?
            .insert_named(flaps)?
            .insert_named(hook)?
            .insert_named(gear)?
            .insert_named(bay)?
            .insert_named(throttle)?
            .insert_named(FlightDynamics::default())?;
        Ok(())
    }

    pub fn weight_lbs(&self) -> f32 {
        self.weight_lbs
    }

    fn sys_simulate(
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
            &mut LocalMotion,
            &mut WorldSpaceFrame,
            &mut FlightDynamics,
        )>,
    ) {
        for (
            xt,
            airbrake,
            flaps,
            hook,
            mut bay,
            mut gear,
            mut throttle,
            mut draw_state,
            mut motion,
            mut frame,
            mut dynamics,
        ) in query.iter_mut()
        {
            let dt = timestep.step();
            let pt = xt.pt().expect("PT");

            // Update states of all flight controls
            airbrake.sys_tick(&mut draw_state);
            flaps.sys_tick(&mut draw_state);
            hook.sys_tick(&mut draw_state);
            bay.sys_tick(dt, &mut draw_state);
            gear.sys_tick(dt, &mut draw_state);
            throttle.sys_tick(dt, pt, &mut draw_state);

            // FIXME: do not consume fuel internally if there are drop tanks
            throttle.consume_fuel(dt, pt);

            // FIXME: add armament weights
            // FIXME: subtract force of drag
            let drag_lbf = 0.;
            let thrust_lbf = throttle.compute_thrust(pt);
            let weight_lbs = pt.nt.ot.empty_weight as f32 + throttle.internal_fuel_lbs() as f32;

            // F=ma
            // a = F/m
            // (lb*ft/s^2) / lb => ft/s^2
            let accel_m_ss = ((thrust_lbf - drag_lbf) / weight_lbs) * FEET_TO_METERS; // m/s^2
            motion.acceleration_m_s2_mut().z = accel_m_ss as f64;
            *motion.forward_velocity_mut() += accel_m_ss as f64 * dt.as_secs_f64();

            // rotate motion into world space frame and apply to position.
            let velocity_m_s = frame.facing() * motion.meters_per_second();
            let world_pos = frame.position().point64() - velocity_m_s * dt.as_secs_f64();
            frame.set_position(world_pos);

            dynamics.weight_lbs = weight_lbs;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {}
}
