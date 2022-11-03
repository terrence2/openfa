// This file is part of Nitrogen.
//
// Nitrogen is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Nitrogen is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with Nitrogen.  If not, see <http://www.gnu.org/licenses/>.
use absolute_unit::*;
use anyhow::{ensure, Result};
use physical_constants::{StandardAtmosphere, STANDARD_GRAVITY};
use std::time::Duration;
use vehicle::{Engine, EnginePower, ThrottlePosition};
use xt::TypeRef;

/// A jet engine modeled on a base thrust and various inlet and limit factors.
#[derive(Debug, Clone)]
pub struct Turbojet {
    // Current engine setting as a percentage.
    power: EnginePower,

    // Pointer to the type, for fast access to the data we need to compute thrust.
    xt: TypeRef,
}

impl Engine for Turbojet {
    fn adjust_power(&mut self, throttle: &ThrottlePosition, dt: &Duration) {
        if self.power.military() < throttle.military() {
            self.power.increase(
                f64::from(self.xt.xpt().throttle_acc) * dt.as_secs_f64(),
                throttle,
            );
        }
        if self.power.military() > throttle.military() {
            self.power.decrease(
                f64::from(self.xt.xpt().throttle_dacc) * dt.as_secs_f64(),
                throttle,
            );
        }
    }

    fn current_power(&self) -> &EnginePower {
        &self.power
    }

    fn compute_thrust(
        &self,
        atmosphere: &StandardAtmosphere,
        velocity: Velocity<Meters, Seconds>,
    ) -> Force<Newtons> {
        let pt = self.xt.xpt();

        // Engine power in [0..N], where afterburner is fractional above 100% military.
        let power_base_mil = match self.power {
            EnginePower::Military(f) => scalar!(f / 100.),
            EnginePower::Afterburner(_) => pt.aft_thrust / pt.thrust,
            _ => scalar!(0.),
        };
        let power = if pt.aft_thrust > pounds_force!(0) {
            power_base_mil * (pt.thrust / pt.aft_thrust)
        } else {
            power_base_mil
        };

        // TODO: try out mapping to envelope
        // Determined empirically; best fit for F-22, B-52, acceptable for F-4 and F-104, but not great.
        // =POW(1.6 * C$60 - 0.92, 2) + 0.89
        let power_max_speed_effect = scalar!((1.6 * power.f64() - 0.92).powf(2.) + 0.89);
        let max_sea_speed = power_max_speed_effect * knots!(pt.max_speed_sea_level);
        let max_36a_speed = power_max_speed_effect * knots!(pt.max_speed_36a);

        // Lerp to get a max speed at current altitude.
        let altitude_ft = feet!(atmosphere.geopotential_altitude());
        let max_speed_at_altitude: Velocity<NauticalMiles, Hours> = max_sea_speed
            + (max_36a_speed - max_sea_speed) * scalar!((altitude_ft / feet!(36_000f64)).min(1.));
        let max_speed_at_power = power * max_speed_at_altitude;

        // What force would allow us to meet this speed, given a nominal drag.
        // Fd = 1/2 p vv A Cd  // ft lb s
        let nominal_thrust: Force<PoundsForce> = (scalar!(f64::from(pt.coef_drag) / 255.).as_dyn()
            * (feet_per_second!(max_speed_at_power) * feet_per_second!(max_speed_at_power))
                .as_dyn()
            * atmosphere.density::<PoundsMass, Feet>().as_dyn()
            * feet2!(meters2!(1f64)).as_dyn())
        .into();

        // Account for inlet falloff at front of envelope.
        let inlet_falloff_f = if let Some(max_velocity_0) = pt
            .envelopes
            .envelope(0)
            .unwrap()
            .find_max_velocity_at(atmosphere.geopotential_altitude())
        {
            let velocity = knots!(velocity);
            let max_velocity_0 = knots!(max_velocity_0);
            if velocity > max_velocity_0 {
                (1. - ((velocity - max_velocity_0).f64() / 100.).powf(2.)).max(0.)
            } else {
                1.
            }
        } else {
            // FIXME: handle flameout, reduce thrust gently above the line
            // No intercept at right means no thrust.
            0.
        };

        let thrust = newtons!(nominal_thrust * scalar!(inlet_falloff_f));

        // Note: could imperial units please go die in a fire already
        thrust / scalar!(feet_per_second2!(*STANDARD_GRAVITY).f64())

        /* Trivial model
        let density_f = (atmosphere.pressure::<Pascals>() / pascals!(101_325_f64));
        if self.power.is_afterburner() {
            // FIXME: almost certainly wrong
            return newtons!(self.xt.xpt().aft_thrust) * density_f;
        }
        return newtons!(self.xt.xpt().thrust) * scalar!(self.power.military() / 100.) * density_f;
         */
    }

    fn compute_fuel_use(&self, dt: &Duration) -> Mass<Kilograms> {
        let base = if self.power.is_afterburner() {
            kilograms_per_second!(self.xt.xpt().aft_fuel_consumption)
        } else {
            kilograms_per_second!(self.xt.xpt().fuel_consumption)
                * scalar!(self.power.military() / 100.)
        };
        base * seconds!(dt.as_secs_f64())
    }

    fn set_out_of_fuel(&mut self) {
        self.power = EnginePower::OutOfFuel;
    }
}

impl Turbojet {
    pub fn new_min_power(xt: TypeRef) -> Result<Self> {
        ensure!(xt.is_pt());
        Ok(Self {
            power: EnginePower::Military(0.),
            xt,
        })
    }
}
