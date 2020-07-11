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
use chrono::{prelude::*, Duration};
use command::{Bindings, Command};
use failure::Fallible;
use lazy_static::lazy_static;
use nalgebra::{Point3, Unit, UnitQuaternion, Vector3, Vector4};
use std::f64::consts::PI;

/**
 * Orbital mechanics works great. Time, however, does not. The time reference for ephimeris is a
 * position on a spinning thing, whose period drifts human observable amounts over human relevant
 * timespans. To complicate matters further, that spinning thing is itself tidally locked to a mass
 * called the moon, which means that the celestially relevant orbital parameters have to be
 * specified around the "barycenter", rather than about the center of spin. Minor celestial
 * fluctuations are amplified in this system, resulting in a spin rate on Earth that is not
 * a constant. Thus, the reference time and direction are not periodic with respect to each other.
 * To throw a further wrench in the works, we offset the meaning of time occasionally so that
 * things appear to more or less line up locally, confounding the larger picture. So if one wants
 * to use J2000 to find the relative position of planets, one needs to subtract leap seconds, but
 * if one wants the locally relevant spin position of a planet, one must not subtract leap seconds.
 *
 * The name orrery was chosen for this module to put people in mind of the tiny and obviously
 * inaccurate physical solar system models built with gears. Because that is ultimately how this
 * module works: a hack that gives a flavor of the the real thing without trying too hard.
 */

/*
Tables taken from: https://ssd.jpl.nasa.gov/txt/p_elem_t2.txt

               a              e               I                L            long.peri.      long.node.
           AU, AU/Cy     rad, rad/Cy     deg, deg/Cy      deg, deg/Cy      deg, deg/Cy     deg, deg/Cy
------------------------------------------------------------------------------------------------------
Mercury   0.38709843      0.20563661      7.00559432      252.25166724     77.45771895     48.33961819
          0.00000000      0.00002123     -0.00590158   149472.67486623      0.15940013     -0.12214182
Venus     0.72332102      0.00676399      3.39777545      181.97970850    131.76755713     76.67261496
         -0.00000026     -0.00005107      0.00043494    58517.81560260      0.05679648     -0.27274174
EM Bary   1.00000018      0.01673163     -0.00054346      100.46691572    102.93005885     -5.11260389
         -0.00000003     -0.00003661     -0.01337178    35999.37306329      0.31795260     -0.24123856
Mars      1.52371243      0.09336511      1.85181869       -4.56813164    -23.91744784     49.71320984
          0.00000097      0.00009149     -0.00724757    19140.29934243      0.45223625     -0.26852431
Jupiter   5.20248019      0.04853590      1.29861416       34.33479152     14.27495244    100.29282654
         -0.00002864      0.00018026     -0.00322699     3034.90371757      0.18199196      0.13024619
Saturn    9.54149883      0.05550825      2.49424102       50.07571329     92.86136063    113.63998702
         -0.00003065     -0.00032044      0.00451969     1222.11494724      0.54179478     -0.25015002
Uranus   19.18797948      0.04685740      0.77298127      314.20276625    172.43404441     73.96250215
         -0.00020455     -0.00001550     -0.00180155      428.49512595      0.09266985      0.05739699
Neptune  30.06952752      0.00895439      1.77005520      304.22289287     46.68158724    131.78635853
          0.00006447      0.00000818      0.00022400      218.46515314      0.01009938     -0.00606302
Pluto    39.48686035      0.24885238     17.14104260      238.96535011    224.09702598    110.30167986
          0.00449751      0.00006016      0.00000501      145.18042903     -0.00968827     -0.00809981
------------------------------------------------------------------------------------------------------

Table 2b.

Additional terms which must be added to the computation of M
for Jupiter through Pluto, 3000 BC to 3000 AD, as described
in the related document.

                b             c             s            f
---------------------------------------------------------------
Jupiter   -0.00012452    0.06064060   -0.35635438   38.35125000
Saturn     0.00025899   -0.13434469    0.87320147   38.35125000
Uranus     0.00058331   -0.97731848    0.17689245    7.67025000
Neptune   -0.00041348    0.68346318   -0.10162547    7.67025000
Pluto     -0.01262724
---------------------------------------------------------------

a / semi-major axis: 384400 km // conversion, 1AU = 1.496e+8km
e / eccentricity:    0.05490  radians(?)
i / inclination:     5.145 deg
l / mean longitude:
omega_bar / longitude of periapsis:
capital_omega / capital_omega / longitude of ascending node:
*/

