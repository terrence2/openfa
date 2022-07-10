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
use crate::controls::throttle::{Throttle, ThrottlePosition};
use absolute_unit::{kilograms, newtons, scalar, Force, Kilograms, Mass, Newtons};
use measure::BodyMotion;
use physical_constants::StandardAtmosphere;
use pt::PlaneType;
use shape::DrawState;
use std::time::Duration;
use xt::TypeRef;

const _AFTERBURNER_ENABLE_SOUND: &str = "&AFTBURN.11K";
const ENGINE_IDLE_CONSUMPTION: f32 = 0.2;

/// Target performance as a percentage with optional afterburner.
/// Actual engine thrust is modeled with other considerations.
/// TODO: better model engine behavior based on atmosphere and nozzle velocity, etc
pub(crate) enum JetEnginePerformance {
    Military(f32),
    Afterburner,
    OutOfFuel,
    // TODO: model flameout
    #[allow(unused)]
    FlameOut,
}

impl JetEnginePerformance {
    pub(crate) fn new_min_power() -> Self {
        Self::Military(0.)
    }

    pub(crate) fn military(&self) -> f32 {
        match self {
            Self::Military(m) => *m,
            Self::Afterburner => 101.,
            Self::OutOfFuel | Self::FlameOut => 0.,
        }
    }

    pub(crate) fn is_afterburner(&self) -> bool {
        matches!(self, Self::Afterburner)
    }

    fn increase(&mut self, delta: f32, max: &ThrottlePosition) -> bool {
        let mut enable_ab = false;
        match self {
            Self::Military(current) => {
                let next = (*current + delta).min(max.military());
                *self = if next >= 100. && max.is_afterburner() {
                    // return a new afterburner state so we can play sounds, etc
                    enable_ab = true;
                    Self::Afterburner
                } else {
                    Self::Military(next)
                };
            }
            Self::Afterburner | Self::OutOfFuel | Self::FlameOut => {}
        }
        enable_ab
    }

    fn decrease(&mut self, delta: f32, min: &ThrottlePosition) -> bool {
        let mut disable_ab = false;
        if self.is_afterburner() {
            disable_ab = true;
            *self = Self::Military(100.);
        }
        if let Self::Military(current) = self {
            let next = (*current - delta).max(min.military());
            *self = Self::Military(next);
        }
        disable_ab
    }
}

impl ToString for JetEnginePerformance {
    fn to_string(&self) -> String {
        match self {
            Self::Afterburner => "AFT".to_owned(),
            Self::Military(m) => format!("{:0.0}%", m),
            Self::OutOfFuel => "OFF".to_owned(),
            Self::FlameOut => "OFF".to_owned(),
        }
    }
}

pub struct JetEngine {
    perf_target: JetEnginePerformance,

    // Engine target response speed to throttle input changes.
    throttle_acc: f32,
    throttle_dacc: f32,

    // model parameters
    max_military_thrust: Force<Newtons>,
    fuel_consumption: Mass<Kilograms>,
    afterburner_thrust: Force<Newtons>,
    afterburner_fuel_consumption: Mass<Kilograms>,
}

impl JetEngine {
    pub fn new_min_power(pt: &PlaneType, draw_state: &mut DrawState) -> Self {
        draw_state.disable_afterburner();
        Self {
            perf_target: JetEnginePerformance::new_min_power(),
            throttle_acc: pt.throttle_acc as f32,
            throttle_dacc: pt.throttle_dacc as f32,
            max_military_thrust: newtons!(pt.thrust),
            fuel_consumption: pt.fuel_consumption.mass::<Kilograms>(),
            afterburner_thrust: newtons!(pt.aft_thrust),
            afterburner_fuel_consumption: pt.aft_fuel_consumption.mass::<Kilograms>(),
        }
    }

    pub fn engine_display(&self) -> String {
        self.perf_target.to_string()
    }

    pub fn forward_thrust(
        &self,
        _atmosphere: &StandardAtmosphere,
        _motion: &BodyMotion,
    ) -> Force<Newtons> {
        match self.perf_target {
            JetEnginePerformance::Afterburner => self.afterburner_thrust,
            JetEnginePerformance::Military(pct) => {
                let power_f = pct / 100.;
                scalar!(power_f) * self.max_military_thrust
            }
            JetEnginePerformance::OutOfFuel | JetEnginePerformance::FlameOut => newtons!(0_f64),
        }
    }

    pub fn fuel_consumption(&self, dt: &Duration) -> Mass<Kilograms> {
        let consumption_rate /* /s */ = if self.perf_target.is_afterburner() {
            self.afterburner_fuel_consumption
        } else {
            let power_f = self.perf_target.military() / 100.;
            let consumption_f = power_f * (1. - ENGINE_IDLE_CONSUMPTION) + ENGINE_IDLE_CONSUMPTION;
            scalar!(consumption_f) * self.fuel_consumption
        };
        scalar!(dt.as_secs_f32()) * consumption_rate
    }

    pub fn set_out_of_fuel(&mut self) {
        self.perf_target = JetEnginePerformance::OutOfFuel;
    }

    pub(crate) fn sys_tick(
        &mut self,
        throttle: &Throttle,
        dt: &Duration,
        draw_state: &mut DrawState,
    ) {
        if self.perf_target.military() < throttle.position().military()
            && self
                .perf_target
                .increase(self.throttle_acc * dt.as_secs_f32(), throttle.position())
        {
            // TODO: play sound
            draw_state.enable_afterburner();
        }
        if self.perf_target.military() > throttle.position().military()
            && self
                .perf_target
                .decrease(self.throttle_dacc * dt.as_secs_f32(), throttle.position())
        {
            draw_state.disable_afterburner();
        }
    }
}

/// Non-jet engines are very weakly modeled in FA
pub struct PistonEngine {
    // todo!
}

impl PistonEngine {}

pub enum PowerPlant {
    Jet(JetEngine),
    Piston(PistonEngine),
}

impl PowerPlant {
    pub fn new_min_power(xt: &TypeRef, draw_state: &mut DrawState) -> Self {
        if let Some(pt) = xt.pt() {
            Self::Jet(JetEngine::new_min_power(pt, draw_state))
        } else {
            Self::Piston(PistonEngine {})
        }
    }

    pub fn engine_display(&self) -> String {
        match self {
            Self::Jet(engine) => engine.engine_display(),
            Self::Piston(_engine) => "N/A".to_owned(),
        }
    }

    pub fn forward_thrust(
        &self,
        atmosphere: &StandardAtmosphere,
        motion: &BodyMotion,
    ) -> Force<Newtons> {
        match self {
            Self::Jet(engine) => engine.forward_thrust(atmosphere, motion),
            Self::Piston(_engine) => newtons!(0_f64),
        }
    }

    pub fn fuel_consumption(&self, dt: &Duration) -> Mass<Kilograms> {
        match self {
            Self::Jet(engine) => engine.fuel_consumption(dt),
            Self::Piston(_engine) => kilograms!(0_f64),
        }
    }

    pub fn set_out_of_fuel(&mut self) {
        match self {
            Self::Jet(engine) => engine.set_out_of_fuel(),
            Self::Piston(_engine) => {}
        }
    }

    pub(crate) fn sys_tick(
        &mut self,
        throttle: &Throttle,
        dt: &Duration,
        draw_state: &mut DrawState,
    ) {
        match self {
            Self::Jet(engine) => engine.sys_tick(throttle, dt, draw_state),
            Self::Piston(_engine) => {}
        }
    }
}
