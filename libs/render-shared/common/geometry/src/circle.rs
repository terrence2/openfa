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
use crate::Plane;
use nalgebra::RealField;

#[derive(Debug)]
pub struct Circle<T: RealField> {
    plane: Plane<T>,
    radius: T,
}

impl<T: RealField> Circle<T> {
    pub fn from_plane_and_radius(plane: &Plane<T>, radius: T) -> Self {
        Self {
            plane: *plane,
            radius,
        }
    }
}
