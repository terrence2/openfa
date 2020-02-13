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
use crate::{length::LengthUnit, unit::kilometers::Kilometers};

#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
pub struct EarthRadii;
impl LengthUnit for EarthRadii {
    fn unit_name() -> &'static str {
        "earth-radii"
    }
    fn suffix() -> &'static str {
        "earths"
    }
    fn nanometers_in_unit() -> i64 {
        Kilometers::nanometers_in_unit() * 6_378
    }
}

#[macro_export]
macro_rules! earth_radii {
    ($num:expr) => {
        $crate::Length::<$crate::EarthRadii>::from(&$num)
    };
}
