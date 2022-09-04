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
use absolute_unit::{scalar, Scalar};
use animate::TimeStep;
use anyhow::Result;
use bevy_ecs::prelude::*;
use marker::EntityMarkers;
use measure::{BodyMotion, WorldSpaceFrame};
use nitrous::{inject_nitrous_component, method, NitrousComponent};
use pt::{GloadExtrema, PlaneType};
use runtime::{Extension, Runtime};
use vehicle::{
    AirbrakeEffector, BayEffector, FlapsEffector, GearEffector, HookEffector, PitchInceptor,
    RollInceptor, YawInceptor,
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
    _id: Entity,

    // Current envelope
    max_g_load: GloadExtrema,
}

impl Extension for ClassicFlightModel {
    fn init(runtime: &mut Runtime) -> Result<()> {
        runtime.add_sim_system(Self::sys_simulate.label(ClassicFlightModelStep::Simulate));
        Ok(())
    }
}

#[inject_nitrous_component]
impl ClassicFlightModel {
    // Derived empirically from testing
    const FORCE_COEF_SCALAR: f64 = 0.048_557_844_22;

    pub fn new(id: Entity) -> Self {
        Self {
            _id: id,
            max_g_load: GloadExtrema::Stall(0.),
        }
    }

    pub fn max_g_load(&self) -> &GloadExtrema {
        &self.max_g_load
    }

    #[method]
    pub fn max_g(&self) -> f64 {
        self.max_g_load.max_g_load()
    }

    fn calculate_coef_drag(
        &self,
        pt: &PlaneType,
        airbrake: &AirbrakeEffector,
        bay: &BayEffector,
        flaps: &FlapsEffector,
        gear: &GearEffector,
        hook: &HookEffector,
        // TODO: aero surfaces + munitions
    ) -> Scalar {
        let coef_drag = f64::from(pt.coef_drag)
            + f64::from(pt.air_brakes_drag) * airbrake.position()
            + f64::from(pt.bay_drag) * bay.position()
            + f64::from(pt.flaps_drag) * flaps.position()
            + f64::from(pt.gear_drag) * gear.position()
            + hook.position();
        scalar!(Self::FORCE_COEF_SCALAR) * scalar!(coef_drag)
    }

    fn simulate(
        &mut self,
        timestep: &TimeStep,
        xt: &TypeRef,
        _motion: &mut BodyMotion,
        _frame: &mut WorldSpaceFrame,
        _markers: Option<Mut<EntityMarkers>>,
    ) {
        let _dt = timestep.step();
        let _pt = xt.pt().expect("PT");
    }

    fn sys_simulate(
        timestep: Res<TimeStep>,
        mut query: Query<(
            &mut ClassicFlightModel,
            (
                &AirbrakeEffector,
                &BayEffector,
                &FlapsEffector,
                &GearEffector,
                &HookEffector,
            ),
            (&PitchInceptor, &RollInceptor, &YawInceptor),
            (&TypeRef, &mut BodyMotion, &mut WorldSpaceFrame),
            Option<&mut EntityMarkers>,
        )>,
    ) {
        for (
            mut dynamics,
            (airbrake, bay, flaps, gear, hook),
            (_pitch_inceptor, _roll_inceptor, _yaw_inceptor),
            // (ailerons, pitch_inceptor, rudder),
            (xt, mut motion, mut frame),
            markers,
        ) in query.iter_mut()
        {
            let pt = xt.pt().expect("PT");
            let _coef_drag = dynamics.calculate_coef_drag(pt, airbrake, bay, flaps, gear, hook);
            // let engine_thrust =
            //     dynamics.calculate_thrust(pt, )
            dynamics.simulate(
                &timestep,
                xt,
                // airbrake,
                // flaps,
                // hook,
                // bay,
                // gear,
                // vehicle,
                // ailerons,
                // pitch_inceptor,
                // rudder,
                &mut motion,
                &mut frame,
                markers,
            );
        }
    }
}
