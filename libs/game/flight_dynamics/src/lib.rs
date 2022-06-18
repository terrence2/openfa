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
    airbrake::Airbrake, bay::Bay, elevator::Elevator, flaps::Flaps, gear::Gear, hook::Hook,
    throttle::Throttle,
};
use absolute_unit::{
    kilograms_meter2, meters, meters2, meters_per_second, meters_per_second2, newton_meters,
    newtons, pounds_weight, radians, radians_per_second, scalar, seconds, Acceleration,
    AngularAcceleration, AngularVelocity, Force, Kilograms, Meters, Newtons, PoundsMass,
    PoundsWeight, Radians, Seconds, Torque, Velocity, Weight,
};
use animate::TimeStep;
use anyhow::Result;
use bevy_ecs::prelude::*;
use geodesy::{GeoCenter, GeoSurface, Graticule};
use measure::{BodyMotion, WorldSpaceFrame};
use nalgebra::UnitQuaternion;
use nitrous::{inject_nitrous_component, HeapMut, NitrousComponent};
use physical_constants::StandardAtmosphere;
use pt::{GloadExtrema, PlaneType};
use runtime::{Extension, Runtime};
use shape::{DrawState, ShapeStep};
use xt::TypeRef;

#[derive(Clone, Debug, Eq, PartialEq, Hash, SystemLabel)]
pub enum FlightStep {
    Simulate,
}

#[derive(Debug, Component, NitrousComponent)]
#[Name = "dynamics"]
pub struct FlightDynamics {
    // Aggregate current weight with all stores and fuel
    weight_lbs: Weight<PoundsWeight>,

    // Current envelope
    max_g_load: GloadExtrema,
}

impl Extension for FlightDynamics {
    fn init(runtime: &mut Runtime) -> Result<()> {
        runtime.add_sim_system(Self::sys_simulate.label(FlightStep::Simulate));

        Ok(())
    }
}

