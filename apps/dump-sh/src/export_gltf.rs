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
use anyhow::{Context, Result};
use gltf::{json, json::validation::Checked::Valid};
use sh::{RawShape, VertexBuf};
use std::{fs, mem, path::PathBuf};
use zerocopy::{AsBytes, FromBytes};

pub fn export_gltf(sh: &RawShape, output_filename: &str) -> Result<()> {
    let path = PathBuf::from(output_filename);
    let name = path.file_name().unwrap().to_str().unwrap();
    fs::create_dir_all(path.clone())?;

    let mut root = json::Root {
        accessors: vec![],    // positions, colors
        buffers: vec![],      // buffer
        buffer_views: vec![], // buffer_view
        meshes: vec![],       // mesh
        nodes: vec![],        // node
        scenes: vec![json::Scene {
            extensions: Default::default(),
            extras: Default::default(),
            name: None,
            nodes: vec![], //json::Index::new(0)
        }],
        ..Default::default()
    };

    let mut vxbuf_num = 0;
    for instr in sh.instrs.iter() {
        if let sh::Instr::VertexBuf(buf) = instr {
            let vxbuf_name = format!("vxbuf-{}-{:08X}", vxbuf_num, buf.at_offset());

            let (accessor, buffer, buffer_view, mesh, node, data) =
                export_mesh(&vxbuf_name, vxbuf_num, buf);
            root.accessors.push(accessor);
            root.buffers.push(buffer);
            root.buffer_views.push(buffer_view);
            root.meshes.push(mesh);
            root.nodes.push(node);
            root.scenes[0].nodes.push(json::Index::new(vxbuf_num));

            let data = data.as_bytes();
            assert_eq!(data.len() % 4, 0, "buffer must be 4-byte aligned");
            let data_out = format!("{}/{}.bin", path.to_string_lossy(), vxbuf_name);
            fs::write(data_out, data)?;

            vxbuf_num += 1;
        }
    }

    let gltf_out = format!("{}/{}.gltf", path.to_string_lossy(), name);
    let writer = fs::File::create(&gltf_out).with_context(|| format!("i/o error in {gltf_out}"))?;
    json::serialize::to_writer_pretty(writer, &root).expect("Serialization error");

    Ok(())
}

#[repr(C)]
#[derive(AsBytes, FromBytes, Copy, Clone, Debug)]
struct Vertex {
    position: [f32; 3],
}

impl Vertex {
    pub fn from_vxbuf(buf: &VertexBuf) -> Vec<Self> {
        buf.verts
            .iter()
            .map(|v| Vertex {
                position: [v[0] as f32, v[1] as f32, v[2] as f32],
            })
            .collect()
    }
}

fn bounding_coords(points: &[Vertex]) -> ([f32; 3], [f32; 3]) {
    let mut min = [f32::MAX, f32::MAX, f32::MAX];
    let mut max = [f32::MIN, f32::MIN, f32::MIN];

    for point in points {
        let p = point.position;
        for i in 0..3 {
            min[i] = f32::min(min[i], p[i]);
            max[i] = f32::max(max[i], p[i]);
        }
    }
    (min, max)
}

fn export_mesh(
    name: &str,
    offset: u32,
    buf: &VertexBuf,
) -> (
    json::Accessor,
    json::Buffer,
    json::buffer::View,
    json::Mesh,
    json::Node,
    Vec<Vertex>,
) {
    let points = Vertex::from_vxbuf(buf);
    println!("At {}, exporting {} verts", buf.at_offset(), points.len());
    let (min, max) = bounding_coords(&points);

    let buffer_length = (points.len() * mem::size_of::<Vertex>()) as u32;
    let buffer = json::Buffer {
        byte_length: buffer_length,
        extensions: Default::default(),
        extras: Default::default(),
        name: Some(name.to_owned()),
        uri: Some(format!("{}.bin", name)),
    };
    let buffer_view = json::buffer::View {
        buffer: json::Index::new(offset),
        byte_length: buffer.byte_length,
        byte_offset: None,
        byte_stride: Some(mem::size_of::<Vertex>() as u32),
        extensions: Default::default(),
        extras: Default::default(),
        name: None,
        target: Some(Valid(json::buffer::Target::ArrayBuffer)),
    };
    let positions = json::Accessor {
        buffer_view: Some(json::Index::new(offset)),
        byte_offset: 0,
        count: points.len() as u32,
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
    let primitive = json::mesh::Primitive {
        attributes: {
            let mut map = std::collections::HashMap::new();
            map.insert(
                Valid(json::mesh::Semantic::Positions),
                json::Index::new(offset),
            );
            map
        },
        extensions: Default::default(),
        extras: Default::default(),
        indices: None,
        material: None,
        mode: Valid(json::mesh::Mode::Points),
        targets: None,
    };
    let mesh = json::Mesh {
        extensions: Default::default(),
        extras: Default::default(),
        name: Some(name.to_owned()),
        primitives: vec![primitive],
        weights: None,
    };
    let node = json::Node {
        camera: None,
        children: None,
        extensions: Default::default(),
        extras: Default::default(),
        matrix: None,
        mesh: Some(json::Index::new(offset)),
        name: None,
        rotation: None,
        scale: None,
        translation: None,
        skin: None,
        weights: None,
    };

    // Note: needs to be 4-byte aligned

    (positions, buffer, buffer_view, mesh, node, points)
}
