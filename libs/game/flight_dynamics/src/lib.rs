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
mod inertia;

pub use crate::{
    control::{
        ailerons::Ailerons, airbrake::Airbrake, bay::Bay, elevator::Elevator, flaps::Flaps,
        gear::Gear, hook::Hook, rudder::Rudder,
    },
    inertia::Inertia,
};
use absolute_unit::{
    degrees, kilograms_meter2, knots, meters, meters2, meters_per_second, meters_per_second2,
    newton_meters, newtons, pounds_weight, radians, radians_per_second, radians_per_second2,
    scalar, seconds, Acceleration, Angle, AngularAcceleration, AngularVelocity, Force, Kilograms,
    Meters, Newtons, PoundsMass, PoundsWeight, Radians, RotationalInertia, Scalar, Seconds, Torque,
    Velocity, Weight,
};
use animate::TimeStep;
use anyhow::Result;
use approx::relative_eq;
use bevy_ecs::prelude::*;
use geodesy::{GeoCenter, GeoSurface, Graticule};
use marker::EntityMarkers;
use measure::{BodyMotion, WorldSpaceFrame};
use nalgebra::{Point3, UnitQuaternion, Vector3};
use nitrous::{inject_nitrous_component, method, HeapMut, NitrousComponent};
use physical_constants::{StandardAtmosphere, STANDARD_GRAVITY};
use pt::{GloadExtrema, PlaneType};
use runtime::{Extension, Runtime};
use shape::{DrawState, ShapeBuffer, ShapeId};
use vehicle_state::VehicleState;
use xt::TypeRef;

#[derive(Clone, Debug, Eq, PartialEq, Hash, SystemLabel)]
pub enum FlightStep {
    UpdateState,
    Simulate,
}

#[derive(Debug, Component, NitrousComponent)]
#[Name = "dynamics"]
pub struct FlightDynamics {
    // self pointer for updating markers
    id: Entity,

    // Current envelope
    max_g_load: GloadExtrema,

    // Intermediates
    alpha: Angle<Radians>,
    beta: Angle<Radians>,

    // //////////// Inertia tensor //////////// //
    // inertia_xx: RotationalInertia<Kilograms, Meters>,
    // inertia_yy: RotationalInertia<Kilograms, Meters>,
    // inertia_zz: RotationalInertia<Kilograms, Meters>,
    // inertia_xz: RotationalInertia<Kilograms, Meters>,

    // //////////// Pitch coefficients //////////// //

    // The contribution of the distribution of mass of the aircraft to the pitching moment.
    // Coef_m0 changes with the cener of gravity (cg) position and the effect of the flaps
    // and undercarrage are often included in this term. The cg position depends on the
    // mass and distribution of the fuel, crew, passengers, and luggage and is usually
    // quoted as a percentage of the chord (c_bar).
    // In FA, planes are always assumed to be well balanced. Or that the implicit
    // compensations are factored into the drag coefficients already.
    coef_m0: Angle<Radians>,

    // The major contribution to pitching stability. It determines the natural frequency
    // of the short period plugoid. It also determines the aircraft response to pilot
    // inputs and gusts. The value should be sufficiently large to give an acceptable
    // response to pilot inputs.
    coef_malpha: Scalar,

    // The term is often referred to as 'elevator effectiveness' or 'elevator power'.
    // It is mainly influenced by the area of the elevator surface and the angular range
    // of movement of the manoeuver or in response to a disturbance.
    coef_mde: Scalar,

    // As the aircraft pitches, resistance to the angular velocity is provided by this term.
    // coef_mq is commonly referred to a pitch damping as it dampens the short period
    // plugoid. It provides a major contribution to longitudinal stability and aircraft
    // handling qualities.
    coef_mq: Scalar,

    // This derivative also contributes to damping of the short period plugoid.
    coef_malphadot: Scalar,
}

impl Extension for FlightDynamics {
    fn init(runtime: &mut Runtime) -> Result<()> {
        runtime.add_sim_system(Self::sys_update_state.label(FlightStep::UpdateState));
        runtime.add_sim_system(
            Self::sys_simulate
                .label(FlightStep::Simulate)
                .after(FlightStep::UpdateState),
        );

        Ok(())
    }
}

