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
use absolute_unit::{degrees, meters, Angle, Degrees, Kilometers, Length, Meters};
use failure::Fallible;
use geodesy::{Cartesian, GeoCenter, GeoSurface, Graticule};
use geometry::IcoSphere;
use memoffset::offset_of;
use nalgebra::Vector3;
use std::{cell::RefCell, mem, ops::Range, sync::Arc};
use wgpu;
use zerocopy::{AsBytes, FromBytes};

#[repr(C)]
#[derive(AsBytes, FromBytes, Copy, Clone, Default)]
pub struct Vertex {
    position: [f32; 4],
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
            ],
        };

        assert_eq!(
            tmp.attributes[0].offset,
            offset_of!(Vertex, position) as wgpu::BufferAddress
        );

        assert_eq!(mem::size_of::<Vertex>(), 16);

        tmp
    }
}

pub struct TerrainGeoBuffer {
    bind_group_layout: wgpu::BindGroupLayout,

    block_vertex_buffer: wgpu::Buffer,
    block_index_buffer: wgpu::Buffer,
    block_index_count: u32,
    block_bind_group: wgpu::BindGroup,
}

const EARTH_TO_KM: f32 = 6370.0;

impl TerrainGeoBuffer {
    pub fn new(device: &wgpu::Device) -> Fallible<Arc<RefCell<Self>>> {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            bindings: &[wgpu::BindGroupLayoutBinding {
                binding: 0,
                visibility: wgpu::ShaderStage::VERTEX,
                ty: wgpu::BindingType::StorageBuffer {
                    dynamic: false,
                    readonly: true,
                },
            }],
        });

        let (block_vertex_buffer, block_index_buffer, block_index_count, block_bind_group) =
            Self::make_clipmap(15, degrees!(1), &bind_group_layout, device)?;

        Ok(Arc::new(RefCell::new(Self {
            bind_group_layout,

            block_vertex_buffer,
            block_index_buffer,
            block_index_count,
            block_bind_group,
        })))
    }

    pub fn make_clipmap(
        size: usize, // `n` in the clipmaps paper
        scale: Angle<Degrees>,
        bind_group_layout: &wgpu::BindGroupLayout,
        device: &wgpu::Device,
    ) -> Fallible<(wgpu::Buffer, wgpu::Buffer, u32, wgpu::BindGroup)> {
        let block_size = (size + 1) / 4; // `m` in the clipmaps paper

        let mut verts = Vec::new();
        for i in 0..block_size {
            let lat = i as f64 * f64::from(scale);
            for j in 0..block_size {
                let lon = j as f64 * f64::from(scale);
                let position =
                    Cartesian::<GeoCenter, Kilometers>::from(Graticule::<GeoCenter>::from(
                        Graticule::<GeoSurface>::new(degrees!(lat), degrees!(lon), meters!(0)),
                    ))
                    .vec64();
                verts.push([
                    position[0] as f32,
                    position[1] as f32,
                    position[2] as f32,
                    1f32,
                ]);
            }
        }
        let vertex_buffer = device
            .create_buffer_mapped(verts.len(), wgpu::BufferUsage::all())
            .fill_from_slice(&verts);

        let mut indices = Vec::new();
        for i0 in 0..block_size - 1 {
            let i1 = i0 + 1;
            for j0 in 0..block_size - 1 {
                let j1 = j0 + 1;
                indices.push((i0 * block_size + j0) as u32);
                indices.push((i1 * block_size + j0) as u32);

                indices.push((i0 * block_size + j0) as u32);
                indices.push((i0 * block_size + j1) as u32);

                indices.push((i0 * block_size + j0) as u32);
                indices.push((i1 * block_size + j1) as u32);

                if i1 == block_size - 1 {
                    indices.push((i1 * block_size + j0) as u32);
                    indices.push((i1 * block_size + j1) as u32);
                }

                if j1 == block_size - 1 {
                    indices.push((i0 * block_size + j1) as u32);
                    indices.push((i1 * block_size + j1) as u32);
                }
            }
        }
        let index_buffer = device
            .create_buffer_mapped(indices.len(), wgpu::BufferUsage::all())
            .fill_from_slice(&indices);

        let buffer_size = (mem::size_of::<[f32; 4]>() * 12) as wgpu::BufferAddress;
        let params = vec![
            [(scale * -7f64).f32(), (scale * 4f64).f32(), 0f32, 0f32],
            [(scale * -4f64).f32(), (scale * 4f64).f32(), 0f32, 0f32],
            [(scale * 1f64).f32(), (scale * 4f64).f32(), 0f32, 0f32],
            [(scale * 4f64).f32(), (scale * 4f64).f32(), 0f32, 0f32],
            [(scale * -7f64).f32(), (scale * 1f64).f32(), 0f32, 0f32],
            [(scale * 4f64).f32(), (scale * 1f64).f32(), 0f32, 0f32],
            [(scale * -7f64).f32(), (scale * -4f64).f32(), 0f32, 0f32],
            [(scale * 4f64).f32(), (scale * -4f64).f32(), 0f32, 0f32],
            [(scale * -7f64).f32(), (scale * -7f64).f32(), 0f32, 0f32],
            [(scale * -4f64).f32(), (scale * -7f64).f32(), 0f32, 0f32],
            [(scale * 1f64).f32(), (scale * -7f64).f32(), 0f32, 0f32],
            [(scale * 4f64).f32(), (scale * -7f64).f32(), 0f32, 0f32],
        ];
        let params_buffer = device
            .create_buffer_mapped(params.len(), wgpu::BufferUsage::all())
            .fill_from_slice(&params);

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: bind_group_layout,
            bindings: &[wgpu::Binding {
                binding: 0,
                resource: wgpu::BindingResource::Buffer {
                    buffer: &params_buffer,
                    range: 0..buffer_size,
                },
            }],
        });

        Ok((
            vertex_buffer,
            index_buffer,
            indices.len() as u32,
            bind_group,
        ))
    }

    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    pub fn index_buffer(&self) -> &wgpu::Buffer {
        &self.block_index_buffer
    }

    pub fn vertex_buffer(&self) -> &wgpu::Buffer {
        &self.block_vertex_buffer
    }

    pub fn index_range(&self) -> Range<u32> {
        0..self.block_index_count
    }

    pub fn block_bind_group(&self) -> &wgpu::BindGroup {
        &self.block_bind_group
    }
}