#[inject_nitrous_component]
impl FlightDynamics {
    pub fn new() -> Self {
        Self {
            weight_lbs: pounds_weight!(0.),
            max_g_load: GloadExtrema::Stall(0.),
        }
    }

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
        let elevator = Elevator::default();
        heap.named_entity_mut(id)
            .insert_named(airbrake)?
            .insert_named(flaps)?
            .insert_named(hook)?
            .insert_named(gear)?
            .insert_named(bay)?
            .insert_named(throttle)?
            .insert_named(elevator)?
            .insert_named(FlightDynamics::new())?;
        Ok(())
    }

    pub fn weight(&self) -> Weight<PoundsWeight> {
        self.weight_lbs
    }

    pub fn weight_lbs(&self) -> f32 {
        self.weight_lbs.f32()
    }

    pub fn max_g_load(&self) -> GloadExtrema {
        self.max_g_load
    }

    // Algorithm taken from David Allerton's Principles of Flight Simulation.
    fn simulate(
        &mut self,
        timestep: &TimeStep,
        xt: &TypeRef,
        airbrake: &Airbrake,
        flaps: &Flaps,
        hook: &Hook,
        bay: &mut Bay,
        gear: &mut Gear,
        throttle: &mut Throttle,
        elevator: &mut Elevator,
        draw_state: &mut DrawState,
        motion: &mut BodyMotion,
        frame: &mut WorldSpaceFrame,
    ) {
        let dt = timestep.step();
        let pt = xt.pt().expect("PT");

        // Update states of all flight controls
        airbrake.sys_tick(draw_state);
        flaps.sys_tick(draw_state);
        hook.sys_tick(draw_state);
        bay.sys_tick(dt, draw_state);
        gear.sys_tick(dt, draw_state);
        throttle.sys_tick(dt, pt, draw_state);
        elevator.sys_tick(dt, draw_state);

        // fwd   axis: X, u, L, p
        // right axis: Y, v, M, q
        // down  axis: Z, w, N, r

        let grat = frame.position_graticule();

        let atmosphere = StandardAtmosphere::at_altitude(grat.distance);

        let mut u = motion.forward_velocity();
        let v = motion.sideways_velocity();
        let w = motion.vertical_velocity();
        let mut q = motion.pitch_velocity();
        let p = motion.roll_velocity();
        let r = motion.yaw_velocity();
        let uw_mag = (u * u + w * w).sqrt(); // m/s
        let uvw_2 = u * u + v * v + w * w; // m^2/s^2
        let velocity_cg = uvw_2.sqrt();
        let mut u_dot = motion.forward_acceleration(); // u*
        let v_dot = motion.sideways_acceleration(); // v*
        let w_dot = motion.vertical_acceleration(); // w*
        let alpha = radians!((w / u).atan());
        let beta = radians!((v / uw_mag).atan());

        // Alpha_dot and beta_dot are trig approximations, so the units intentionally don't
        // make much of any sense.
        let alpha_dot =
            radians_per_second!((u.f64() * w_dot.f64() - w.f64() * u_dot.f64()) / uw_mag.f64());
        // v* * sqrt(u^2 + w^2) - v * (u * u* + w * w*)
        // --------------------------------------------
        // sqrt(u^2 + w^2) * (u^2 + v^2 + w^2)
        let beta_dot_numerator_1 = v_dot.f64() * uw_mag.f64();
        let beta_dot_numerator_2 = v.f64() * (u.f64() * u_dot.f64() + w.f64() * w_dot.f64());
        let beta_dot_numerator = beta_dot_numerator_1 - beta_dot_numerator_2;
        let beta_dot_denominator = uw_mag.f64() * uvw_2.f64();
        let beta_dot = radians_per_second!(beta_dot_numerator / beta_dot_denominator);

        // //////////////////// THRUST ////////////////////
        // FIXME: do not consume fuel internally if there are drop tanks
        // TODO: better model engine behavior based on atmosphere and nozzle velocity
        throttle.consume_fuel(dt, pt);
        let thrust = throttle.compute_thrust::<Newtons>(pt);
        let weight_lb = pounds_weight!(pt.nt.ot.empty_weight) + throttle.internal_fuel();
        let mass_kg = weight_lb.mass::<Kilograms>();
        let engine_moment_pitch = newton_meters!(0f64);

        // //////////////////// LIFT ////////////////////
        // FA does not track coefficients of lift. In general, planes always fly straight in FA
        // unless they are about to stall out. In effect, we are simulating automatic trim,
        // such that the plane's stability axis is managed such that the lift always produces straight
        // flight. This is only untrue if the max g-loading is < 1.0 (or whatever is required
        // for the current flight regime).
        self.max_g_load = pt
            .envelopes
            .find_g_load_maxima(motion.forward_velocity(), grat.distance);
        // I think this is the aoa at max g, so use the alpha that we know from current state
        // to determine the g's we are currently pulling, use that as the lift magnitude.
        let lift_g_ratio = alpha.f64() / f64::from(pt.gpull_aoa);
        // FIXME: keep us in lift regime somehow, or make this smoother, or both.
        let lift_gs = if lift_g_ratio > 1_f64 || lift_g_ratio < -1_f64 {
            0_f64
        } else if lift_g_ratio > 1_f64 {
            // alpha of 0 maps to g-load of 1, so remap in ranges.
            // For C-130 with max load of 3
            // 0.0 loading => 1G
            // 0.5 loading => 2G
            // 1.0 loading => 3G
            1_f64 + lift_g_ratio * (f64::from(pt.envelopes.max_g_load()) - 1_f64)
        } else if lift_g_ratio < 1_f64 {
            // For C-130, with min load of -2
            // -0.0 loading => 1G
            // -0.33 loading => 0G
            // -0.66 loading => -1G
            // -1.0 loading => -2G
            1_f64 - lift_g_ratio * (f64::from(pt.envelopes.min_g_load()) - 1_f64)
        } else {
            1_f64
        };
        // Convert lift in G to lift in force through CG
        // Given F=ma, multiply by the current weight.
        let lift = newtons!(lift_gs * mass_kg.f64());

        // //////////////////// DRAG ////////////////////
        // F{drag} = 0.5 * C{drag} * p * v**2 * A
        //                           Kg/m^3 * m/s * m/s * m^2 => m*Kg/s^2
        // A is usually assumed to be 1, and FA doesn't track it anyway.
        // We can assume that the units probably match thrust units, which are ft*lb/s^2
        // FIXME: add in _gpull_drag; e.g. induced drag coefficient
        // FIXME: add in control surface drag; e.g. rudder_drag
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
                // / 10_000.
                / 8_192.
        );
        let drag: Force<Newtons> = (scalar!(0.5).as_dyn()
            * coef_drag.as_dyn()
            * atmosphere.density::<Kilograms, Meters>().as_dyn()
            * (u * u).as_dyn()
            * meters2!(1f64).as_dyn())
        .into();

        // //////////////////// PITCH ////////////////////
        let coef_m0 = radians!(0f64);
        let coef_malpha = scalar!(0.);
        let coef_mde = scalar!(1.);
        let coef_mq = scalar!(0.);
        let coef_malphadot = scalar!(1.);
        let dist_cg_to_lift = meters!(0f64);
        let gear_moment_pitch = newton_meters!(0f64);

        let m_stab: Torque<Newtons, Meters> = ((scalar!(0.5).as_dyn()
            * atmosphere.density::<Kilograms, Meters>().as_dyn()
            * velocity_cg.as_dyn()
            * velocity_cg.as_dyn()
            * meters2!(1f64).as_dyn() // s
            * meters!(1f64).as_dyn() // c_bar
            * radians!(
                    coef_m0 + coef_malpha * radians!(alpha) + coef_mde * radians!(elevator.position())
                ).as_dyn())
            + (
            scalar!(0.25).as_dyn()
                * atmosphere.density::<Kilograms, Meters>().as_dyn()
                * velocity_cg.as_dyn()
                * meters2!(1f64).as_dyn() // s
                * meters!(1f64).as_dyn() // c_bar
                * meters!(1f64).as_dyn() // c_bar
                * (coef_mq.as_dyn() * q.as_dyn()
                + coef_malphadot.as_dyn() * alpha_dot.as_dyn())
        )).into();

        let m = m_stab
            + lift * dist_cg_to_lift * alpha.cos()
            + drag * dist_cg_to_lift * alpha.sin()
            + engine_moment_pitch
            + gear_moment_pitch;

        let i_xx = kilograms_meter2!(1f64);
        let i_zz = kilograms_meter2!(1f64);
        let i_xz = kilograms_meter2!(1f64);
        let i_yy = kilograms_meter2!(1f64);
        // Compute body frame accelerations
        // (kg*m^2 / s^2) / kg*m^2 => rad / s^2
        let q_dot: AngularAcceleration<Radians, Seconds> = ((m.as_dyn() * radians!(1f64).as_dyn()
            + (i_zz - i_xx).as_dyn() * (r * p).as_dyn()
            + i_xz.as_dyn() * (r * r - p * p).as_dyn())
            / i_yy.as_dyn())
        .into();
        // rad/s
        q += q_dot * seconds!(dt.as_secs_f64());
        println!("RAD/S: {}", q);

        // a=F/m
        // kg*m/s^2/m +
        u_dot = (thrust - drag) / weight_lb.mass::<Kilograms>() - w * q + v * r;
        u += u_dot * seconds!(dt.as_secs_f64());

        *motion.forward_acceleration_mut() = u_dot;
        *motion.forward_velocity_mut() = u;
        *motion.pitch_velocity_mut() = q;

        // Update the frame facing with our angular velocities.
        let rot = UnitQuaternion::from_euler_angles(p.f64(), q.f64(), r.f64());
        // *frame.facing_mut() = rot * frame.facing();

        // rotate motion into world space frame and apply to position.
        let velocity_m_s = (frame.facing() * motion.velocity_m_s().map(|v| v.f64()))
            .map(|v| meters_per_second!(v));
        let world_pos = frame.position_pt3() - velocity_m_s.map(|v| v * seconds!(dt.as_secs_f64()));
        frame.set_position(world_pos);

        // let delta_quat = UnitQuaternion::from

        self.weight_lbs = weight_lb;
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
            &mut Elevator,
            &mut DrawState,
            &mut BodyMotion,
            &mut WorldSpaceFrame,
            &mut FlightDynamics,
            &PlayerMarker,
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
            mut elevator,
            mut draw_state,
            mut motion,
            mut frame,
            mut dynamics,
            _,
        ) in query.iter_mut()
        {
            dynamics.simulate(
                &timestep,
                &xt,
                &airbrake,
                &flaps,
                &hook,
                &mut bay,
                &mut gear,
                &mut throttle,
                &mut elevator,
                &mut draw_state,
                &mut motion,
                &mut frame,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {}
}
