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
use absolute_unit::Kilometers;
use geodesy::{Cartesian, GeoCenter, GeoSurface, Graticule};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform(Graticule<GeoSurface>);

impl Transform {
    pub fn new(v: Graticule<GeoSurface>) -> Self {
        Self(v)
    }

    pub fn cartesian(&self) -> Cartesian<GeoCenter, Kilometers> {
        Cartesian::<GeoCenter, Kilometers>::from(Graticule::<GeoCenter>::from(self.0))
    }

    pub fn graticule(&self) -> &Graticule<GeoSurface> {
        &self.0
    }
}
