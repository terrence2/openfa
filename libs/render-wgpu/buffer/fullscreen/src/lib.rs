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
use camera_parameters::CameraParametersBuffer;
use failure::Fallible;
use std::mem;
use wgpu;

#[derive(Clone, Copy)]
pub struct FullscreenVertex {
    _pos: [f32; 2],
}

impl FullscreenVertex {
    pub fn new(pos: [i8; 2]) -> Self {
        Self {
            _pos: [f32::from(pos[0]), f32::from(pos[1])],
        }
    }

    pub fn buffer(device: &wgpu::Device) -> wgpu::Buffer {
        let vertices = vec![
            Self::new([-1, -1]),
            Self::new([-1, 1]),
            Self::new([1, -1]),
            Self::new([1, 1]),
        ];
        device
            .create_buffer_mapped(vertices.len(), wgpu::BufferUsage::VERTEX)
            .fill_from_slice(&vertices)
    }

    pub fn descriptor() -> wgpu::VertexBufferDescriptor<'static> {
        wgpu::VertexBufferDescriptor {
            stride: mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::InputStepMode::Vertex,
            attributes: &[wgpu::VertexAttributeDescriptor {
                format: wgpu::VertexFormat::Float2,
                offset: 0,
                shader_location: 0,
            }],
        }
    }
}

pub struct FullscreenBuffer {
    vertex_buffer: wgpu::Buffer,
}

impl FullscreenBuffer {
    pub fn new(_camera_buffer: &CameraParametersBuffer, device: &wgpu::Device) -> Fallible<Self> {
        Ok(Self {
            vertex_buffer: FullscreenVertex::buffer(device),
        })
    }

    pub fn vertex_buffer(&self) -> &wgpu::Buffer {
        &self.vertex_buffer
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpu::GPU;
    use input::InputSystem;

    #[test]
    fn it_can_create_a_buffer() -> Fallible<()> {
        let input = InputSystem::new(vec![])?;
        let gpu = GPU::new(&input, Default::default())?;
        let camera_buffer = CameraParametersBuffer::new(gpu.device())?;
        let _fullscreen_buffer = FullscreenBuffer::new(&camera_buffer, gpu.device())?;
        Ok(())
    }
}
