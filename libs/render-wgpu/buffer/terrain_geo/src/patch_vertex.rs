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
use absolute_unit::{Kilometers, Radians};
use geodesy::{Cartesian, GeoCenter, Graticule};
use memoffset::offset_of;
use nalgebra::{Point3, Vector3};
use std::mem;
use zerocopy::{AsBytes, FromBytes};

#[repr(C)]
#[derive(AsBytes, FromBytes, Copy, Clone, Default)]
pub struct PatchVertex {
    position: [f32; 3],
    normal: [f32; 3],
    graticule: [f32; 2],
}

impl PatchVertex {
    pub fn empty() -> Self {
        Self {
            position: [0f32; 3],
            normal: [0f32; 3],
            graticule: [0f32; 2],
        }
    }

    pub fn new(v0: &Point3<f64>, n0: &Vector3<f64>) -> Self {
        Self {
            position: [v0[0] as f32, v0[1] as f32, v0[2] as f32],
            normal: [n0[0] as f32, n0[1] as f32, n0[2] as f32],
            graticule: Graticule::<GeoCenter>::from(Cartesian::<GeoCenter, Kilometers>::from(*v0))
                .lat_lon::<Radians, f32>(),
        }
    }

    pub fn mem_size() -> usize {
        mem::size_of::<Self>()
    }

    #[allow(clippy::unneeded_field_pattern)]
    pub fn descriptor() -> wgpu::VertexBufferDescriptor<'static> {
        let tmp = wgpu::VertexBufferDescriptor {
            stride: mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::InputStepMode::Vertex,
            attributes: &[
                // position
                wgpu::VertexAttributeDescriptor {
                    format: wgpu::VertexFormat::Float3,
                    offset: 0,
                    shader_location: 0,
                },
                // normal
                wgpu::VertexAttributeDescriptor {
                    format: wgpu::VertexFormat::Float3,
                    offset: 12,
                    shader_location: 1,
                },
                // graticule
                wgpu::VertexAttributeDescriptor {
                    format: wgpu::VertexFormat::Float2,
                    offset: 24,
                    shader_location: 2,
                },
            ],
        };

        assert_eq!(
            tmp.attributes[0].offset,
            offset_of!(PatchVertex, position) as wgpu::BufferAddress
        );

        assert_eq!(
            tmp.attributes[1].offset,
            offset_of!(PatchVertex, normal) as wgpu::BufferAddress
        );

        assert_eq!(
            tmp.attributes[2].offset,
            offset_of!(PatchVertex, graticule) as wgpu::BufferAddress
        );

        assert_eq!(mem::size_of::<PatchVertex>(), 32);

        tmp
    }
}
