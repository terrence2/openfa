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
use absolute_unit::{EarthRadii, Kilometers, Meters};
use failure::Fallible;
use geodesy::{Cartesian, GeoCenter, GeoSurface, Graticule};
use geometry::IcoSphere;
use memoffset::offset_of;
use nalgebra::{Unit, UnitQuaternion, Vector3};
use std::{cell::RefCell, f64::consts::PI, mem, ops::Range, sync::Arc};
use wgpu;
use zerocopy::{AsBytes, FromBytes};

#[repr(C)]
#[derive(Debug, AsBytes, FromBytes, Copy, Clone, Default)]
pub struct Vertex {
    position: [f32; 4],
    color: [f32; 4],
}

impl Vertex {
    #[allow(clippy::unneeded_field_pattern)]
    pub fn descriptor() -> wgpu::VertexBufferDescriptor<'static> {
        let tmp = wgpu::VertexBufferDescriptor {
            stride: mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::InputStepMode::Vertex,
            attributes: &[
                // position
                wgpu::VertexAttributeDescriptor {
                    format: wgpu::VertexFormat::Float4,
                    offset: 0,
                    shader_location: 0,
                },
                wgpu::VertexAttributeDescriptor {
                    format: wgpu::VertexFormat::Float4,
                    offset: 16,
                    shader_location: 1,
                },
            ],
        };

        assert_eq!(
            tmp.attributes[0].offset,
            offset_of!(Vertex, position) as wgpu::BufferAddress
        );
        assert_eq!(
            tmp.attributes[1].offset,
            offset_of!(Vertex, color) as wgpu::BufferAddress
        );

        assert_eq!(mem::size_of::<Vertex>(), 32);

        tmp
    }

    pub fn new(position: [f32; 4], color: [f32; 4]) -> Self {
        Self { position, color }
    }
}

#[repr(C)]
#[derive(Debug, AsBytes, FromBytes, Copy, Clone, Default)]
struct TileData {
    rotation_and_scale: [f32; 4],
}

impl TileData {
    pub fn new(scale: f64) -> Self {
        Self {
            rotation_and_scale: [0f32, 0f32, 0f32, scale as f32],
        }
    }
}

pub struct TerrainGeoBuffer {
    base_vertex_buffer: wgpu::Buffer,
    base_index_buffer: wgpu::Buffer,
    index_count: u32,
    tile_buffer: wgpu::Buffer,
    tile_count: u32,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
}

const EARTH_TO_KM: f64 = 6378.0;

fn vec2a(v: Vector3<f64>) -> [f32; 4] {
    [v[0] as f32, v[1] as f32, v[2] as f32, 1f32]
}

impl TerrainGeoBuffer {
    fn bisect_edge(v0: &Vector3<f64>, v1: &Vector3<f64>) -> Vector3<f64> {
        v0 + ((v1 - v0) / 2f64)
    }

    pub fn build_vb_for_face(
        v0: &Vector3<f64>,
        v1: &Vector3<f64>,
        v2: &Vector3<f64>,
        device: &wgpu::Device,
    ) -> Fallible<(f64, wgpu::Buffer, wgpu::Buffer, u32)> {
        // Orient the tile to (0,0).
        let origin_surface =
            Graticule::<GeoCenter>::from(Cartesian::<GeoCenter, Kilometers>::from(
                ((*v0 + *v1 + *v2) / 3f64).normalize() * EARTH_TO_KM,
            ));
        let q_lon = UnitQuaternion::from_axis_angle(
            &Unit::new_unchecked(Vector3::new(0f64, 1f64, 0f64)),
            f64::from(origin_surface.longitude),
        );
        let q_lat = UnitQuaternion::from_axis_angle(
            &Unit::new_unchecked(Vector3::new(1f64, 0f64, 0f64)),
            f64::from(origin_surface.latitude),
        );
        let v0 = q_lat * (q_lon * *v0);
        let v1 = q_lat * (q_lon * *v1);
        let v2 = q_lat * (q_lon * *v2);
        let origin = (v0 + v1 + v2) / 3f64;
        //let d = Vector3::new(v0.x - origin.x, v0.y - origin.y, 0f64).normalize();
        let d = (v0 - origin).normalize();
        let facing = d.y.atan2(d.x);

        let a = Self::bisect_edge(&v0, &v1); //.normalize();
        let b = Self::bisect_edge(&v1, &v2); //.normalize();
        let c = Self::bisect_edge(&v2, &v0); //.normalize();
        let red = [1f32, 0f32, 0f32, 1f32];
        let green = [0f32, 1f32, 0f32, 1f32];
        let blue = [0f32, 0f32, 1f32, 1f32];
        let cyan = [0f32, 1f32, 1f32, 1f32];
        let vertices = vec![
            Vertex::new(vec2a(v0), red),
            Vertex::new(vec2a(v1), green),
            Vertex::new(vec2a(v2), blue),
            Vertex::new(vec2a(a), cyan),
            Vertex::new(vec2a(b), cyan),
            Vertex::new(vec2a(c), cyan),
        ];
        let vertex_buffer = device
            .create_buffer_mapped(vertices.len(), wgpu::BufferUsage::all())
            .fill_from_slice(&vertices);

        // TODO: for faces rather than line lists.
        let (v0i, v1i, v2i, ai, bi, ci) = (0, 1, 2, 3, 4, 5);
        let mut indices = vec![
            v0i, ai, ai, v1i, v1i, bi, bi, v2i, v2i, ci, ci, v0i, ai, bi, bi, ci, ci, ai,
        ];
        let index_buffer = device
            .create_buffer_mapped(indices.len(), wgpu::BufferUsage::all())
            .fill_from_slice(&indices);

        //let center_facing = Self::build_vb_for_face()

        Ok((facing, vertex_buffer, index_buffer, indices.len() as u32))
    }

