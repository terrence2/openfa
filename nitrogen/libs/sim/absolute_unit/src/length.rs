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
use crate::impl_unit_for_numerics;
use approx::AbsDiffEq;
use std::{
    fmt,
    marker::PhantomData,
    ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign},
};

pub trait LengthUnit: Copy {
    fn unit_name() -> &'static str;
    fn suffix() -> &'static str;
    fn nanometers_in_unit() -> i64;
}

#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
pub struct Length<Unit: LengthUnit> {
    nm: i64, // in nanometers
    phantom: PhantomData<Unit>,
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

impl<UnitA, UnitB> Add<Length<UnitA>> for Length<UnitB>
where
    UnitA: LengthUnit,
    UnitB: LengthUnit,
{
    type Output = Length<UnitB>;

    fn add(self, other: Length<UnitA>) -> Self {
        Self {
            nm: self.nm + other.nm,
            phantom: PhantomData,
        }
    }
}

impl<UnitA, UnitB> AddAssign<Length<UnitA>> for Length<UnitB>
where
    UnitA: LengthUnit,
    UnitB: LengthUnit,
{
    fn add_assign(&mut self, other: Length<UnitA>) {
        self.nm += other.nm;
    }
}

impl<UnitA, UnitB> Sub<Length<UnitA>> for Length<UnitB>
where
    UnitA: LengthUnit,
    UnitB: LengthUnit,
{
    type Output = Length<UnitB>;

    fn sub(self, other: Length<UnitA>) -> Self {
        Self {
            nm: self.nm - other.nm,
            phantom: PhantomData,
        }
    }
}

impl<UnitA, UnitB> SubAssign<Length<UnitA>> for Length<UnitB>
where
    UnitA: LengthUnit,
    UnitB: LengthUnit,
{
    fn sub_assign(&mut self, other: Length<UnitA>) {
        self.nm -= other.nm;
    }
}

// When cartesian coordinates arrive as a result of Geodic calculations, we
// expect some slop. This lets us account for that easily in tests.
impl<Unit> AbsDiffEq for Length<Unit>
where
    Unit: LengthUnit + PartialEq,
{
    type Epsilon = i64;

    fn default_epsilon() -> Self::Epsilon {
        // 360nm was max error at earth surface when converting to cartesian in units of km.
        400i64
    }

    fn abs_diff_eq(&self, other: &Length<Unit>, epsilon: Self::Epsilon) -> bool {
        i64::abs(self.nm - other.nm) <= epsilon
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

        impl<Unit> Mul<$Num> for Length<Unit>
        where
            Unit: LengthUnit,
        {
            type Output = Length<Unit>;

            fn mul(self, other: $Num) -> Self {
                Self {
                    nm: (self.nm as f64 * other as f64) as i64,
                    phantom: PhantomData,
                }
            }
        }

        impl<Unit> MulAssign<$Num> for Length<Unit>
        where
            Unit: LengthUnit,
        {
            fn mul_assign(&mut self, other: $Num) {
                self.nm = (self.nm as f64 * other as f64) as i64;
            }
        }

        impl<Unit> Div<$Num> for Length<Unit>
        where
            Unit: LengthUnit,
        {
            type Output = Length<Unit>;

            fn div(self, other: $Num) -> Self {
                Self {
                    nm: (self.nm as f64 / other as f64) as i64,
                    phantom: PhantomData,
                }
            }
        }

        impl<Unit> DivAssign<$Num> for Length<Unit>
        where
            Unit: LengthUnit,
        {
            fn div_assign(&mut self, other: $Num) {
                self.nm = (self.nm as f64 / other as f64) as i64;
            }
        }
    };
}
impl_unit_for_numerics!(impl_length_unit_for_numeric_type);

#[cfg(test)]
mod test {
    use crate::{feet, meters};

    #[test]
    fn test_meters_to_feet() {
        let m = meters!(1);
        println!("m : {}", m);
        println!("ft: {}", feet!(m));
    }
}
