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
use crate::angle::AngleUnit;

#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
pub struct Radians;
impl AngleUnit for Radians {
    fn unit_name() -> &'static str {
        "radians"
    }
    fn suffix() -> &'static str {
        " ㎭"
    }
    fn femto_radians_in_unit() -> i64 {
        1_000_000_000_000_000 // peta = 10**15
    }
}

#[macro_export]
macro_rules! radians {
    ($num:expr) => {
        $crate::Angle::<$crate::Radians>::from(&$num)
    };
}
