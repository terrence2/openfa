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
use crate::patch_tree::Peer;

pub enum PatchWinding {
    Full,
    Missing0,
    Missing1,
    Missing2,
    Missing01,
    Missing12,
    Missing20,
    Empty,
}

impl PatchWinding {
    pub(crate) fn from_peers(peers: &[Option<Peer>; 3]) -> Self {
        match (peers[0].is_some(), peers[1].is_some(), peers[2].is_some()) {
            (true, true, true) => Self::Full,
            (false, true, true) => Self::Missing0,
            (true, false, true) => Self::Missing1,
            (true, true, false) => Self::Missing2,
            (false, false, true) => Self::Missing01,
            (true, false, false) => Self::Missing12,
            (false, true, false) => Self::Missing20,
            (false, false, false) => Self::Empty,
        }
    }
}
