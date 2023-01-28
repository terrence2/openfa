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
use crate::{Instr, RawShape};
use anyhow::Result;
use csv::WriterBuilder;
use reverse::bs2s;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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

    // Processed Content - On export, raw_content for certain instructions is processed into
    //                     parts for easier editing and on import parsed back into raw_content
    //                     before writing.
    pub processed_content: String,

    // Comment - Our interpretation of the instruction content (varies)
    pub comment: String,
}

pub fn export_csv(sh: &RawShape, output_filename: &str) -> Result<()> {
    let path = PathBuf::from(output_filename);

    let code_offset = sh.pe.code_section().expect("code section").file_offset() as usize;
    let mut records = Vec::with_capacity(sh.instrs.len());
    for (i, instr) in sh.instrs.iter().enumerate() {
        let raw_content = bs2s(&sh.pe.code[instr.at_offset()..instr.at_offset() + instr.size()]);
        let processed_content = match instr {
            Instr::VertexBuf(vxbuf) => vxbuf
                .verts
                .iter()
                .map(|[a, b, c]| format!("{a} {b} {c}"))
                .collect::<Vec<_>>()
                .join("\n"),
            Instr::Facet(facet) => [
                format!("color: {:X}", facet.color),
                format!(
                    "tex_coords: {}",
                    facet
                        .tex_coords
                        .iter()
                        .map(|[s, t]| format!("{s} {t}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ]
            .to_vec()
            .join("\n"),
            _ => "".to_owned(),
        };
        records.push(Record {
            instr_number: i,
            code_offset: instr.at_offset(),
            file_offset: instr.at_offset() + code_offset,
            instr_size: instr.size(),
            magic: instr.magic().to_owned(),
            raw_content,
            processed_content,
            comment: format!("{instr:?}"),
        });
    }

    let mut writer = WriterBuilder::new()
        .has_headers(true)
        .flexible(false)
        .from_path(path)?;
    for record in records {
        writer.serialize(record)?;
    }

    Ok(())
}
