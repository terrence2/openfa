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
mod code;
mod geometry;
mod jump;
mod marker;
mod mask;
mod meta;

use failure::{Fail, Fallible};
use std::str;

pub use crate::instr::{
    code::{X86Code, X86Message, X86Trampoline},
    geometry::{Facet, FacetFlags, TextureIndex, TextureRef, VertexBuf, VertexNormal},
    jump::{Jump, JumpToDamage, JumpToDetail, JumpToFrame, JumpToLOD},
    marker::PtrToObjEnd,
    mask::{Unmask, Unmask4, XformUnmask, XformUnmask4},
    meta::{EndOfObject, EndOfShape, Pad1E, SourceRef},
};

#[derive(Debug, Fail)]
pub enum ShError {
    #[fail(display = "name ran off end of file")]
    NameUnending {},
}

pub fn read_name(n: &[u8]) -> Fallible<String> {
    let end_offset: usize = n
        .iter()
        .position(|&c| c == 0)
        .ok_or::<ShError>(ShError::NameUnending {})?;
    Ok(str::from_utf8(&n[..end_offset])?.to_owned())
}
