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
use absolute_unit::prelude::*;
use animate::TimeStep;
use anyhow::Result;
use approx::relative_eq;
use bevy_ecs::prelude::*;
use marker::EntityMarkers;
use measure::{BodyMotion, WorldSpaceFrame};
use nalgebra::{Point3, UnitQuaternion, Vector3};
use nitrous::{inject_nitrous_component, method, HeapMut, NitrousComponent};
use physical_constants::{StandardAtmosphere, STANDARD_GRAVITY};
use pt::{GloadExtrema, PlaneType};
use runtime::{Extension, Runtime};
use std::time::Duration;
use vehicle::{
    AirbrakeEffector, Airframe, BayEffector, FlapsEffector, FuelSystem, GearEffector, HookEffector,
    PitchInceptor, PowerSystem, RollInceptor, YawInceptor,
};
use xt::TypeRef;

#[derive(Clone, Debug, Eq, PartialEq, Hash, SystemLabel)]
pub enum ClassicFlightModelStep {
    Simulate,
}

/// The classic flight model tries to mimic FA as closely as possible.
/// This means, no inertial model for the plane, no atmospheric model
/// impacting handling characteristics, linear engine thrust handling
/// with no altitude falloff, strict bounding to the 0G envelope, and
/// all the rest of the annoying bits.
#[derive(Debug, Component, NitrousComponent)]
#[Name = "flight"]
pub struct ClassicFlightModel {
    // self pointer for updating markers
    id: Entity,

    // Current envelope and position within it
    max_g_load: GloadExtrema,
    g_load: f64,

    coef_of_drag: f64,
    force_of_drag: f64,
}

impl Extension for ClassicFlightModel {
    fn init(runtime: &mut Runtime) -> Result<()> {
        runtime.add_sim_system(Self::sys_simulate.label(ClassicFlightModelStep::Simulate));
        Ok(())
    }
}

#[inject_nitrous_component]
impl ClassicFlightModel {
    pub fn new(id: Entity) -> Self {
        Self {
            id,
            max_g_load: GloadExtrema::Stall(0.),
            g_load: 1f64,
            coef_of_drag: 0.,
            force_of_drag: 0.,
        }
    }

    pub fn max_g_load(&self) -> &GloadExtrema {
        &self.max_g_load
    }

    pub fn coef_of_drag(&self) -> f64 {
        self.coef_of_drag
    }

    pub fn force_of_drag(&self) -> f64 {
        self.force_of_drag
    }

    #[method]
    pub fn max_g(&self) -> f64 {
        self.max_g_load.max_g_load()
    }

    #[method]
    pub fn show_velocity_vector(&mut self, mut heap: HeapMut) -> Result<()> {
        if heap.maybe_get::<EntityMarkers>(self.id).is_none() {
            heap.entity_mut(self.id).insert(EntityMarkers::default());
        }
        let mut markers = heap.get_mut::<EntityMarkers>(self.id);
        markers.add_motion_arrow(
            "velocity",
            Point3::origin(),
            Vector3::new(meters!(0f64), meters!(0f64), meters!(0f64)),
            meters!(0.25_f64),
            "#F7F".parse()?,
        );
        Ok(())
    }

    #[method]
    pub fn hide_velocity_vector(&mut self, mut heap: HeapMut) -> Result<()> {
        if let Some(mut markers) = heap.maybe_get_mut::<EntityMarkers>(self.id) {
            markers.remove_motion_arrow("velocity");
        }
        Ok(())
    }

