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

pub enum GlobalSets {
    Global = 0,
    Atmosphere = 1,
    Stars = 2,
    ShapeBuffers = 3,
    ShapeXforms = 4,
    ShapeTextures = 5,
}

impl From<GlobalSets> for usize {
    fn from(gs: GlobalSets) -> usize {
        gs as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let v: usize = GlobalSets::Stars.into();
        assert_eq!(v, 2);
    }
}
