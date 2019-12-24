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
use crate::impl_unit_for_numerics;
use std::{fmt, marker::PhantomData};

pub trait LengthUnit: Copy {
    fn unit_name() -> &'static str;
    fn suffix() -> &'static str;
    fn nanometers_in_unit() -> i64;
}

#[derive(Debug, Clone, Copy)]
pub struct Length<T: LengthUnit> {
    nm: i64, // in nanometers
    phantom: PhantomData<T>,
}

impl<Unit> fmt::Display for Length<Unit>
where
    Unit: LengthUnit,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let v = self.nm as f64 / Unit::nanometers_in_unit() as f64;
        write!(f, "{:0.4}{}", v, Unit::suffix())
    }
}

impl<'a, UnitA, UnitB> From<&'a Length<UnitA>> for Length<UnitB>
where
    UnitA: LengthUnit,
    UnitB: LengthUnit,
{
    fn from(v: &'a Length<UnitA>) -> Self {
        Self {
            nm: v.nm,
            phantom: PhantomData,
        }
    }
}

macro_rules! impl_length_unit_for_numeric_type {
    ($Num:ty) => {
        impl<Unit> From<$Num> for Length<Unit>
        where
            Unit: LengthUnit,
        {
            fn from(v: $Num) -> Self {
                Self {
                    nm: (v as f64 * Unit::nanometers_in_unit() as f64) as i64,
                    phantom: PhantomData,
                }
            }
        }

        impl<Unit> From<&$Num> for Length<Unit>
        where
            Unit: LengthUnit,
        {
            fn from(v: &$Num) -> Self {
                Self {
                    nm: (*v as f64 * Unit::nanometers_in_unit() as f64) as i64,
                    phantom: PhantomData,
                }
            }
        }

        impl<Unit> From<Length<Unit>> for $Num
        where
            Unit: LengthUnit,
        {
            fn from(v: Length<Unit>) -> $Num {
                (v.nm as f64 / Unit::nanometers_in_unit() as f64) as $Num
            }
        }
    };
}
impl_unit_for_numerics!(impl_length_unit_for_numeric_type);

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        feet, meters,
        unit::{feet::Feet, meters::Meters},
    };
    use std::f64::consts::PI;

    #[test]
    fn test_meters_to_feet() {
        let m = meters!(1);
        println!("m : {}", m);
        println!("ft: {}", feet!(m));
    }
}
