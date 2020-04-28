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
use memoffset::offset_of;
use nalgebra::{Point3, Vector3};
use std::mem;
use wgpu;
use zerocopy::{AsBytes, FromBytes};

#[repr(C)]
#[derive(AsBytes, FromBytes, Copy, Clone, Default, Debug)]
pub struct DebugVertex {
    pub position: [f32; 4],
    pub color: [f32; 4],
}

impl DebugVertex {
    pub fn new(p: &Point3<f64>, _normal: &Vector3<f64>, color: &[f32; 3]) -> Self {
        Self {
            position: [p[0] as f32, p[1] as f32, p[2] as f32, 1f32],
            color: [color[0], color[1], color[2], 1f32],
        }
    }
}

impl DebugVertex {
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
                // color
                wgpu::VertexAttributeDescriptor {
                    format: wgpu::VertexFormat::Float4,
                    offset: 16,
                    shader_location: 1,
                },
            ],
        };

        assert_eq!(
            tmp.attributes[0].offset,
            offset_of!(DebugVertex, position) as wgpu::BufferAddress
        );
        assert_eq!(
            tmp.attributes[1].offset,
            offset_of!(DebugVertex, color) as wgpu::BufferAddress
        );

        assert_eq!(mem::size_of::<DebugVertex>(), 32);

        tmp
    }
}
