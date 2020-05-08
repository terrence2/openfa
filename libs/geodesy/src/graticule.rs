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
use crate::{Cartesian, GeoCenter, GeoSurface};
use absolute_unit::{
    degrees, kilometers, meters, radians, Angle, AngleUnit, Length, LengthUnit, Meters, Radians,
};
use num_traits::Float;
use physical_constants::EARTH_RADIUS_KM;
use std::{convert::From, fmt, marker::PhantomData};

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
    pub fn new<UnitAng: AngleUnit, UnitLen: LengthUnit>(
        latitude: Angle<UnitAng>,
        longitude: Angle<UnitAng>,
        distance: Length<UnitLen>,
    ) -> Self {
        Self {
            latitude: radians!(latitude),
            longitude: radians!(longitude),
            distance: meters!(distance),
            phantom: PhantomData,
        }
    }

    pub fn lat_lon<UnitAng: AngleUnit, T: Float>(&self) -> [T; 2] {
        [
            T::from(f64::from(Angle::<UnitAng>::from(&self.latitude))).unwrap(),
            T::from(f64::from(Angle::<UnitAng>::from(&self.longitude))).unwrap(),
        ]
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

impl From<Graticule<GeoSurface>> for Graticule<GeoCenter> {
    fn from(surface: Graticule<GeoSurface>) -> Self {
        Self::new(
            surface.latitude,
            surface.longitude,
            surface.distance + kilometers!(EARTH_RADIUS_KM),
        )
    }
}

impl From<Graticule<GeoCenter>> for Graticule<GeoSurface> {
    fn from(surface: Graticule<GeoCenter>) -> Self {
        Self::new(
            surface.latitude,
            surface.longitude,
            surface.distance - kilometers!(6378),
        )
    }
}

impl<Unit: LengthUnit> From<Cartesian<GeoCenter, Unit>> for Graticule<GeoCenter> {
    fn from(xyz: Cartesian<GeoCenter, Unit>) -> Self {
        let x = f64::from(xyz.coords[0]);
        let y = f64::from(xyz.coords[1]);
        let z = f64::from(xyz.coords[2]);
        let distance = (x * x + y * y + z * z).sqrt();
        let lon = (-x).atan2(z);
        let lat = (y / distance).asin();
        Self::new(radians!(lat), radians!(lon), meters!(distance))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use absolute_unit::{meters, radians};
    use approx::abs_diff_eq;

    #[test]
    fn test_position() {
        let p = Graticule::<GeoSurface>::new(radians!(0), radians!(0), meters!(0));
        println!("geo^ : {}", p);

        let c = Graticule::<GeoCenter>::from(p);
        println!("geo. : {}", c);
    }

    fn roundtrip(lat: i64, lon: i64) -> bool {
        let g0 = Graticule::<GeoCenter>::new(degrees!(lat), degrees!(lon), meters!(100));
        let c = Cartesian::<GeoCenter, Meters>::from(g0);
        let g1 = Graticule::<GeoCenter>::from(c);
        println!("g0: {:?}", g0);
        println!("g1: {:?}", g1);
        abs_diff_eq!(f64::from(g0.latitude), f64::from(g1.latitude))
            && abs_diff_eq!(f64::from(g0.longitude), f64::from(g1.longitude))
            && abs_diff_eq!(f64::from(g0.distance), f64::from(g1.distance))
    }

    #[test]
    fn test_roundtrip() {
        // Note: at -90 latitude, any longitude is correct.
        for lat in -89..89 {
            for lon in -180..180 {
                assert!(roundtrip(lat, lon));
            }
        }
    }
}
