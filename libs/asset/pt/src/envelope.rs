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
    feet, feet_per_second, meters, meters_per_second, Length, Meters, Seconds, Velocity,
};
use anyhow::{bail, ensure, Result};
use nalgebra::Vector2;
use ot::{
    make_type_struct,
    parse::{FieldRow, FromRows},
};
use std::{collections::HashMap, fmt};

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

// Result of testing a position with an envelope. Tracks where inside we are, or how far out in
// what direction. This lets us interpolate to get sub-G results from a nested envelope set.
pub enum EnvelopeIntersection {
    Inside {
        to_stall: f64,
        to_over_speed: f64,
        to_lift_fail: f64,
    },
    Stall(f64),
    LiftFail(f64),
    OverSpeed(f64),
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

    // https://stackoverflow.com/questions/563198/how-do-you-detect-where-two-line-segments-intersect
    #[inline]
    fn segment_intersection(
        (p0_x, p0_y): (f64, f64),
        (p1_x, p1_y): (f64, f64),
        (p2_x, p2_y): (f64, f64),
        (p3_x, p3_y): (f64, f64),
    ) -> Option<(f64, f64)> {
        let s1_x = p1_x - p0_x;
        let s1_y = p1_y - p0_y;
        let s2_x = p3_x - p2_x;
        let s2_y = p3_y - p2_y;

        let s = (-s1_y * (p0_x - p2_x) + s1_x * (p0_y - p2_y)) / (-s2_x * s1_y + s1_x * s2_y);
        let t = (s2_x * (p0_y - p2_y) - s2_y * (p0_x - p2_x)) / (-s2_x * s1_y + s1_x * s2_y);

        if s >= 0. && s <= 1. && t >= 0. && t <= 1. {
            let i_x = p0_x + (t * s1_x);
            let i_y = p0_y + (t * s1_y);
            return Some((i_x, i_y));
        }
        None
    }

    pub fn is_in_envelope(
        &self,
        speed: Velocity<Meters, Seconds>,
        altitude: Length<Meters>,
    ) -> EnvelopeIntersection {
        let origin = Vector2::new(speed.f64(), altitude.f64());

        let ends = [
            // Left
            Vector2::new(-1_000f64, altitude.f64()),
            // Right
            Vector2::new(7_000f64, altitude.f64()),
            // Down
            Vector2::new(speed.f64(), -1_000f64),
            // Up
            Vector2::new(speed.f64(), 120_000f64),
        ];
        let mut counts = [0, 0, 0, 0];
        let mut minimums2 = [f64::INFINITY, f64::INFINITY, f64::INFINITY, f64::INFINITY];

        for (i, coord0) in self.shape.iter().enumerate() {
            let j = (i + 1) % self.shape.len();
            let coord1 = &self.shape[j];

            for dir in 0..4 {
                if let Some((intersect_x, intersect_y)) = Self::segment_intersection(
                    (origin.x, origin.y),
                    (ends[dir].x, ends[dir].y),
                    (coord0.speed().f64(), coord0.altitude().f64()),
                    (coord1.speed().f64(), coord1.altitude().f64()),
                ) {
                    counts[dir] += 1;
                    let dx = intersect_x - origin.x;
                    let dy = intersect_y - origin.y;
                    let d2 = dx * dx + dy * dy;
                    if d2 < minimums2[dir] {
                        minimums2[dir] = d2;
                    }
                }
            }
        }

        // If one intersects then, in a perfect universe, all others would intersect.
        let intersect = counts.map(|c| c > 0 && c % 2 == 1);
        if intersect[0] || intersect[1] || intersect[2] || intersect[3] {
            return EnvelopeIntersection::Inside {
                to_stall: minimums2[0].sqrt(),
                to_over_speed: minimums2[1].sqrt(),
                to_lift_fail: minimums2[3].sqrt(),
            };
        }

        let visible = counts.map(|c| c > 0);
        match visible {
            [true, _, _, _] => EnvelopeIntersection::OverSpeed(minimums2[0].sqrt()),
            [_, true, _, _] => EnvelopeIntersection::Stall(minimums2[1].sqrt()),
            [_, _, true, _] => EnvelopeIntersection::LiftFail(minimums2[2].sqrt()),
            _ => {
                // Upper left or upper right, or below.
                if altitude < meters!(0f64) {
                    EnvelopeIntersection::Inside {
                        to_stall: minimums2[0].sqrt(),
                        to_over_speed: minimums2[1].sqrt(),
                        to_lift_fail: minimums2[3].sqrt(),
                    }
                } else {
                    EnvelopeIntersection::Stall(minimums2[1].sqrt())
                }
            }
        }
    }

    pub fn find_min_lift_speed_at(
        &self,
        altitude: Length<Meters>,
    ) -> Option<Velocity<Meters, Seconds>> {
        let origin = Vector2::new(0_f64, altitude.f64());
        let end = Vector2::new(10_000_f64, altitude.f64());

        let mut minima = None;

        for (i, coord0) in self.shape.iter().enumerate() {
            let j = (i + 1) % self.shape.len();
            let coord1 = &self.shape[j];

            if let Some((intersect_x, intersect_y)) = Self::segment_intersection(
                (origin.x, origin.y),
                (end.x, end.y),
                (coord0.speed().f64(), coord0.altitude().f64()),
                (coord1.speed().f64(), coord1.altitude().f64()),
            ) {
                let dx = intersect_x - origin.x;
                let dy = intersect_y - origin.y;
                let d = meters_per_second!((dx * dx + dy * dy).sqrt());
                if let Some(m) = minima {
                    if d < m {
                        minima = Some(d);
                    }
                } else {
                    minima = Some(d);
                }
            }
        }

        minima
    }
}

impl fmt::Display for EnvelopeShape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<EnvelopeShape:{}>", self.shape.len())
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
    ) -> EnvelopeIntersection {
        self.shape.is_in_envelope(speed, altitude)
    }

    pub fn find_min_lift_speed_at(
        &self,
        altitude: Length<Meters>,
    ) -> Option<Velocity<Meters, Seconds>> {
        self.shape.find_min_lift_speed_at(altitude)
    }
}