    #[method]
    pub fn show_gravity_vectors(&mut self, mut heap: HeapMut) -> Result<()> {
        if heap.maybe_get::<EntityMarkers>(self.id).is_none() {
            heap.entity_mut(self.id).insert(EntityMarkers::default());
        }
        let mut markers = heap.get_mut::<EntityMarkers>(self.id);
        markers.add_motion_arrow(
            "gravity",
            Point3::origin(),
            Vector3::new(meters!(0f64), meters!(0f64), meters!(0f64)),
            meters!(0.25_f64),
            "#F7F".parse()?,
        );
        markers.add_motion_arrow(
            "gravity_x",
            Point3::origin(),
            Vector3::new(meters!(0f64), meters!(0f64), meters!(0f64)),
            meters!(0.25_f64),
            "#F77".parse()?,
        );
        markers.add_motion_arrow(
            "gravity_y",
            Point3::origin(),
            Vector3::new(meters!(0f64), meters!(0f64), meters!(0f64)),
            meters!(0.25_f64),
            "#7F7".parse()?,
        );
        markers.add_motion_arrow(
            "gravity_z",
            Point3::new(meters!(0f64), meters!(0f64), meters!(0f64)),
            Vector3::new(meters!(0f64), meters!(0f64), meters!(0f64)),
            meters!(0.25_f64),
            "#77F".parse()?,
        );
        Ok(())
    }

    #[method]
    pub fn hide_gravity_vectors(&mut self, mut heap: HeapMut) -> Result<()> {
        if let Some(mut markers) = heap.maybe_get_mut::<EntityMarkers>(self.id) {
            markers.remove_motion_arrow("gravity");
            markers.remove_motion_arrow("gravity_x");
            markers.remove_motion_arrow("gravity_y");
            markers.remove_motion_arrow("gravity_z");
        }
        Ok(())
    }

    fn update_gravity_vectors(m: &mut EntityMarkers, gravity_wf: &Vector3<f64>) {
        let gravity_x = meters_per_second2!(-gravity_wf.z);
        let gravity_y = meters_per_second2!(gravity_wf.x);
        let gravity_z = meters_per_second2!(-gravity_wf.y);
        let na = meters!(0_f64);
        m.update_motion_arrow_vector("gravity", gravity_wf.map(|tmp| meters!(tmp)));
        m.update_motion_arrow_vector("gravity_x", Vector3::new(na, na, -meters!(gravity_x.f64())));
        m.update_motion_arrow_vector("gravity_y", Vector3::new(meters!(gravity_y.f64()), na, na));
        m.update_motion_arrow_vector("gravity_z", Vector3::new(na, -meters!(gravity_z.f64()), na));
    }

    // Sorry, Clippy, planes are complicated.
    #[allow(clippy::too_many_arguments)]
    fn compute_coef_drag(
        pt: &PlaneType,
        airbrake: &AirbrakeEffector,
        bay: &BayEffector,
        flaps: &FlapsEffector,
        gear: &GearEffector,
        hook: &HookEffector,
        drag_g_load: f64,
        is_loaded: bool,
    ) -> Scalar {
        let mut coef_drag = pt.coef_drag;
        let mut g_drag = pt._gpull_drag;
        if is_loaded {
            coef_drag += pt.loaded_drag;
            g_drag += pt.loaded_gpull_drag;
        }
        scalar!(
            f64::from(coef_drag) / 255.
                + drag_g_load * (f64::from(g_drag) / 255.)
                + f64::from(pt.air_brakes_drag) / 255. * airbrake.position()
                + f64::from(pt.bay_drag) / 255. * bay.position()
                + f64::from(pt.flaps_drag) / 255. * flaps.position()
                + f64::from(pt.gear_drag) / 255. * gear.position()
                + 1. / 255. * hook.position()
        )
    }

