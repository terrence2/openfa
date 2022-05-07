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
use absolute_unit::{
    feet, feet_per_second, meters, meters_per_second, meters_per_second2, Acceleration, Feet,
    Length, Meters, Seconds, Velocity,
};
use anyhow::{bail, ensure, Result};
use nalgebra::Vector2;
use ot::{
    make_type_struct,
    parse::{FieldRow, FromRows},
};
use std::collections::HashMap;

#[derive(Debug, Ord, PartialOrd, Eq, PartialEq)]
enum EnvelopeVersion {
    V0,
}

impl EnvelopeVersion {
    #[allow(clippy::unnecessary_wraps)] // actually necessary
    fn from_len(_: usize) -> Result<Self> {
        Ok(EnvelopeVersion::V0)
    }
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct EnvelopeCoord {
    speed: Velocity<Meters, Seconds>,
    altitude: Length<Meters>,
}

impl EnvelopeCoord {
    pub fn speed(&self) -> Velocity<Meters, Seconds> {
        self.speed
    }

    pub fn altitude(&self) -> Length<Meters> {
        self.altitude
    }
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct EnvelopeShape {
    shape: Vec<EnvelopeCoord>, // max of 20
}

impl FromRows for EnvelopeShape {
    type Produces = EnvelopeShape;
    fn from_rows(
        rows: &[FieldRow],
        _pointers: &HashMap<&str, Vec<&str>>,
    ) -> Result<(Self::Produces, usize)> {
        let mut shape = Vec::new();
        for j in 0..20 {
            let speed = u32::from(rows[j * 2].value().numeric()?.word()?) as i32;
            let altitude = rows[j * 2 + 1].value().numeric()?.dword()? as i32;
            shape.push(EnvelopeCoord {
                speed: meters_per_second!(feet_per_second!(speed)),
                altitude: meters!(feet!(altitude)),
            });
        }
        Ok((Self { shape }, 40))
    }
}

impl EnvelopeShape {
    pub fn coord(&self, offset: usize) -> &EnvelopeCoord {
        &self.shape[offset]
    }

    pub fn is_in_envelope(
        &self,
        speed: Velocity<Meters, Seconds>,
        altitude: Length<Meters>,
    ) -> bool {
        let o = Vector2::new(speed.f64(), altitude.f64());
        let m = Vector2::new(5000f64, altitude.f64());

        let v3 = Vector2::new(0f64, -1f64);
        let mut cnt = 0;
        for (i, coord0) in self.shape.iter().enumerate() {
            let j = (i + 1) % self.shape.len();
            let coord1 = &self.shape[j];

            let a = Vector2::new(coord0.speed.f64(), coord0.altitude.f64());
            let b = Vector2::new(coord1.speed.f64(), coord1.altitude.f64());

            let v1 = a - o;
            let v2 = b - o;
            let v3 = m - o;

            if v1.perp(&v3).signum() == v3.perp(&v2).signum() && v3.dot(&(v1 + v2)) > 0. {
                cnt += 1;
            }

            // let v1 = o - a;
            // let v2 = b - a;
            // let t1 = v2.perp(&v1) / v2.dot(&v3);
            // let t2 = v1.dot(&v3) / v2.dot(&v3);
            // println!("{i}-{j} => {t1}, {t2}");
        }
        // println!("CNT: {}", cnt);
        // false
        cnt % 2 == 0
    }
}

make_type_struct![
Envelope(parent: (), version: EnvelopeVersion) {
(Word, [Dec],     "env [ii].gload", Signed, gload,          i16, V0, panic!()), // word 5 ; env [ii].gload
(Word, [Dec],     "env [ii].count", Signed, count,          i16, V0, panic!()), // word 5 ; env [ii].count
(Word, [Dec], "env [ii].stallLift", Signed, stall_lift,     i16, V0, panic!()), // word 2 ; env [ii].stallLift
(Word, [Dec],  "env [ii].maxSpeed", Signed, max_speed_index,i16, V0, panic!()), // word 3 ; env [ii].maxSpeed
(Num,  [Dec], "env [ii].data [j].",CustomN, shape,EnvelopeShape, V0, panic!())  // word 300 ; env [ii].data [j].speed
}];

impl Envelope {
    pub fn find_g_load_extrema(
        &self,
        speed: Velocity<Meters, Seconds>,
        altitude: Length<Meters>,
    ) -> bool {
        // if speed.f64() > 1. {
        //     let hit = self.shape.is_in_envelope(speed, altitude);
        //     println!("{:>4} => {hit}", self.gload);
        //     return hit;
        // }
        false
    }
}
