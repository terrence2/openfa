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
use absolute_unit::{degrees, kilometers, meters, Angle, Length, LengthUnit, Meters, Radians};
use std::{fmt, marker::PhantomData};

pub trait GraticuleOrigin: Copy {
    fn origin_marker() -> &'static str;
}

#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
pub struct Graticule<Origin>
where
    Origin: GraticuleOrigin,
{
    pub latitude: Angle<Radians>,
    pub longitude: Angle<Radians>,
    pub distance: Length<Meters>,
    phantom: PhantomData<Origin>,
}

impl<Origin> Graticule<Origin>
where
    Origin: GraticuleOrigin,
{
    pub fn new<Unit: LengthUnit>(
        latitude: Angle<Radians>,
        longitude: Angle<Radians>,
        distance: Length<Unit>,
    ) -> Self {
        Self {
            latitude,
            longitude,
            distance: meters!(distance),
            phantom: PhantomData,
        }
    }
}

impl<Origin> fmt::Display for Graticule<Origin>
where
    Origin: GraticuleOrigin,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "({}, {})[{}]{}",
            degrees!(self.latitude),
            degrees!(self.longitude),
            self.distance,
            Origin::origin_marker(),
        )
    }
}

// FIXME: manual conversions
use crate::{GeoCenter, GeoSurface};
impl From<Graticule<GeoSurface>> for Graticule<GeoCenter> {
    fn from(surface: Graticule<GeoSurface>) -> Self {
        Self::new(
            surface.latitude,
            surface.longitude,
            surface.distance + kilometers!(6378),
        )
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use absolute_unit::{meters, radians};

    #[test]
    fn test_position() {
        let p = Graticule::<GeoSurface>::new(radians!(0), radians!(0), meters!(0));
        println!("geo^ : {}", p);

        let c = Graticule::<GeoCenter>::from(p);
        println!("geo. : {}", c);
    }
}