pub struct KeplerianElements {
    initial: OrbitalParameters,
    delta_per_century: OrbitalParameters,

    b: f64,
    c: f64,
    s: f64,
    f: f64,
}

impl KeplerianElements {
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::many_single_char_names)]
    pub fn new(
        a: f64,
        e: f64,
        i: f64,
        l: f64,
        omega_bar: f64,
        capital_omega: f64,
        apc: f64,
        epc: f64,
        ipc: f64,
        lpc: f64,
        long_node_pc: f64,
        omega_bar_pc: f64,
        b: f64,
        c: f64,
        s: f64,
        f: f64,
    ) -> Self {
        Self {
            initial: OrbitalParameters::new(a, e, i, l, omega_bar, capital_omega),
            delta_per_century: OrbitalParameters::new(
                apc,
                epc,
                ipc,
                lpc,
                omega_bar_pc,
                long_node_pc,
            ),
            b,
            c,
            s,
            f,
        }
    }

    pub fn at_century(&self, centuries_from_j2000: f64) -> OrbitalParameters {
        OrbitalParameters::new(
            self.project_coord(
                self.initial.a,
                self.delta_per_century.a,
                centuries_from_j2000,
            ),
            self.project_coord(
                self.initial.e,
                self.delta_per_century.e,
                centuries_from_j2000,
            ),
            self.project_coord(
                self.initial.i,
                self.delta_per_century.i,
                centuries_from_j2000,
            ) * PI
                / 180f64,
            self.project_coord(
                self.initial.l,
                self.delta_per_century.l,
                centuries_from_j2000,
            ) * PI
                / 180f64,
            self.project_coord(
                self.initial.omega_bar,
                self.delta_per_century.omega_bar,
                centuries_from_j2000,
            ) * PI
                / 180f64,
            self.project_coord(
                self.initial.capital_omega,
                self.delta_per_century.capital_omega,
                centuries_from_j2000,
            ) * PI
                / 180f64,
        )
    }

    pub fn project_coord(&self, n0: f64, ndot: f64, centuries_from_j2000: f64) -> f64 {
        n0 + ndot * centuries_from_j2000
            + self.b * centuries_from_j2000.powf(2f64)
            + self.c * (self.f * centuries_from_j2000).cos()
            + self.s * (self.f * centuries_from_j2000).sin()
    }
}

pub struct OrbitalParameters {
    a: f64,             // AU
    e: f64,             // rad
    i: f64,             // deg
    l: f64,             // deg
    omega_bar: f64,     // deg
    capital_omega: f64, // deg
}

impl OrbitalParameters {
    pub fn new(
        a: f64,
        e: f64,
        i: f64,             // deg
        l: f64,             // deg
        omega_bar: f64,     // deg
        capital_omega: f64, // deg
    ) -> Self {
        Self {
            a,
            e,
            i,
            l,
            omega_bar,
            capital_omega,
        }
    }

    // Returns in AU.
    // Method taken from: https://space.stackexchange.com/questions/8911/determining-orbital-position-at-a-future-point-in-time
    #[allow(non_snake_case)]
    #[allow(clippy::many_single_char_names)]
    pub fn eccliptic_position(&self) -> Point3<f64> {
        let i = self.i;
        let l = self.l;
        let omega_bar = self.omega_bar;
        let capital_omega = self.capital_omega;

        // M = l - w|  =>  mean anomaly = mean longitude - longitude of the periapsis
        let M = l - omega_bar; // mean anomaly

        // argument_of_periapsis + longitude_of_ascending_node = longitude_of_periapsis
        let w = omega_bar - capital_omega; // argument of periapsis

        // Solve Euler's equation using Newton's method.
        let mut E = M;
        loop {
            let dE = (E - self.e * E.sin() - M) / (1f64 - self.e * E.cos());
            E -= dE;
            if dE.abs() < 1e-6 {
                break;
            }
        }

        // Convert to polar.
        let P = self.a * (E.cos() - self.e);
        let Q = self.a * E.sin() * (1f64 - self.e.powf(2f64)).sqrt();

        // Rotate the 2d frame into 3d
        // rotate by argument of periapsis
        let x = w.cos() * P - w.sin() * Q;
        let y = w.sin() * P + w.cos() * Q;
        // rotate by inclination
        let z = i.sin() * x;
        let x = i.cos() * x;
        // rotate by longitude of ascending node
        let xtemp = x;
        let x = capital_omega.cos() * xtemp - capital_omega.sin() * y;
        let y = capital_omega.sin() * xtemp + capital_omega.cos() * y;

        Point3::new(x, y, z)
    }
}