    // While FA specifies the max aoa, this is (I think) decorative. We actually
    // deflect the facing based on the max g-load and pitch inceptor (basically
    // short-circuiting the work we'd do anyway to figure out control inputs to
    // make that true). What's not clear is if we need to increase the lift vector
    // with our G-force -- presumably yes, but worth checking later.
    //
    // Coefficients of lift are generally linear from C{L0} to C{Lmax}.
    // Here we use our max-g load as C{Lmax} and compute C{L0} such that it always
    // produces 1G.
    //
    // Note: The effects of atmospheric density etc are baked into the envelope shape.
    //
    // FIXME: test for "backwards" velocities... lift needs to not be totally normal. There are PT params?
    // FIXME: what about ground-effect?
    fn compute_target_g_load(
        &self,
        pt: &PlaneType,
        (pitch_inceptor, flaps): (&PitchInceptor, &FlapsEffector),
        (velocity_cg, altitude, gs_z): (Velocity<Meters, Seconds>, Length<Meters>, f64),
        dt: &Duration,
    ) -> (GloadExtrema, f64) {
        // Compute current min/max g-loading range
        let g_load_minima = pt.envelopes.find_g_load_minima(velocity_cg, altitude);
        let g_load_maxima = pt.envelopes.find_g_load_maxima(velocity_cg, altitude);

        let min_g_load = g_load_minima.min_g_load();
        let max_g_load =
            g_load_maxima.max_g_load() + (f64::from(pt.flaps_lift) / 255.) * flaps.position();

        // Compute current desired g-loading within our current envelope limits
        let incept = pitch_inceptor.position();
        let target_g_load = if relative_eq!(incept, 0.) {
            // 1g unless we max below 1g
            max_g_load.min(gs_z)
        } else if incept > 0. {
            if max_g_load >= 1. {
                // linear in [1..max]
                1. + ((max_g_load - 1.) * pitch_inceptor.position())
            } else {
                // No effect from pulling back below 1 g
                max_g_load
            }
        } else {
            -(min_g_load * pitch_inceptor.position())
        };

        // actual target is not instantaneous with control input, it is controlled by a g/s value
        // in acc/dacc of brv_y of the PT.
        let acc = pt.brv_y.acc64() * dt.as_secs_f64();
        let dacc = pt.brv_y.dacc64() * dt.as_secs_f64();
        let target_g_load = {
            // The symmetry in this algorithm is far more helpful here than clippy's pedantry.
            #[allow(clippy::collapsible_else_if)]
            if target_g_load < 0. {
                if target_g_load < self.g_load {
                    (self.g_load - acc).max(target_g_load)
                } else {
                    (self.g_load + dacc).min(target_g_load)
                }
            } else {
                if target_g_load > self.g_load {
                    (self.g_load + acc).min(target_g_load)
                } else {
                    (self.g_load - dacc).max(target_g_load)
                }
            }
        };
        let g_load = {
            // The symmetry in this algorithm is far more helpful here than clippy's pedantry.
            #[allow(clippy::collapsible_else_if)]
            if target_g_load < 0. {
                if target_g_load < self.g_load {
                    (self.g_load - acc).max(target_g_load)
                } else {
                    (self.g_load + dacc).min(target_g_load)
                }
            } else {
                if target_g_load > self.g_load {
                    (self.g_load + acc).min(target_g_load)
                } else {
                    (self.g_load - dacc).max(target_g_load)
                }
            }
        };

        (g_load_maxima, g_load)
    }

    fn compute_roll_rate(
        pt: &PlaneType,
        roll_inceptor: &RollInceptor,
        max_g_load: GloadExtrema,
        dt: &Duration,
        roll_rate: AngularVelocity<Radians, Seconds>,
    ) -> AngularVelocity<Radians, Seconds> {
        debug_assert_eq!(-pt.brv_x.min64(), pt.brv_x.max64());
        let authority = max_g_load.max_g_load().min(1.);
        let target_roll_rate =
            degrees_per_second!(roll_inceptor.position() * pt.brv_x.max64() * authority);
        let target_roll_rad = radians_per_second!(target_roll_rate);
        if relative_eq!(target_roll_rate.f64(), 0.) {
            return radians_per_second!(0f64);
        }
        let acc = degrees_per_second2!(pt.brv_x.acc64()) * seconds!(dt.as_secs_f64());
        let dacc = degrees_per_second2!(pt.brv_x.dacc64()) * seconds!(dt.as_secs_f64());

        // The symmetry in this algorithm is far more helpful here than clippy's pedantry.
        #[allow(clippy::collapsible_else_if)]
        if target_roll_rate < degrees_per_second!(0.) {
            if target_roll_rate < degrees_per_second!(roll_rate) {
                (roll_rate - acc).max(target_roll_rad)
            } else {
                (roll_rate + dacc).min(target_roll_rad)
            }
        } else {
            if target_roll_rate > degrees_per_second!(roll_rate) {
                (roll_rate + acc).min(target_roll_rad)
            } else {
                (roll_rate - dacc).max(target_roll_rad)
            }
        }
    }

