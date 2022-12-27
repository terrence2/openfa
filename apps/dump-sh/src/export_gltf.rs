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
use gltf_json as json;
use sh::{RawShape, VertexBuf};
use std::mem;

pub fn export_gltf(sh: &RawShape, output_filename: &str) -> Result<()> {
    let mut vxbuf_num = 0;
    for instr in sh.instrs.iter() {
        if let sh::Instr::VertexBuf(buf) = instr {
            let name = format!("vxbuf-{}", vxbuf_num);
            let foo = export_mesh(&name, buf);

            // Bytecode is 2 bytes, position is 4 bytes, so 6 off from instruction start
            let base_offset = buf.at_offset() + 6;
            assert!(base_offset < 0xFF_FFFF);
            let base_offset = base_offset as i32;
            for (i, v) in buf.verts.iter().enumerate() {
                /*
                // 2 bytes for 3 positions per vert
                let vert_offset = base_offset + (i as i32) * 6;
                let mut vert = Vertex::new(Point::new(v[0] as f64, v[1] as f64, v[2] as f64));
                vert.identifier = vert_offset;
                let mut ent = Entity::new(EntityType::Vertex(vert));
                ent.common.layer = name.clone();
                println!("Vertex: {} on {}", vert_offset, ent.common.layer);
                let _ref = drawing.add_entity(ent);
                 */
            }
            vxbuf_num += 1;
        }
    }

    Ok(())
}

fn export_mesh(name: &str, buf: &VertexBuf) {
    // let (min, max) = bounding_coords(&triangle_vertices);

    let buffer_length = (buf.verts.len() * mem::size_of::<f32>() * 3) as u32;
    let buffer = json::Buffer {
        byte_length: buffer_length,
        extensions: Default::default(),
        extras: Default::default(),
        name: None,
        uri: Some(format!("{}.bin", name)),
    };
    /*
    let buffer_view = json::buffer::View {
        buffer: json::Index::new(0),
        byte_length: buffer.byte_length,
        byte_offset: None,
        byte_stride: Some(mem::size_of::<Vertex>() as u32),
        extensions: Default::default(),
        extras: Default::default(),
        name: None,
        target: Some(Valid(json::buffer::Target::ArrayBuffer)),
    };
    let positions = json::Accessor {
        buffer_view: Some(json::Index::new(0)),
        byte_offset: 0,
        count: triangle_vertices.len() as u32,
        component_type: Valid(json::accessor::GenericComponentType(
            json::accessor::ComponentType::F32,
        )),
        extensions: Default::default(),
        extras: Default::default(),
        type_: Valid(json::accessor::Type::Vec3),
        min: Some(json::Value::from(Vec::from(min))),
        max: Some(json::Value::from(Vec::from(max))),
        name: None,
        normalized: false,
        sparse: None,
    };
    let colors = json::Accessor {
        buffer_view: Some(json::Index::new(0)),
        byte_offset: (3 * mem::size_of::<f32>()) as u32,
        count: triangle_vertices.len() as u32,
        component_type: Valid(json::accessor::GenericComponentType(
            json::accessor::ComponentType::F32,
        )),
        extensions: Default::default(),
        extras: Default::default(),
        type_: Valid(json::accessor::Type::Vec3),
        min: None,
        max: None,
        name: None,
        normalized: false,
        sparse: None,
    };

    let primitive = json::mesh::Primitive {
        attributes: {
            let mut map = std::collections::HashMap::new();
            map.insert(Valid(json::mesh::Semantic::Positions), json::Index::new(0));
            map.insert(Valid(json::mesh::Semantic::Colors(0)), json::Index::new(1));
            map
        },
        extensions: Default::default(),
        extras: Default::default(),
        indices: None,
        material: None,
        mode: Valid(json::mesh::Mode::Triangles),
        targets: None,
    };

    let mesh = json::Mesh {
        extensions: Default::default(),
        extras: Default::default(),
        name: None,
        primitives: vec![primitive],
        weights: None,
    };

    let node = json::Node {
        camera: None,
        children: None,
        extensions: Default::default(),
        extras: Default::default(),
        matrix: None,
        mesh: Some(json::Index::new(0)),
        name: None,
        rotation: None,
        scale: None,
        translation: None,
        skin: None,
        weights: None,
    };
     */
}