lazy_static! {
    static ref LEAP_SECONDS: Vec<DateTime<Utc>> = {
        let mut v = Vec::new();
        v.push(Utc.ymd(1972, 6, 30).and_hms(23, 59, 59));
        v.push(Utc.ymd(1972, 12, 31).and_hms(23, 59, 59));
        v.push(Utc.ymd(1973, 12, 31).and_hms(23, 59, 59));
        v.push(Utc.ymd(1974, 12, 31).and_hms(23, 59, 59));
        v.push(Utc.ymd(1975, 12, 31).and_hms(23, 59, 59));

        v.push(Utc.ymd(1976, 12, 31).and_hms(23, 59, 59));
        v.push(Utc.ymd(1977, 12, 31).and_hms(23, 59, 59));
        v.push(Utc.ymd(1978, 12, 31).and_hms(23, 59, 59));
        v.push(Utc.ymd(1979, 12, 31).and_hms(23, 59, 59));
        v.push(Utc.ymd(1981, 6, 30).and_hms(23, 59, 59));

        v.push(Utc.ymd(1982, 6, 30).and_hms(23, 59, 59));
        v.push(Utc.ymd(1983, 6, 30).and_hms(23, 59, 59));
        v.push(Utc.ymd(1985, 6, 30).and_hms(23, 59, 59));
        v.push(Utc.ymd(1987, 12, 31).and_hms(23, 59, 59));
        v.push(Utc.ymd(1989, 12, 31).and_hms(23, 59, 59));

        v.push(Utc.ymd(1990, 12, 31).and_hms(23, 59, 59));
        v.push(Utc.ymd(1992, 6, 30).and_hms(23, 59, 59));
        v.push(Utc.ymd(1993, 6, 30).and_hms(23, 59, 59));
        v.push(Utc.ymd(1994, 6, 30).and_hms(23, 59, 59));
        v.push(Utc.ymd(1995, 12, 31).and_hms(23, 59, 59));

        v.push(Utc.ymd(1997, 6, 30).and_hms(23, 59, 59));
        v.push(Utc.ymd(1998, 12, 31).and_hms(23, 59, 59));
        v.push(Utc.ymd(2005, 12, 31).and_hms(23, 59, 59));
        v.push(Utc.ymd(2008, 12, 31).and_hms(23, 59, 59));
        v.push(Utc.ymd(2012, 6, 30).and_hms(23, 59, 59));

        v.push(Utc.ymd(2015, 6, 30).and_hms(23, 59, 59));
        v.push(Utc.ymd(2016, 12, 31).and_hms(23, 59, 59));
        v.reverse();
        v
    };
}

pub struct Orrery {
    earth_moon_barycenter: KeplerianElements,

    now: DateTime<Utc>,
    in_debug_override: bool,
}

impl Orrery {
    pub fn now() -> Self {
        Self::new(Utc::now())
    }

    #[allow(clippy::unreadable_literal)]
    #[rustfmt::skip]
    pub fn new(initial_time: DateTime<Utc>) -> Self {
        Self {
            //EM Bary   1.00000018      0.01673163     -0.00054346      100.46691572    102.93005885     -5.11260389
            //         -0.00000003     -0.00003661     -0.01337178    35999.37306329      0.31795260     -0.24123856
            earth_moon_barycenter: KeplerianElements::new(
                 1.00000018,  0.01673163, -0.00054346,   100.46691572, 102.93005885, -5.11260389,
                -0.00000003, -0.00003661, -0.01337178, 35999.37306329,   0.31795260, -0.24123856,
                0.0, 0.0, 0.0, 0.0,
            ),

            now: initial_time,
            in_debug_override: false,
        }
    }

    pub fn get_time(&self) -> DateTime<Utc> {
        self.now
    }

    fn num_leap_seconds(&self) -> Duration {
        for (offset, date) in LEAP_SECONDS.iter().enumerate() {
            if self.now > *date {
                return Duration::seconds((LEAP_SECONDS.len() - offset) as i64);
            }
        }
        Duration::seconds(0)
    }

