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

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum PatchWinding {
    Full,
    Missing0,
    Missing2,
    Missing20,
}

impl PatchWinding {
    pub(crate) fn all_windings() -> [Self; 4] {
        [Self::Full, Self::Missing0, Self::Missing2, Self::Missing20]
    }

    pub(crate) fn from_peers(peers: &[Option<Peer>; 3]) -> Self {
        assert!(peers[1].is_some());
        match (peers[0].is_some(), peers[2].is_some()) {
            (true, true) => Self::Full,
            (false, true) => Self::Missing0,
            (true, false) => Self::Missing2,
            (false, false) => Self::Missing20,
        }
    }

    pub(crate) fn index(&self) -> usize {
        match self {
            Self::Full => 0,
            Self::Missing0 => 1,
            Self::Missing2 => 2,
            Self::Missing20 => 3,
        }
    }
}
