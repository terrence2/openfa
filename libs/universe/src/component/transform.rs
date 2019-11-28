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
use nalgebra::Vector3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform(Vector3<f32>);

impl Transform {
    pub fn new(v: Vector3<f32>) -> Self {
        Self(v)
    }

    // Convert to dense pack for upload.
    pub fn compact(&self) -> [f32; 3] {
        [self.0.x, self.0.y, self.0.z]
    }
}