    fn compute_yaw_rate(
        pt: &PlaneType,
        _yaw_inceptor: &YawInceptor,
        dt: &Duration,
        beta: Angle<Radians>,
        yaw_rate: AngularVelocity<Radians, Seconds>,
    ) -> AngularVelocity<Radians, Seconds> {
        // At max turn rate all planes appear to traverse 360 degrees in 120 seconds.
        // let target_yaw_rad = degrees_per_second!(yaw_inceptor.postion() * 3.);

        // TODO: rudder yaw rate (based on max-g-loading? how is the caret chosen?)

        // weathervane into the motion vector
        let max = radians_per_second!(degrees_per_second!(pt.rudder_yaw.max64()));
        let min = radians_per_second!(degrees_per_second!(pt.rudder_yaw.min64()));
        let _acc = degrees_per_second2!(pt.rudder_yaw.acc64()) * seconds!(dt.as_secs_f64());
        let dacc = degrees_per_second2!(pt.rudder_yaw.dacc64()) * seconds!(dt.as_secs_f64());
        let mut s = scalar!(degrees!(beta).f64());
        if degrees!(radians!(beta.f64().abs())) < degrees!(1.) {
            s = s * s * scalar!(beta.sign());
        }
        (yaw_rate + dacc * s).clamp(min * s.abs(), max * s.abs())
    }

    fn compute_pitch_rate(
        pt: &PlaneType,
        (stall_speed, max_altitude): (Option<Velocity<Meters, Seconds>>, Option<Length<Meters>>),
        (velocity_cg, _altitude): (Velocity<Meters, Seconds>, Length<Meters>),
        (target_g_load, alpha): (f64, Angle<Radians>),
        (gravity_z, air_density): (Acceleration<Meters, Seconds>, Density<Kilograms, Meters>),
        dt: &Duration,
    ) -> AngularVelocity<Radians, Seconds> {
        // The target g load gives us a circle radius and angular speed, which gives us a deflection.
        // a = v^2/r = vw
        // w = a / v
        let turn_accel = (scalar!(target_g_load) * *STANDARD_GRAVITY) - gravity_z;

        let mut elevator_pitch_rate = if velocity_cg > meters_per_second!(feet_per_second!(1_f64)) {
            (turn_accel.as_dyn() / velocity_cg.as_dyn()).into()
        } else {
            radians_per_second!(0_f64)
        };
        if stall_speed.is_none() {
            // Above the atmospheric lift limits for 1-g.
            // Modulate the pitch authority by fractional density relative to top of envelope
            if let Some(max_altitude) = max_altitude {
                let max_density =
                    StandardAtmosphere::at_altitude(max_altitude).density::<Kilograms, Meters>();
                elevator_pitch_rate *= max_density / air_density;
            }
        }

        // weathervane into the motion vector
        let max = radians_per_second!(degrees_per_second!(pt.brv_z.max64()));
        let min = radians_per_second!(degrees_per_second!(pt.brv_z.min64()));
        let _acc = degrees_per_second2!(pt.brv_z.acc64()) * seconds!(dt.as_secs_f64());
        let dacc = degrees_per_second2!(pt.brv_z.dacc64()) * seconds!(dt.as_secs_f64());
        let mut s = scalar!(degrees!(alpha).f64());
        if degrees!(radians!(alpha.f64().abs())) < degrees!(1.) {
            s = s * s * scalar!(alpha.sign());
        }
        (elevator_pitch_rate - dacc * s).clamp(min * s.abs(), max * s.abs())
    }

