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
use serde::{Deserialize, Serialize};

/// A record suitable for use in CSV processing
#[derive(Debug, Serialize, Deserialize)]
pub struct Record {
    // Instruction Number - Just a counting index; should match the row number.
    pub instr_number: usize,

    // CODE Offset - Byte offset within the CODE section of the PE (what we have when loading)
    pub code_offset: usize,

    // File Offset - CODE Offset + the offset of the code section in the PE
    pub file_offset: usize,

    // Size - Size of the instruction in bytes
    pub instr_size: usize,

    // Magic - Generally the 2-byte identifier, but sometimes a memonic? Needs a rewrite.
    pub magic: String,

    // Raw Content - Raw content of the instruction (including prefix), formatted as hex bytes.
    pub raw_content: String,

    // Comment - Our interpretation of the instruction content (varies)
    pub comment: String,
}
