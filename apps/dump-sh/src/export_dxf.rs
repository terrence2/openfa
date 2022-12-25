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
use dxf::{
    entities::{Entity, EntityType, Vertex},
    enums::AcadVersion,
    tables::Layer,
    Drawing, Point,
};
use sh::RawShape;

pub fn export_dxf(sh: &RawShape, output_filename: &str) -> Result<()> {
    let mut drawing = Drawing::new();
    drawing.header.version = AcadVersion::R2010; // for identifier

    let mut vxbuf_num = 0;
    for instr in sh.instrs.iter() {
        if let sh::Instr::VertexBuf(buf) = instr {
            let name = format!("vxbuf-{}", vxbuf_num);
            let layer = Layer {
                name: name.clone(),
                ..Default::default()
            };
            drawing.add_layer(layer);

            // Bytecode is 2 bytes, position is 4 bytes, so 6 off from instruction start
            let base_offset = buf.at_offset() + 6;
            assert!(base_offset < 0xFF_FFFF);
            let base_offset = base_offset as i32;
            for (i, v) in buf.verts.iter().enumerate() {
                // 2 bytes for 3 positions per vert
                let vert_offset = base_offset + (i as i32) * 6;
                let mut vert = Vertex::new(Point::new(v[0] as f64, v[1] as f64, v[2] as f64));
                vert.identifier = vert_offset;
                let mut ent = Entity::new(EntityType::Vertex(vert));
                ent.common.layer = name.clone();
                println!("Vertex: {} on {}", vert_offset, ent.common.layer);
                let _ref = drawing.add_entity(ent);
            }
            vxbuf_num += 1;
        }
    }
    drawing.save_file(output_filename)?;
    Ok(())
}