    fn centuries_from_j2000(&self) -> f64 {
        // Note that 364.25 days per year is definitional to j2000 ecliptic coordinates. It allows
        // us to accurately determine the position of the planets relative to the sun in the
        // ecliptic plane, (given the additional drift parameters), assuming that we have the time
        // offset from January 2000 without leap seconds added.

        const MILLIS_PER_CENTURY: f64 = 1000f64 * 60f64 * 60f64 * 24f64 * 364.25f64 * 100f64;
        let from_j2000 = self.now - Utc.ymd(2000, 1, 1).and_hms(12, 0, 0) + self.num_leap_seconds();
        (from_j2000.num_milliseconds() as f64) / MILLIS_PER_CENTURY
    }

    fn days_from_jan1(&self) -> f64 {
        // Given leap seconds, we can assume that the earth's rotation is pointing a more or less
        // consistent direction every year at UTC time Jan 1, 12:00 PM. Thus, we want to get the
        // number of days from Jan 1 to now.
        const MILLIS_PER_DAY: f64 = 1000f64 * 60f64 * 60f64 * 24f64;
        let from_base = self.now - Utc.ymd(self.now.year(), 1, 1).and_hms(12, 0, 0);
        (from_base.num_milliseconds() as f64) / MILLIS_PER_DAY
    }

    //fn earth_position(&self) -> Point3<f64> {}

    pub fn sun_direction(&self) -> Vector3<f64> {
        let centuries_from_j2000 = self.centuries_from_j2000();

        // Get sun position in ecliptic, earth centric.
        let orbit = self.earth_moon_barycenter.at_century(centuries_from_j2000);
        let sun_position_ecliptic = -orbit.eccliptic_position();

        // Convert to equitorial coordinates from the eccliptic
        const AXIAL_TILT_AT_J2000: f64 = PI / 180f64 * 23.439_3;
        const AXIAL_TILT_PER_DAY_DEG: f64 = -3.563E-7;
        const AXIAL_TILT_PER_CENTURY: f64 =
            PI / 180f64 * (AXIAL_TILT_PER_DAY_DEG * 365.242_19 * 100.0);
        let axial_tilt = AXIAL_TILT_AT_J2000 + AXIAL_TILT_PER_CENTURY * centuries_from_j2000;
        let x_eq = sun_position_ecliptic.x;
        let y_eq = -sun_position_ecliptic.y * axial_tilt.cos();
        let z_eq = sun_position_ecliptic.y * axial_tilt.sin();
        let sun_position_equitorial = Point3::new(x_eq, z_eq, y_eq);

        // Rotate once per day, starting at the nearest year boundary, counting on leap seconds
        // to ensure that the angle at Jan 1 (Utc) is consistent.
        let rot = UnitQuaternion::from_axis_angle(
            &Unit::new_unchecked(Vector3::new(0f64, -1f64, 0f64)),
            self.days_from_jan1() * 2f64 * PI,
        );

        Vector4::from(rot * sun_position_equitorial)
            .xyz()
            .normalize()
    }

    pub fn debug_bindings() -> Fallible<Bindings> {
        Ok(Bindings::new("orrery").bind("+move-sun", "mouse2")?)
    }

    pub fn handle_command(&mut self, command: &Command) -> Fallible<()> {
        match command.name.as_str() {
            "+move-sun" => self.in_debug_override = true,
            "-move-sun" => self.in_debug_override = false,
            "mouse-move" => {
                if self.in_debug_override {
                    let hours = command.displacement()?.0 as i64;
                    //println!("ADDING minutes: {}", minutes);
                    self.now = self.now.checked_add_signed(Duration::hours(hours)).unwrap();
                }
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let orrery = Orrery::new(Utc::now());
        orrery.sun_direction();
    }

    #[test]
    fn test_leap_seconds() {
        assert_eq!(
            Orrery::new(Utc.ymd(2020, 1, 1).and_hms(12, 0, 0)).num_leap_seconds(),
            Duration::seconds(27)
        );
        assert_eq!(
            Orrery::new(Utc.ymd(2010, 1, 1).and_hms(12, 0, 0)).num_leap_seconds(),
            Duration::seconds(24)
        );
        assert_eq!(
            Orrery::new(Utc.ymd(1969, 1, 1).and_hms(12, 0, 0)).num_leap_seconds(),
            Duration::seconds(0)
        );
    }
}
