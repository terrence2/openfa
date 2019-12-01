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

// Our shared shader includes expect certain bind groups to be in certain spots.
// Note that these are not unique because we need to stay under 4 and thus re-use heavily.
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum Group {
    Globals,
    GlyphCache,
    Atmosphere,
    TextLayout,
    Stars,
    Terrain,
    ShapeChunk,
    ShapeBlock,
}

impl Group {
    pub fn index(self) -> u32 {
        match self {
            Self::Globals => 0,
            Self::GlyphCache => 0,
            Self::TextLayout => 1,
            Self::Atmosphere => 1,
            Self::Stars => 2,
            Self::Terrain => 2,
            Self::ShapeChunk => 1,
            Self::ShapeBlock => 2,
        }
    }
}