    fn simulate(
        &mut self,
        timestep: &TimeStep,
        (airframe, fuel): (&Airframe, &FuelSystem),
        // 1. Acquire inceptor inputs
        (pitch_inceptor, roll_inceptor, yaw_inceptor, power, airbrake, bay, flaps, gear, hook): (
            &PitchInceptor,
            &RollInceptor,
            &YawInceptor,
            &PowerSystem,
            &AirbrakeEffector,
            &BayEffector,
            &FlapsEffector,
            &GearEffector,
            &HookEffector,
        ),
        (xt, motion, frame): (&TypeRef, &mut BodyMotion, &mut WorldSpaceFrame),
        mut markers: Option<Mut<EntityMarkers>>,
    ) {
        if let Some(_markers) = markers.as_ref() {}

        let dt = timestep.step();
        let pt = xt.pt().expect("PT");

        let grat = frame.position_graticule();
        let altitude = grat.distance;
        let atmosphere = StandardAtmosphere::at_altitude(altitude);
        let air_density = atmosphere.density::<Kilograms, Meters>();
        assert!(air_density.is_finite(), "NaN air density at {altitude}");

        let mass_kg = airframe.dry_mass() + fuel.fuel_mass();

        // 2. Compute AoA and side-slip
        let mut u = motion.vehicle_forward_velocity();
        let mut v = motion.vehicle_sideways_velocity();
        let mut w = motion.vehicle_vertical_velocity();
        let mut q = motion.vehicle_pitch_velocity();
        let mut p = motion.vehicle_roll_velocity();
        let mut r = motion.vehicle_yaw_velocity();
        let uw_mag = (u * u + w * w).sqrt(); // m/s
        let velocity_cg_2 = u * u + v * v + w * w; // m^2/s^2
        let velocity_cg = velocity_cg_2.sqrt();
        let _u_dot = motion.vehicle_forward_acceleration(); // u*
        let _v_dot = motion.vehicle_sideways_acceleration(); // v*
        let _w_dot = motion.vehicle_vertical_acceleration(); // w*
        let alpha = radians!(w.f64().atan2(u.f64()));
        let beta = radians!(v.f64().atan2(uw_mag.f64()));

        // FA specific heuristics
        let stall_speed = pt.envelopes.find_min_lift_speed_at(altitude);
        let max_altitude = pt.envelopes.find_max_lift_altitude_at(velocity_cg);

        // Translate world frame gravity into the plane's body frame (Allerton)
        let gravity_wf = motion.stability().inverse()
            * (-frame.position().vec64().normalize() * STANDARD_GRAVITY.f64());
        let gravity_x = meters_per_second2!(-gravity_wf.z);
        let gravity_y = meters_per_second2!(gravity_wf.x);
        let gravity_z = meters_per_second2!(-gravity_wf.y);
        let gs_z = gravity_z.f64() / STANDARD_GRAVITY.f64();
        if let Some(em) = markers.as_mut() {
            Self::update_gravity_vectors(em, &gravity_wf);
        }

        // 3. Compute coefficients of lift, lift_t, drag, yaw_beta, yaw_inceptor_yaw
        let (max_g_load, target_g_load) = self.compute_target_g_load(
            pt,
            (pitch_inceptor, flaps),
            (velocity_cg, altitude, gs_z),
            dt,
        );
        debug_assert!(target_g_load.is_finite(), "NaN target_g_load");
        let drag_g_load = (target_g_load - gs_z).abs();
        debug_assert!(drag_g_load.is_finite(), "NaN drag_g_load");
        // TODO: reflect munitions in the is_loaded category
        let is_loaded = fuel.has_drop_tanks();
        let coef_drag =
            Self::compute_coef_drag(pt, airbrake, bay, flaps, gear, hook, drag_g_load, is_loaded);
        debug_assert!(coef_drag.f64().is_finite(), "NaN Cd");

        // 4. Compute coefficients of aerodynamic moments of pitch
        // 5. Compute coefficients of aerodynamic moments of roll
        // 6. Compute coefficients of aerodynamic moments of yaw

        // 7. Compute body frame forces
        let lift = mass_kg * *STANDARD_GRAVITY * scalar!(target_g_load);
        let drag: Force<Newtons> = (coef_drag.as_dyn()
            * air_density.as_dyn()
            * (velocity_cg * velocity_cg).as_dyn()
            * meters2!(1_f64).as_dyn())
        .into();
        debug_assert!(drag.is_finite(), "NaN drag");
        let side_force = newtons!(0_f64);

        // 8. Compute engine forces and moments
        let engine_thrust_x = power.current_thrust(&atmosphere, u);
        let engine_thrust_y = newtons!(0_f64);
        let engine_thrust_z = newtons!(0_f64);
        let _engine_moment_pitch = newton_meters!(0_f64);
        let _engine_moment_yaw = newton_meters!(0_f64);
        let _engine_moment_roll = newton_meters!(0_f64);

        // 9. Compute gear forces and moments
        let gear_force_x = newtons!(0_f64);
        let gear_force_y = newtons!(0_f64);
        let gear_force_z = newtons!(0_f64);
        let _gear_moment_pitch = newton_meters!(0_f64);
        let _gear_moment_yaw = newton_meters!(0_f64);
        let _gear_moment_roll = newton_meters!(0_f64);

        // 10. Resolve body frame forces
        let force_x = alpha.sin() * lift - alpha.cos() * drag
            + mass_kg * gravity_x
            + engine_thrust_x
            + gear_force_x;
        let force_y = side_force + mass_kg * gravity_y + engine_thrust_y + gear_force_y;
        let force_z = -alpha.cos() * lift - alpha.sin() * drag
            + mass_kg * gravity_z
            + engine_thrust_z
            + gear_force_z;

        // 11. Compute the body frame acceleration
        // Resolve linear forces into acceleration
        let u_dot = force_x / mass_kg - w * q + v * r;
        let v_dot = force_y / mass_kg - u * r + w * p;
        let w_dot = force_z / mass_kg - v * p + u * q;
        debug_assert!(u_dot.is_finite(), "NaN u_dot");
        debug_assert!(v_dot.is_finite(), "NaN v_dot");
        debug_assert!(w_dot.is_finite(), "NaN w_dot");

        // 12. Compute body frame aerodynamic velocities
        u += u_dot * seconds!(dt.as_secs_f64());
        v += v_dot * seconds!(dt.as_secs_f64());
        w += w_dot * seconds!(dt.as_secs_f64());
        motion.set_vehicle_forward_acceleration(u_dot);
        motion.set_vehicle_forward_velocity(u);
        motion.set_vehicle_sideways_acceleration(v_dot);
        motion.set_vehicle_sideways_velocity(v);
        motion.set_vehicle_vertical_acceleration(w_dot);
        motion.set_vehicle_vertical_velocity(w);
        if let Some(em) = markers.as_mut() {
            em.update_motion_arrow_vector(
                "velocity",
                Vector3::new(v, -w, -u).map(|tmp| meters!(tmp.f64() / 10.)),
            );
        }

        // 13. Include the wind components (north, east and down)
        // 14. Include the turbulence components

        // 15. Compute the Earth velocities
        // Rotate our body velocity into the earth frame.
        // let body_velocity = Vector3::new(u.f64(), v.f64(), w.f64());
        // let world_velocity = motion.stability() * body_velocity;
        // let (latitude_velocity, longitude_velocity, height_velocity) = {
        //     // Use arcball's transform from abs to lat/lon here
        //     let x = world_velocity.x;
        //     let y = world_velocity.y;
        //     let z = world_velocity.z;
        //     let distance = world_velocity.magnitude();
        //     let lon = -x.atan2(z);
        //     let lat = (y / distance).asin();
        //     (lat, lon, meters!(distance))
        // };
        // if let Some(_markers) = markers.as_ref() {
        //     println!(
        //         "lat: {latitude_velocity}, lon: {longitude_velocity}, dist: {height_velocity}"
        //     );
        // }

        // 16. Compute the aircraft latitude and longitude rates

        // 17. Compute the aircraft position
        // TODO: do this from map space
        let velocity_m_s = (motion.stability() * motion.velocity().map(|v| v.f64()))
            .map(|v| meters_per_second!(v));
        let world_pos = frame.position_pt3() + velocity_m_s.map(|v| v * seconds!(dt.as_secs_f64()));
        debug_assert!(world_pos.x.is_finite(), "world x is NaN");
        debug_assert!(world_pos.y.is_finite(), "world y is NaN");
        debug_assert!(world_pos.z.is_finite(), "world z is NaN");
        frame.set_position(world_pos);

        // 18. Compute the body rates in stability axes
        // 19. Compute the body frame moments in stability axes
        // 20. Compute the body frame moments in the body frame
        // 21. Compute the body frame angular accelerations
        // 22. Compute the body rates
        // Or shortcut all of it like FA does...
        p = Self::compute_roll_rate(pt, roll_inceptor, max_g_load, dt, p);
        q = Self::compute_pitch_rate(
            pt,
            (stall_speed, max_altitude),
            (velocity_cg, altitude),
            (target_g_load, alpha),
            (gravity_z, air_density),
            dt,
        );
        r = Self::compute_yaw_rate(pt, yaw_inceptor, dt, beta, r);
        debug_assert!(p.is_finite(), "NaN p");
        debug_assert!(q.is_finite(), "NaN q");
        debug_assert!(r.is_finite(), "NaN r");
        motion.set_vehicle_roll_velocity(p);
        motion.set_vehicle_pitch_velocity(q);
        motion.set_vehicle_yaw_velocity(r);

        // 23. Compute the quaternions
        let rot = UnitQuaternion::from_euler_angles(
            q.f64() * dt.as_secs_f64(),
            -r.f64() * dt.as_secs_f64(),
            -p.f64() * dt.as_secs_f64(),
        );

        // 24. Compute the DCM
        // 25. Compute the Euler angles
        // Or maybe just use quaternions? ¯\_(ツ)_/¯
        *motion.stability_mut() *= rot;

        // Apply additional rotations off of stability axis
        let extra_pitch = degrees!(target_g_load / f64::from(pt.env_max) * f64::from(pt.gpull_aoa));
        let rot = UnitQuaternion::from_euler_angles(radians!(extra_pitch).f64(), 0_f64, 0_f64);
        *frame.facing_mut() = motion.stability() * rot;

        // Store values for various displays
        self.coef_of_drag = coef_drag.f64();
        self.force_of_drag = drag.f64();
        self.max_g_load = max_g_load;
        self.g_load = target_g_load;
    }

    fn sys_simulate(
        timestep: Res<TimeStep>,
        mut query: Query<(
            &mut ClassicFlightModel,
            (&Airframe, &FuelSystem),
            (
                &PitchInceptor,
                &RollInceptor,
                &YawInceptor,
                &PowerSystem,
                &AirbrakeEffector,
                &BayEffector,
                &FlapsEffector,
                &GearEffector,
                &HookEffector,
            ),
            (&TypeRef, &mut BodyMotion, &mut WorldSpaceFrame),
            Option<&mut EntityMarkers>,
        )>,
    ) {
        for (
            mut dynamics,
            (airframe, fuel),
            (pitch_inceptor, roll_inceptor, yaw_inceptor, power, airbrake, bay, flaps, gear, hook),
            (xt, mut motion, mut frame),
            markers,
        ) in query.iter_mut()
        {
            dynamics.simulate(
                &timestep,
                (airframe, fuel),
                (
                    pitch_inceptor,
                    roll_inceptor,
                    yaw_inceptor,
                    power,
                    airbrake,
                    bay,
                    flaps,
                    gear,
                    hook,
                ),
                (xt, &mut motion, &mut frame),
                markers,
            );
        }
    }
}