#[inject_nitrous_component]
impl FlightDynamics {
    pub fn new(id: Entity) -> Self {
        Self {
            id,
            max_g_load: GloadExtrema::Stall(0.),
            alpha: radians!(0.),
            beta: radians!(0.),
            // Pitch coefficients
            coef_m0: radians!(0_f64),
            coef_malpha: scalar!(0.5_f64),
            coef_mde: scalar!(0.1_f64),
            coef_mq: scalar!(-5000_f64),
            coef_malphadot: scalar!(0.001_f64),
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
        let ailerons = Ailerons::default();
        let elevator = Elevator::default();
        let rudder = Rudder::default();
        let inertia = Inertia::from_extent(
            pt,
            heap.resource::<ShapeBuffer>()
                .metadata(*heap.get::<ShapeId>(id))
                .read()
                .extent(),
        );
        heap.named_entity_mut(id)
            .insert_named(airbrake)?
            .insert_named(flaps)?
            .insert_named(hook)?
            .insert_named(gear)?
            .insert_named(bay)?
            .insert_named(ailerons)?
            .insert_named(elevator)?
            .insert_named(rudder)?
            .insert_named(inertia)?
            .insert_named(FlightDynamics::new(id))?;
        Ok(())
    }

    // Basis Vectors in Body
    #[method]
    pub fn show_body_coordinates(&mut self, mut heap: HeapMut) -> Result<()> {
        if heap.maybe_get::<EntityMarkers>(self.id).is_none() {
            heap.entity_mut(self.id).insert(EntityMarkers::default());
        }
        let mut markers = heap.get_mut::<EntityMarkers>(self.id);
        markers.add_point(
            "center_of_gravity",
            Point3::origin(),
            meters!(0.5_f64),
            "#F0F".parse()?,
        );
        markers.add_arrow(
            "positive_x",
            Point3::origin(),
            Vector3::new(meters!(10f64), meters!(0f64), meters!(0f64)),
            meters!(0.25_f64),
            "#F00".parse()?,
        );
        markers.add_arrow(
            "positive_y",
            Point3::origin(),
            Vector3::new(meters!(0f64), meters!(10f64), meters!(0f64)),
            meters!(0.25_f64),
            "#0F0".parse()?,
        );
        markers.add_arrow(
            "positive_z",
            Point3::origin(),
            Vector3::new(meters!(0f64), meters!(0f64), meters!(10f64)),
            meters!(0.25_f64),
            "#00F".parse()?,
        );
        Ok(())
    }

    #[method]
    pub fn hide_body_coordinates(&mut self, mut heap: HeapMut) -> Result<()> {
        if let Some(mut markers) = heap.maybe_get_mut::<EntityMarkers>(self.id) {
            markers.remove_arrow("positive_x");
            markers.remove_arrow("positive_y");
            markers.remove_arrow("positive_z");
        }
        Ok(())
    }

    #[method]
    pub fn show_gravity_vectors(&mut self, mut heap: HeapMut) -> Result<()> {
        if heap.maybe_get::<EntityMarkers>(self.id).is_none() {
            heap.entity_mut(self.id).insert(EntityMarkers::default());
        }
        let mut markers = heap.get_mut::<EntityMarkers>(self.id);
        markers.add_arrow(
            "gravity",
            Point3::origin(),
            Vector3::new(meters!(0f64), meters!(0f64), meters!(0f64)),
            meters!(0.25_f64),
            "#F00".parse()?,
        );
        markers.add_arrow(
            "gravity_x",
            Point3::origin(),
            Vector3::new(meters!(0f64), meters!(0f64), meters!(0f64)),
            meters!(0.25_f64),
            "#F77".parse()?,
        );
        markers.add_arrow(
            "gravity_y",
            Point3::origin(),
            Vector3::new(meters!(0f64), meters!(0f64), meters!(0f64)),
            meters!(0.25_f64),
            "#F77".parse()?,
        );
        markers.add_arrow(
            "gravity_z",
            Point3::new(meters!(0f64), meters!(0f64), meters!(0f64)),
            Vector3::new(meters!(0f64), meters!(0f64), meters!(0f64)),
            meters!(0.25_f64),
            "#F77".parse()?,
        );
        Ok(())
    }

    #[method]
    pub fn hide_gravity_vectors(&mut self, mut heap: HeapMut) -> Result<()> {
        if let Some(mut markers) = heap.maybe_get_mut::<EntityMarkers>(self.id) {
            markers.remove_arrow("gravity");
            markers.remove_arrow("gravity_x");
            markers.remove_arrow("gravity_y");
            markers.remove_arrow("gravity_z");
        }
        Ok(())
    }

    #[method]
    pub fn show_lift_vectors(&mut self, mut heap: HeapMut) -> Result<()> {
        if heap.maybe_get::<EntityMarkers>(self.id).is_none() {
            heap.entity_mut(self.id).insert(EntityMarkers::default());
        }
        let mut markers = heap.get_mut::<EntityMarkers>(self.id);
        markers.add_arrow(
            "lift",
            Point3::origin(),
            Vector3::new(meters!(0f64), meters!(0f64), meters!(0f64)),
            meters!(0.25_f64),
            "#00F".parse()?,
        );
        markers.add_arrow(
            "lift_x",
            Point3::origin(),
            Vector3::new(meters!(0f64), meters!(0f64), meters!(0f64)),
            meters!(0.25_f64),
            "#77F".parse()?,
        );
        markers.add_arrow(
            "lift_z",
            Point3::new(meters!(0f64), meters!(0f64), meters!(0f64)),
            Vector3::new(meters!(0f64), meters!(0f64), meters!(0f64)),
            meters!(0.25_f64),
            "#77F".parse()?,
        );
        Ok(())
    }

    #[method]
    pub fn hide_lift_vectors(&mut self, mut heap: HeapMut) -> Result<()> {
        if let Some(mut markers) = heap.maybe_get_mut::<EntityMarkers>(self.id) {
            markers.remove_arrow("lift");
            markers.remove_arrow("lift_x");
            markers.remove_arrow("lift_y");
            markers.remove_arrow("lift_z");
        }
        Ok(())
    }

    #[method]
    pub fn show_inertia_model(&mut self, mut heap: HeapMut) -> Result<()> {
        if heap.maybe_get::<EntityMarkers>(self.id).is_none() {
            heap.entity_mut(self.id).insert(EntityMarkers::default());
        }
        let (fuselage_front, fuselage_back, wing) = {
            let inertia = heap.get::<Inertia>(self.id);
            (
                inertia.fuselage_front().to_owned(),
                inertia.fuselage_back().to_owned(),
                inertia.wing().to_owned(),
            )
        };
        let mut markers = heap.get_mut::<EntityMarkers>(self.id);
        markers.add_cylinder_direct("inertia_fuselage_front", fuselage_front, "#0D05".parse()?);
        markers.add_cylinder_direct("inertia_fuselage_back", fuselage_back, "#0E05".parse()?);
        markers.add_box_direct("inertia_wing", wing, "#0D05".parse()?);
        Ok(())
    }

    #[method]
    pub fn hide_inertia_model(&mut self, mut heap: HeapMut) -> Result<()> {
        if let Some(mut markers) = heap.maybe_get_mut::<EntityMarkers>(self.id) {
            markers.remove_cylinder("inertia_fuselage_front");
            markers.remove_cylinder("inertia_fuselage_back");
            markers.remove_box("inertia_wing");
        }
        Ok(())
    }

    #[method]
    pub fn show_force_vectors(&mut self, mut heap: HeapMut) -> Result<()> {
        if heap.maybe_get::<EntityMarkers>(self.id).is_none() {
            heap.entity_mut(self.id).insert(EntityMarkers::default());
        }
        let mut markers = heap.get_mut::<EntityMarkers>(self.id);
        markers.add_arrow(
            "force_x",
            Point3::origin(),
            Vector3::new(meters!(0f64), meters!(0f64), meters!(0f64)),
            meters!(0.25_f64),
            "#7F7".parse()?,
        );
        markers.add_arrow(
            "force_y",
            Point3::origin(),
            Vector3::new(meters!(0f64), meters!(0f64), meters!(0f64)),
            meters!(0.25_f64),
            "#00F".parse()?,
        );
        markers.add_arrow(
            "force_z",
            Point3::origin(),
            Vector3::new(meters!(0f64), meters!(0f64), meters!(0f64)),
            meters!(0.25_f64),
            "#F00".parse()?,
        );
        Ok(())
    }

    #[method]
    pub fn hide_force_vectors(&mut self, mut heap: HeapMut) -> Result<()> {
        if let Some(mut markers) = heap.maybe_get_mut::<EntityMarkers>(self.id) {
            markers.remove_arrow("force_x");
            markers.remove_arrow("force_y");
            markers.remove_arrow("force_z");
        }
        Ok(())
    }

    #[method]
    pub fn show_velocity_vector(&mut self, mut heap: HeapMut) -> Result<()> {
        if heap.maybe_get::<EntityMarkers>(self.id).is_none() {
            heap.entity_mut(self.id).insert(EntityMarkers::default());
        }
        let mut markers = heap.get_mut::<EntityMarkers>(self.id);
        markers.add_arrow(
            "velocity",
            Point3::origin(),
            Vector3::new(meters!(0f64), meters!(0f64), meters!(0f64)),
            meters!(0.25_f64),
            "#3AF".parse()?,
        );
        Ok(())
    }

    #[method]
    pub fn hide_velocity_vector(&mut self, mut heap: HeapMut) -> Result<()> {
        if let Some(mut markers) = heap.maybe_get_mut::<EntityMarkers>(self.id) {
            markers.remove_arrow("velocity");
        }
        Ok(())
    }

    pub fn alpha(&self) -> Angle<Radians> {
        self.alpha
    }

    pub fn beta(&self) -> Angle<Radians> {
        self.beta
    }

    pub fn max_g_load(&self) -> GloadExtrema {
        self.max_g_load
    }

    #[method]
    pub fn coef_m0(&self) -> f64 {
        self.coef_m0.f64()
    }

    #[method]
    pub fn set_coef_m0(&mut self, v: f64) {
        self.coef_m0 = radians!(v);
    }

    #[method]
    pub fn coef_malpha(&self) -> f64 {
        self.coef_malpha.f64()
    }

    #[method]
    pub fn set_coef_malpha(&mut self, v: f64) {
        self.coef_malpha = scalar!(v);
    }

    #[method]
    pub fn coef_malphadot(&self) -> f64 {
        self.coef_malphadot.f64()
    }

    #[method]
    pub fn set_coef_malphadot(&mut self, v: f64) {
        self.coef_malphadot = scalar!(v);
    }

    #[method]
    pub fn coef_mde(&self) -> f64 {
        self.coef_mde.f64()
    }

    #[method]
    pub fn set_coef_mde(&mut self, v: f64) {
        self.coef_mde = scalar!(v);
    }

    #[method]
    pub fn coef_mq(&self) -> f64 {
        self.coef_mq.f64()
    }

    #[method]
    pub fn set_coef_mq(&mut self, v: f64) {
        self.coef_mq = scalar!(v);
    }

    fn update_state(
        &mut self,
        timestep: &TimeStep,

        airbrake: &Airbrake,
        flaps: &Flaps,
        hook: &Hook,
        bay: &mut Bay,
        gear: &mut Gear,

        ailerons: &mut Ailerons,
        elevator: &mut Elevator,
        rudder: &mut Rudder,

        xt: &TypeRef,
        draw_state: &mut DrawState,
    ) {
        let dt = timestep.step();
        let pt = xt.pt().expect("PT");

        airbrake.sys_tick(draw_state);
        flaps.sys_tick(draw_state);
        hook.sys_tick(draw_state);
        bay.sys_tick(dt, draw_state);
        gear.sys_tick(dt, draw_state);
        ailerons.sys_tick(dt, draw_state);
        elevator.sys_tick(dt, draw_state);
        rudder.sys_tick(dt, draw_state);
    }

    // Algorithm taken from David Allerton's Principles of Flight Simulation.
    fn simulate(
        &mut self,
        timestep: &TimeStep,
        xt: &TypeRef,
        airbrake: &Airbrake,
        flaps: &Flaps,
        hook: &Hook,
        bay: &Bay,
        gear: &Gear,
        vehicle: &VehicleState,
        ailerons: &Ailerons,
        elevator: &Elevator,
        rudder: &Rudder,
        inertia: &mut Inertia,
        motion: &mut BodyMotion,
        frame: &mut WorldSpaceFrame,
        mut markers: Option<Mut<EntityMarkers>>,
    ) {
        let dt = timestep.step();
        let pt = xt.pt().expect("PT");

        // Allerton defines the SDoF axes as:
        // fwd   axis: X, u, L, p
        // right axis: Y, v, M, q
        // down  axis: Z, w, N, r

        let grat = frame.position_graticule();

        let altitude = grat.distance;
        let atmosphere = StandardAtmosphere::at_altitude(altitude);
        let air_density = atmosphere.density::<Kilograms, Meters>();
        assert!(air_density.is_finite(), "NaN air density at {altitude}");

        let mut u = motion.vehicle_forward_velocity();
        let mut v = motion.vehicle_sideways_velocity();
        let mut w = motion.vehicle_vertical_velocity();
        let mut q = motion.vehicle_pitch_velocity();
        let p = motion.vehicle_roll_velocity();
        let r = motion.vehicle_yaw_velocity();
        let wing_area_s = meters2!(1_f64); // s
        let uw_mag = (u * u + w * w).sqrt(); // m/s
        let velocity_cg_2 = u * u + v * v + w * w; // m^2/s^2
        let velocity_cg = velocity_cg_2.sqrt();
        let mut u_dot = motion.vehicle_forward_acceleration(); // u*
        let mut v_dot = motion.vehicle_sideways_acceleration(); // v*
        let mut w_dot = motion.vehicle_vertical_acceleration(); // w*
        let max_aoa = radians!(degrees!(pt.gpull_aoa));
        let alpha = radians!(w.f64().atan2(u.f64()));
        let beta = radians!(v.f64().atan2(uw_mag.f64()));
        if let Some(_) = markers.as_mut() {
            if alpha > max_aoa {
                println!(
                    "OOB alpha > max_aoa: {} > {}",
                    degrees!(alpha),
                    degrees!(max_aoa)
                );
            }
        }

        // Alpha_dot and beta_dot are trig approximations, so the units intentionally don't
        // make much of any sense.
        let alpha_dot = radians_per_second!(if relative_eq!(uw_mag.f64(), 0f64) {
            0_f64
        } else {
            (u.f64() * w_dot.f64() - w.f64() * u_dot.f64()) / uw_mag.f64()
        });
        // v* * sqrt(u^2 + w^2) - v * (u * u* + w * w*)
        // --------------------------------------------
        // sqrt(u^2 + w^2) * (u^2 + v^2 + w^2)
        let beta_dot_numerator_1 = v_dot.f64() * uw_mag.f64();
        let beta_dot_numerator_2 = v.f64() * (u.f64() * u_dot.f64() + w.f64() * w_dot.f64());
        let beta_dot_numerator = beta_dot_numerator_1 - beta_dot_numerator_2;
        let beta_dot_denominator = uw_mag.f64() * velocity_cg_2.f64();
        let beta_dot = radians_per_second!(if relative_eq!(beta_dot_denominator, 0f64) {
            0f64
        } else {
            beta_dot_numerator / beta_dot_denominator
        });

        // //////////////////// GRAVITY ////////////////////
        let gravity_wf = frame.facing().inverse()
            * (-frame.position().vec64().normalize() * STANDARD_GRAVITY.f64());
        if let Some(em) = markers.as_mut() {
            let na = meters!(0_f64);
            em.update_arrow_vector("gravity", gravity_wf.map(|tmp| meters!(tmp)));
            em.update_arrow_vector("gravity_x", Vector3::new(na, na, meters!(gravity_wf.z)));
            em.update_arrow_vector("gravity_y", Vector3::new(meters!(gravity_wf.x), na, na));
            em.update_arrow_vector("gravity_z", Vector3::new(na, meters!(gravity_wf.y), na));
        }
        // Translate from nitrous world/body frame to Allerton frame
        let gravity_x = meters_per_second2!(-gravity_wf.z);
        let gravity_y = meters_per_second2!(gravity_wf.x);
        let gravity_z = meters_per_second2!(-gravity_wf.y);

        // //////////////////// THRUST ////////////////////
        let engine_thrust_x = vehicle.power_plant().forward_thrust(&atmosphere, motion);
        let engine_thrust_y = newtons!(0_f64);
        let engine_thrust_z = newtons!(0_f64);
        let mass_kg = vehicle.current_mass();
        let engine_moment_pitch = newton_meters!(0_f64);
        let engine_moment_yaw = newton_meters!(0_f64);
        let engine_moment_roll = newton_meters!(0_f64);

        // //////////////////// LIFT ////////////////////
        // Coefficients of lift are linear from C{L0} to C{Lmax}. FA specifies the max
        // aoa, which is the stall aoa of the wing, corresponding to C{Lmax}, presumably
        // in the max g envelope somewhere. We can get the max current lift from the
        // current g envelope (e.g. the maximum that it can lift is the plane time N G's).
        // Similarly, planes fly exactly in the direction they are pointed in FA, so
        // C{L0} must be trimmed such that if alpha is 0, the lift will exactly
        // counteract gravity, at least above 1G.
        // FIXME: for velocity less than 0... lift needs to not be totally normal.
        // FIXME: account for `flaps_lift`
        self.max_g_load = pt.envelopes.find_g_load_maxima(velocity_cg, altitude);
        let stall_speed = if let Some(stall_speed) = pt.envelopes.find_min_lift_speed_at(altitude) {
            // TODO: helicopters have an envelope that stalls at 0; bump this so that we
            //       at least don't get a NaN in the works for the moment.
            stall_speed.max(meters_per_second!(1_f64))
        } else {
            meters_per_second!(pt.max_speed_36a)
        };
        debug_assert!(
            stall_speed > meters_per_second!(0_f64),
            "stall speed of zero"
        );
        // We have to simulate the coefficient, so build a factor that will
        // act as trim, down to the stall speed, then use the lift at stall
        // as our baseline when in a stalled state.
        let coef_divisor: Force<Newtons> = (scalar!(0.5).as_dyn()
            * air_density.as_dyn()
            * (velocity_cg_2.max(stall_speed * stall_speed)).as_dyn()
            * wing_area_s.as_dyn())
        .into();
        debug_assert!(coef_divisor > newtons!(0_f64), "0 lift coef divisor");
        debug_assert!(coef_divisor.is_finite(), "NaN lift coef divisor");
        let coef_lift_0 = (mass_kg * gravity_z).f64() / coef_divisor.f64();
        let coef_lift_max = (pounds_weight!(pt.max_takeoff_weight).mass::<Kilograms>()
            * scalar!(pt.envelopes.max_g_load())
            * *STANDARD_GRAVITY)
            .f64()
            / coef_divisor.f64();
        debug_assert!(coef_lift_0.is_finite(), "NaN lift0 coef");
        debug_assert!(coef_lift_max.is_finite(), "NaN liftMax coef");
        let coef_lift = coef_lift_0 + (alpha.f64() / max_aoa.f64()) * (coef_lift_max - coef_lift_0);
        debug_assert!(coef_lift.is_finite(), "NaN lift coef");

        let lift: Force<Newtons> = (scalar!(0.5).as_dyn()
            * air_density.as_dyn()
            * velocity_cg_2.as_dyn()
            * wing_area_s.as_dyn()
            * scalar!(coef_lift).as_dyn())
        .into();
        debug_assert!(lift.is_finite(), "NaN lift");
        // println!(
        //     "pref: @{:0.1} => {:0.1} * {velocity_cg_2:0.1} {lift_prefix}",
        //     altitude, air_density
        // );
        if let Some(em) = markers.as_mut() {
            let na = meters!(0_f64);
            let lift_body_y = meters!((lift * alpha.cos() / scalar!(500.)).f64());
            let lift_body_z = meters!((lift * alpha.sin() / -scalar!(500.)).f64());
            em.update_arrow_vector("lift", Vector3::new(na, lift_body_y, lift_body_z));
            em.update_arrow_vector("lift_x", Vector3::new(na, na, lift_body_z));
            em.update_arrow_vector("lift_z", Vector3::new(na, lift_body_y, na));
        }
        // assert!(lift < scalar!(15_f64) * mass_kg * *STANDARD_GRAVITY);
        // if velocity_cg > meters_per_second!(340) {
        //     println!("speed: {velocity_cg:0.1}, lift: {lift:0.1}, vel: {u:0.1}, {v:0.1}, {w:0.1}, acc: {u_dot:0.1}, {v_dot:0.1}, {w_dot:0.1}");
        // }

        // //////////////////// DRAG ////////////////////
        // A is usually assumed to be 1, and FA doesn't track it anyway.
        // We can assume that the units probably match thrust units, which are ft*lb/s^2
        // FIXME: add in _gpull_drag; e.g. induced drag coefficient
        // FIXME: add in control surface drag; e.g. rudder_drag
        let mut coef_drag = scalar!(
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
        // If the plane is moving backwards, drag should oppose the direction of movement.
        if u < meters_per_second!(0_f64) {
            coef_drag = -coef_drag;
        }
        let drag: Force<Newtons> = (scalar!(0.5).as_dyn()
            * coef_drag.as_dyn()
            * air_density.as_dyn()
            * (u * u).as_dyn()
            * wing_area_s.as_dyn())
        .into();
        debug_assert!(drag.is_finite(), "NaN drag");

        // //////////////////// SIDE FORCE ////////////////////
        let coef_ydr = scalar!(0.0001_f64);
        let coef_ybeta = scalar!(0.1_f64);
        let rudder_position = radians!(0_f64);
        // 1/2 p V^2 s (C_ydr * dr + Cyb * B);
        let side_force: Force<Newtons> = (scalar!(0.5).as_dyn()
            * air_density.as_dyn()
            * velocity_cg.as_dyn()
            * velocity_cg.as_dyn()
            * wing_area_s.as_dyn() // s
            * (coef_ydr * rudder_position + coef_ybeta * beta).as_dyn())
        .into();
        debug_assert!(side_force.is_finite(), "NaN sideforce");

        // //////////////////// GEAR FORCE ////////////////////
        // body frame
        let gear_x = newtons!(0_f64);
        let gear_y = newtons!(0_f64);
        let gear_z = newtons!(0_f64);

        // //////////////////// BODY FRAME FORCES ////////////////////
        let force_x = alpha.sin() * lift - alpha.cos() * drag
            + mass_kg * gravity_x
            + engine_thrust_x
            + gear_x;
        let force_y = side_force + mass_kg * gravity_y + engine_thrust_y + gear_y;
        let force_z = -alpha.cos() * lift - alpha.sin() * drag
            + mass_kg * gravity_z
            + engine_thrust_z
            + gear_z;
        debug_assert!(force_x.is_finite(), "NaN force_x");
        debug_assert!(force_y.is_finite(), "NaN force_y");
        debug_assert!(force_z.is_finite(), "NaN force_z");
        if let Some(em) = markers.as_mut() {
            let na = meters!(0_f64);
            em.update_arrow_vector(
                "force_x",
                Vector3::new(na, na, meters!(-force_x.f64() / 100.)),
            );
            em.update_arrow_vector(
                "force_y",
                Vector3::new(meters!(force_y.f64() / 100.), na, na),
            );
            em.update_arrow_vector(
                "force_z",
                Vector3::new(na, meters!(-force_z.f64() / 100.), na),
            );
        }

        // //////////////////// BODY FRAME ACCELERATIONS ////////////////////
        u_dot = force_x / mass_kg - w * q + v * r;
        // v_dot = force_y / mass_kg - u * r + w * p;
        w_dot = force_z / mass_kg - v * p + u * q;
        debug_assert!(u_dot.is_finite(), "NaN u_dot");
        debug_assert!(v_dot.is_finite(), "NaN v_dot");
        debug_assert!(w_dot.is_finite(), "NaN w_dot");

        // //////////////////// PITCH ////////////////////
        let dist_cg_to_lift = meters!(0f64);
        let gear_moment_pitch = newton_meters!(0f64);

        let coef_1 =
            self.coef_m0 + self.coef_malpha * alpha + self.coef_mde * radians!(elevator.position());
        let coef_2a = self.coef_mq * q;
        let coef_2b = self.coef_malphadot.as_dyn() * alpha_dot.as_dyn();
        let coef_2 = coef_2a.as_dyn() + coef_2b;

        let m_stab: Torque<Newtons, Meters> = ((scalar!(0.5).as_dyn()
            * air_density.as_dyn()
            * velocity_cg.as_dyn()
            * velocity_cg.as_dyn()
            * meters2!(1f64).as_dyn() // s
            * meters!(1f64).as_dyn() // c_bar
            * coef_1.as_dyn())
            + (scalar!(0.25).as_dyn()
                * air_density.as_dyn()
                * velocity_cg.as_dyn()
                * meters2!(1f64).as_dyn() // s
                * meters!(1f64).as_dyn() // c_bar
                * meters!(1f64).as_dyn() // c_bar
                * coef_2.as_dyn()))
        .into();
        debug_assert!(m_stab.is_finite(), "m_stab is NaN");

        let m = m_stab
            // Induced from plane balance
            + lift * dist_cg_to_lift * alpha.cos()
            + drag * dist_cg_to_lift * alpha.sin()
            // thrust vectoring
            + engine_moment_pitch
            // ground interactions
            + gear_moment_pitch;
        debug_assert!(m.is_finite(), "m is NaN");

        let i_xx = kilograms_meter2!(100f64);
        let i_zz = kilograms_meter2!(100f64);
        let i_xz = kilograms_meter2!(100f64);
        let i_yy = inertia.i_xx(); // I totally got the axis wrong here
                                   // Compute body frame accelerations
                                   // (kg*m^2 / s^2) / kg*m^2 => rad / s^2
        assert_eq!(r, radians_per_second!(0f64));
        assert_eq!(p, radians_per_second!(0f64));
        let q_dot: AngularAcceleration<Radians, Seconds> = ((m.as_dyn() * radians!(1f64).as_dyn()
            + (i_zz - i_xx).as_dyn() * (r * p).as_dyn() // 0
            + i_xz.as_dyn() * (r * r - p * p).as_dyn()) // 0
            / i_yy.as_dyn())
        .into();
        debug_assert!(q_dot.is_finite(), "q_dot is NaN");
        // rad/s
        q += q_dot * seconds!(dt.as_secs_f64());
        debug_assert!(q.is_finite(), "q is NaN");

        // a=F/m
        // kg*m/s^2/m +
        u += u_dot * seconds!(dt.as_secs_f64());
        // v += v_dot * seconds!(dt.as_secs_f64());
        w += w_dot * seconds!(dt.as_secs_f64());

        motion.set_vehicle_forward_acceleration(u_dot);
        motion.set_vehicle_forward_velocity(u);

        // motion.set_vehicle_sideways_acceleration(v_dot);
        // motion.set_vehicle_sideways_velocity(v);

        motion.set_vehicle_vertical_acceleration(w_dot);
        motion.set_vehicle_vertical_velocity(w);

        motion.set_vehicle_pitch_velocity(q);

        // Update the frame facing with our angular velocities.
        let rot = UnitQuaternion::from_euler_angles(q.f64(), r.f64(), p.f64());
        *frame.facing_mut() = frame.facing() * rot;

        // rotate motion into world space frame and apply to position.
        let velocity_m_s =
            (frame.facing() * motion.velocity().map(|v| v.f64())).map(|v| meters_per_second!(v));
        let world_pos = frame.position_pt3() + velocity_m_s.map(|v| v * seconds!(dt.as_secs_f64()));
        debug_assert!(world_pos.x.is_finite(), "world x is NaN");
        debug_assert!(world_pos.y.is_finite(), "world y is NaN");
        debug_assert!(world_pos.z.is_finite(), "world z is NaN");
        frame.set_position(world_pos);
        if frame.position_graticule().distance < meters!(0_f64) {
            let mut grat = frame.position_graticule();
            grat.distance = meters!(0_f64);
            frame.set_position_graticule(grat);
            motion.freeze();
        }
        if frame.position_graticule().distance > meters!(100_000_f64) {
            let mut grat = frame.position_graticule();
            grat.distance = meters!(100_000_f64);
            frame.set_position_graticule(grat);
            motion.freeze();
        }

        if let Some(em) = markers.as_mut() {
            em.update_arrow_vector(
                "velocity",
                motion.velocity().map(|v| meters!(v.f64() / 50.)),
            );
        }

        // let delta_quat = UnitQuaternion::from

        self.alpha = alpha;
        self.beta = beta;
    }

    fn sys_update_state(
        timestep: Res<TimeStep>,
        mut query: Query<(
            &mut FlightDynamics,
            (&Airbrake, &Flaps, &Hook, &mut Bay, &mut Gear),
            (&mut Ailerons, &mut Elevator, &mut Rudder),
            (&TypeRef, &mut DrawState),
        )>,
    ) {
        for (
            mut dynamics,
            (airbrake, flaps, hook, mut bay, mut gear),
            (mut ailerons, mut elevator, mut rudder),
            (xt, mut draw_state),
        ) in query.iter_mut()
        {
            dynamics.update_state(
                &timestep,
                &airbrake,
                &flaps,
                &hook,
                &mut bay,
                &mut gear,
                &mut ailerons,
                &mut elevator,
                &mut rudder,
                &xt,
                &mut draw_state,
            );
        }
    }

    fn sys_simulate(
        timestep: Res<TimeStep>,
        mut query: Query<(
            &mut FlightDynamics,
            &mut Inertia,
            (&Airbrake, &Flaps, &Hook, &Bay, &Gear, &VehicleState),
            (&Ailerons, &Elevator, &Rudder),
            (&TypeRef, &mut BodyMotion, &mut WorldSpaceFrame),
            Option<&mut EntityMarkers>,
        )>,
    ) {
        for (
            mut dynamics,
            mut inertia,
            (airbrake, flaps, hook, bay, gear, vehicle),
            (ailerons, elevator, rudder),
            (xt, mut motion, mut frame),
            markers,
        ) in query.iter_mut()
        {
            dynamics.simulate(
                &timestep,
                &xt,
                &airbrake,
                &flaps,
                &hook,
                &bay,
                &gear,
                &vehicle,
                &ailerons,
                &elevator,
                &rudder,
                &mut inertia,
                &mut motion,
                &mut frame,
                markers,
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
