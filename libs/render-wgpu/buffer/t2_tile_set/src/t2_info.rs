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
use std::mem;
use zerocopy::{AsBytes, FromBytes};

// We allocate a block of these GPU side to tell us what projection to use for each tile.
#[repr(C)]
#[derive(AsBytes, FromBytes, Copy, Clone, Default, Debug)]
pub struct T2Info {
    tile_base_rad: [f32; 2],
    tile_extent_rad: [f32; 2],
}

impl T2Info {
    pub fn new(tile_base_rad: [f32; 2], tile_extent_rad: [f32; 2]) -> Self {
        Self {
            tile_base_rad,
            tile_extent_rad,
        }
    }

    pub fn mem_size() -> u64 {
        mem::size_of::<Self>() as u64
    }
}
