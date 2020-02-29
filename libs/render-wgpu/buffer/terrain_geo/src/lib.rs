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
use absolute_unit::{Angle, Degrees, Kilometers, Length, Meters};
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
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
}

impl TerrainGeoBuffer {
    pub fn new(device: &wgpu::Device) -> Fallible<Arc<RefCell<Self>>> {
        const SIZE: isize = 20;
        const EARTH_TO_KM: f32 = 6370.0;

        let mut verts = Vec::new();
        for lat in -SIZE..SIZE {
            for lon in -SIZE..SIZE {
                verts.push([lat as f32, lon as f32, EARTH_TO_KM, 1f32]);
            }
        }
        let vertex_buffer = device
            .create_buffer_mapped(verts.len(), wgpu::BufferUsage::all())
            .fill_from_slice(&verts);

        let mut indices = Vec::new();
        for i0 in 0..(2 * SIZE - 2) {
            let i1 = i0 + 1;
            for j0 in 0..(2 * SIZE - 2) {
                let j1 = j0 + 1;
                indices.push((i0 * 2 * SIZE + j0) as u32);
                indices.push((i1 * 2 * SIZE + j0) as u32);

                indices.push((i0 * 2 * SIZE + j0) as u32);
                indices.push((i0 * 2 * SIZE + j1) as u32);

                indices.push((i0 * 2 * SIZE + j0) as u32);
                indices.push((i1 * 2 * SIZE + j1) as u32);

                if i1 == (2 * SIZE - 2) {
                    indices.push((i1 * 2 * SIZE + j0) as u32);
                    indices.push((i1 * 2 * SIZE + j1) as u32);
                }

                if j1 == (2 * SIZE - 2) {
                    indices.push((i0 * 2 * SIZE + j1) as u32);
                    indices.push((i1 * 2 * SIZE + j1) as u32);
                }
            }
        }
        let index_buffer = device
            .create_buffer_mapped(indices.len(), wgpu::BufferUsage::all())
            .fill_from_slice(&indices);

        Ok(Arc::new(RefCell::new(Self {
            vertex_buffer,
            index_buffer,
            index_count: indices.len() as u32,
        })))
    }

    pub fn index_buffer(&self) -> &wgpu::Buffer {
        &self.index_buffer
    }

    pub fn vertex_buffer(&self) -> &wgpu::Buffer {
        &self.vertex_buffer
    }

    pub fn index_range(&self) -> Range<u32> {
        0..self.index_count
    }
}
