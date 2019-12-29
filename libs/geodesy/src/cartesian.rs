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
use std::{fmt, marker::PhantomData, ops::Add};

pub trait CartesianOrigin {
    fn origin_name() -> &'static str;
}

#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
pub struct Cartesian<Origin>
where
    Origin: CartesianOrigin,
{
    coords: [Length<Meters>; 3],
    phantom: PhantomData<Origin>,
}

impl<Origin> Cartesian<Origin>
where
    Origin: CartesianOrigin,
{
    pub fn new<Unit: LengthUnit>(x: Length<Unit>, y: Length<Unit>, z: Length<Unit>) -> Self {
        Self {
            coords: [meters!(x), meters!(y), meters!(z)],
            phantom: PhantomData,
        }
    }
}

impl<Origin> fmt::Display for Cartesian<Origin>
where
    Origin: CartesianOrigin,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "[{}, {}, {}]{}",
            self.coords[0],
            self.coords[1],
            self.coords[2],
            Origin::origin_name(),
        )
    }
}

use crate::{GeoCenter, Graticule, Target};
impl From<Graticule<GeoCenter>> for Cartesian<GeoCenter> {
    fn from(graticule: Graticule<GeoCenter>) -> Self {
        let lat = f64::from(graticule.latitude);
        let lon = f64::from(graticule.longitude);
        Self {
            coords: [
                graticule.distance * lat.cos() * lon.sin(),
                graticule.distance * -lat.sin(),
                graticule.distance * lat.cos() * lon.cos(),
            ],
            phantom: PhantomData,
        }
    }
}

impl From<Graticule<Target>> for Cartesian<Target> {
    fn from(graticule: Graticule<Target>) -> Self {
        let lat = f64::from(graticule.latitude);
        let lon = f64::from(graticule.longitude);
        Self {
            coords: [
                graticule.distance * lat.cos() * lon.sin(),
                graticule.distance * -lat.sin(),
                graticule.distance * lat.cos() * lon.cos(),
            ],
            phantom: PhantomData,
        }
    }
}

impl Add<Cartesian<Target>> for Cartesian<GeoCenter> {
    type Output = Cartesian<GeoCenter>;

    fn add(self, other: Cartesian<Target>) -> Self {
        Self {
            coords: [
                self.coords[0] + other.coords[0],
                self.coords[1] + other.coords[1],
                self.coords[2] + other.coords[2],
            ],
            phantom: PhantomData,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::GeoCenter;
    use absolute_unit::{meters, radians};

    #[test]
    fn test_position() {
        let c = Cartesian::<GeoCenter>::new(meters!(0), meters!(0), meters!(0));
        println!("c: {}", c);
    }
}
