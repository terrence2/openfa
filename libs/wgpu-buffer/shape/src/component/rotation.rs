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
use nalgebra::UnitQuaternion;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rotation(UnitQuaternion<f32>);

impl Rotation {
    pub fn new(q: UnitQuaternion<f32>) -> Self {
        Self(q)
    }

    pub fn quaternion(&self) -> &UnitQuaternion<f32> {
        &self.0
    }

    // Convert to dense pack for upload.
    pub fn compact(&self) -> [f32; 3] {
        let (a, b, c) = self.0.euler_angles();
        [a, b, c]
    }
}
