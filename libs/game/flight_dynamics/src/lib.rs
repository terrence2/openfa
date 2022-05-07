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
use absolute_unit::{
    feet, feet2, feet_per_second, meters2, meters_per_second, meters_per_second2, pounds_force,
    pounds_weight, scalar, seconds, Acceleration, Feet, Force, Kilograms, Mass, Meters, Newtons,
    PoundsForce, PoundsMass, PoundsWeight, Seconds, Velocity, Weight,
};
use animate::TimeStep;
use anyhow::Result;
use bevy_ecs::prelude::*;
use measure::{LocalMotion, WorldSpaceFrame};
use nitrous::{inject_nitrous_component, method, HeapMut, NamedEntityMut, NitrousComponent};
use physical_constants::{StandardAtmosphere, FEET_TO_M_32, GRAVITY_M_S2_32, METERS_TO_FEET_32};
use pt::PlaneType;
use runtime::{Extension, Runtime};
use shape::{DrawState, ShapeStep};
use xt::TypeRef;

#[derive(Clone, Debug, Eq, PartialEq, Hash, SystemLabel)]
pub enum FlightStep {
    Simulate,
}

#[derive(Debug, Default, Component, NitrousComponent)]
#[Name = "dynamics"]
pub struct FlightDynamics {
    // Aggregate current weight with all stores and fuel
    weight_lbs: Weight<PoundsWeight>,
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

    pub fn weight(&self) -> Weight<PoundsWeight> {
        self.weight_lbs
    }

    pub fn weight_lbs(&self) -> f32 {
        self.weight_lbs.f32()
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
            let grat = frame.position_graticule();

            // Update states of all flight controls
            airbrake.sys_tick(&mut draw_state);
            flaps.sys_tick(&mut draw_state);
            hook.sys_tick(&mut draw_state);
            bay.sys_tick(dt, &mut draw_state);
            gear.sys_tick(dt, &mut draw_state);
            throttle.sys_tick(dt, pt, &mut draw_state);

            // FIXME: do not consume fuel internally if there are drop tanks
            throttle.consume_fuel(dt, pt);

            // What is our max g-loading?
            pt.envelopes
                .find_g_load_extrema(motion.forward_velocity(), grat.distance);

            // F{drag} = 0.5 * C{drag} * p * v**2 * A
            //                           Kg/m^3 * m/s * m/s * m^2 => m*Kg/s^2
            // FA accounts for A as part of the coefficient.
            // We can assume that the units probably match thrust units, which are ft*lb/s^2
            // FIXME: add in _gpull_drag; e.g. induced drag coefficient
            let coef_drag = scalar!(
                (pt.coef_drag as f32
                    + airbrake.coefficient_of_drag(pt)
                    + flaps.coefficient_of_drag(pt)
                    + hook.coefficient_of_drag(pt)
                    + bay.coefficient_of_drag(pt)
                    + gear.coefficient_of_drag(pt))
                // Cd (drag coefficient) is the same for all PT! It's probably not that huge
                // a differentiator between aircraft models compared to induced drag, thrust
                // and other factors, so makes some sense. Typical Cd are 0.01 to 0.02 range,
                // whereas the sum above is going to be 256 + modifiers.
                // Typical drags are on the order of 0.01 to 0.03. Divide by ~10_000.
                / 10_000.
            );

            let atmosphere = StandardAtmosphere::at_altitude(grat.distance);
            let forward_velocity_m_s: Velocity<Meters, Seconds> =
                meters_per_second!(motion.forward_velocity());
            // lb/ft^3 * ft/s * ft/s * ft^2 => lb*ft/s^2
            let drag_lbf: Force<Newtons> = (scalar!(0.5).as_dyn()
                * coef_drag.as_dyn()
                * atmosphere.density::<Kilograms, Meters>().as_dyn()
                * forward_velocity_m_s.as_dyn()
                * forward_velocity_m_s.as_dyn()
                * meters2!(1f64).as_dyn())
            .into(); // area (usually pre-mixed with Cd for aircraft)
                     // let drag_lbf = pounds_force!(0i8);

            // FIXME: add armament weights
            let thrust_lbf = throttle.compute_thrust(pt);
            let weight_lb = pounds_weight!(pt.nt.ot.empty_weight) + throttle.internal_fuel();

            // a=F/m
            let accel_m_ss: Acceleration<Meters, Seconds> =
                meters_per_second2!((thrust_lbf - drag_lbf) / weight_lb.mass::<PoundsMass>());
            motion.acceleration_m_s2_mut().z = accel_m_ss;
            *motion.forward_velocity_mut() += accel_m_ss * seconds!(dt.as_secs_f64());

            // rotate motion into world space frame and apply to position.
            let velocity_m_s = (frame.facing() * motion.velocity_m_s().map(|v| v.f64()))
                .map(|v| meters_per_second!(v));
            let world_pos =
                frame.position_pt3() - velocity_m_s.map(|v| v * seconds!(dt.as_secs_f64()));
            frame.set_position(world_pos);

            dynamics.weight_lbs = weight_lb;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {}
}
