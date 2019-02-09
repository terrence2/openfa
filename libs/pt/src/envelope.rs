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
use failure::{bail, ensure, Fallible};
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
    fn from_len(_: usize) -> Fallible<Self> {
        Ok(EnvelopeVersion::V0)
    }
}

#[allow(dead_code)]
pub struct EnvelopeCoord {
    speed: f32,
    altitude: f32,
}

#[allow(dead_code)]
pub struct EnvelopeShape {
    shape: Vec<EnvelopeCoord>, // max of 20
}

impl FromRows for EnvelopeShape {
    type Produces = EnvelopeShape;
    fn from_rows(
        rows: &[FieldRow],
        _pointers: &HashMap<&str, Vec<&str>>
    ) -> Fallible<(Self::Produces, usize)> {
        let mut shape = Vec::new();
        for j in 0..20 {
            let speed = u32::from(rows[j * 2].value().numeric()?.word()?) as i32 as f32;
            let altitude = rows[j * 2 + 1].value().numeric()?.dword()? as i32 as f32;
            shape.push(EnvelopeCoord { speed, altitude });
        }
        Ok((Self { shape }, 40))
    }
}

make_type_struct![
Envelope(parent: (), version: EnvelopeVersion) {
(Word, [Dec],     "env [ii].gload", Signed, gload,          i16, V0, panic!()), // word 5 ; env [ii].gload
(Word, [Dec],     "env [ii].count", Signed, count,          i16, V0, panic!()), // word 5 ; env [ii].count
(Word, [Dec], "env [ii].stallLift", Signed, stallLift,      i16, V0, panic!()), // word 2 ; env [ii].stallLift
(Word, [Dec],  "env [ii].maxSpeed", Signed, maxSpeed,       i16, V0, panic!()), // word 3 ; env [ii].maxSpeed
(Num,  [Dec], "env [ii].data [j].",CustomN, shape,EnvelopeShape, V0, panic!())  // word 300 ; env [ii].data [j].speed
                                                                                // dword 0 ; env [ii].data [j].alt
}];
