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
use anyhow::Result;
use csv::WriterBuilder;
use reverse::bs2s;
use sh::{RawShape, Record};
use std::path::PathBuf;

pub fn export_csv(sh: &RawShape, output_filename: &str) -> Result<()> {
    let path = PathBuf::from(output_filename);

    let code_offset = sh.pe.section_info["CODE"].file_offset() as usize;
    let mut records = Vec::with_capacity(sh.instrs.len());
    for (i, instr) in sh.instrs.iter().enumerate() {
        let content = &sh.pe.code[instr.at_offset()..instr.at_offset() + instr.size()];
        records.push(Record {
            instr_number: i,
            code_offset: instr.at_offset(),
            file_offset: instr.at_offset() + code_offset,
            instr_size: instr.size(),
            magic: instr.magic().to_owned(),
            raw_content: bs2s(content),
            comment: format!("{:?}", instr),
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