    fn get_face_orientation(
        lat: f64,
        lon: f64,
        v0: &Vector3<f64>,
        v1: &Vector3<f64>,
        v2: &Vector3<f64>,
    ) -> f64 {
        // Rotate to lat:0, lon:0.
        let q_lon = UnitQuaternion::from_axis_angle(
            &Unit::new_unchecked(Vector3::new(0f64, 1f64, 0f64)),
            f64::from(lon),
        );
        let q_lat = UnitQuaternion::from_axis_angle(
            &Unit::new_unchecked(Vector3::new(1f64, 0f64, 0f64)),
            f64::from(lat),
        );
        let v0 = q_lat * (q_lon * *v0);
        let v1 = q_lat * (q_lon * *v1);
        let v2 = q_lat * (q_lon * *v2);
        let centroid = (v0 + v1 + v2) / 3f64;
        //println!("centroid: {}", centroid);

        //let d = Vector3::new(v0.x - centroid.x, v0.y - centroid.y, 0f64).normalize();
        let d = (v0 - centroid).normalize();

        let facing = d.y.atan2(d.x);

        return facing;
    }

    fn xyz_to_latlon(v: &Vector3<f64>) -> (f64, f64) {
        let x = v[0];
        let y = v[1];
        let z = v[2];
        let distance = (x * x + y * y + z * z).sqrt();
        let lon = (-x).atan2(z);
        let lat = (y / distance).asin();
        (lat, lon)
    }

    pub fn new(device: &wgpu::Device) -> Fallible<Arc<RefCell<Self>>> {
        let sphere = IcoSphere::new(3);

        // l0 buffer
        let base_face = &sphere.faces[0];
        let (base_facing, base_vertex_buffer, base_index_buffer, index_count) =
            Self::build_vb_for_face(
                &sphere.verts[base_face.i0()],
                &sphere.verts[base_face.i1()],
                &sphere.verts[base_face.i2()],
                device,
            )?;

        let mut tiles = Vec::new();
        let sphere = IcoSphere::new(3);
        for face in sphere.faces {
            let v0 = &sphere.verts[face.i0()];
            let v1 = &sphere.verts[face.i1()];
            let v2 = &sphere.verts[face.i2()];
            let target_centroid = (*v0 + *v1 + *v2) / 3f64;
            let mut tile = TileData::new(6378.0);
            let (lat, lon) = Self::xyz_to_latlon(&target_centroid);
            tile.rotation_and_scale[0] = lat as f32;
            tile.rotation_and_scale[1] = lon as f32;

            let facing = Self::get_face_orientation(lat, lon, v0, v1, v2);
            tile.rotation_and_scale[2] = (facing - base_facing) as f32;

            tiles.push(tile);
        }
        let tile_buffer_size = (mem::size_of::<TileData>() * tiles.len()) as wgpu::BufferAddress;
        let tile_count = tiles.len() as u32;
        let tile_buffer = device
            .create_buffer_mapped(tiles.len(), wgpu::BufferUsage::all())
            .fill_from_slice(&tiles);

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            bindings: &[wgpu::BindGroupLayoutBinding {
                binding: 0,
                visibility: wgpu::ShaderStage::VERTEX,
                ty: wgpu::BindingType::UniformBuffer { dynamic: false },
            }],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
            bindings: &[
                // camera and sun
                wgpu::Binding {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer {
                        buffer: &tile_buffer,
                        range: 0..tile_buffer_size,
                    },
                },
            ],
        });

        Ok(Arc::new(RefCell::new(Self {
            base_vertex_buffer,
            base_index_buffer,
            index_count,
            tile_buffer,
            tile_count,
            bind_group_layout,
            bind_group,
        })))
    }

    pub fn index_buffer(&self) -> &wgpu::Buffer {
        &self.base_index_buffer
    }

    pub fn vertex_buffer(&self) -> &wgpu::Buffer {
        &self.base_vertex_buffer
    }

    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    pub fn index_range(&self) -> Range<u32> {
        0..self.index_count
    }

    pub fn instance_range(&self) -> Range<u32> {
        0..self.tile_count
    }
}
