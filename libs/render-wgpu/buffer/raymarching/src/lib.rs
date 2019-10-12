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
use camera::CameraAbstract;
use failure::Fallible;
use std::mem;
use wgpu;

#[derive(Clone, Copy)]
pub struct RaymarchingVertex {
    _pos: [f32; 2],
}

impl RaymarchingVertex {
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

pub struct RaymarchingBuffer {
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    buffer_size: u64,
    device_buffer: wgpu::Buffer,
    vertex_buffer: wgpu::Buffer,
}

impl RaymarchingBuffer {
    pub fn new(device: &wgpu::Device) -> Fallible<Self> {
        let buffer_size = mem::size_of::<[[[f32; 4]; 4]; 2]>() as u64;
        let device_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            size: buffer_size,
            usage: wgpu::BufferUsage::STORAGE_READ | wgpu::BufferUsage::COPY_DST,
        });

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

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
            bindings: &[wgpu::Binding {
                binding: 0,
                resource: wgpu::BindingResource::Buffer {
                    buffer: &device_buffer,
                    range: 0..buffer_size,
                },
            }],
        });

        Ok(Self {
            bind_group_layout,
            bind_group,
            buffer_size,
            device_buffer,
            vertex_buffer: RaymarchingVertex::buffer(device),
        })
    }

    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    pub fn vertex_buffer(&self) -> &wgpu::Buffer {
        &self.vertex_buffer
    }

    pub fn make_upload_buffer(
        &self,
        camera: &dyn CameraAbstract,
        device: &wgpu::Device,
    ) -> wgpu::Buffer {
        device
            .create_buffer_mapped::<[[f32; 4]; 4]>(
                2,
                wgpu::BufferUsage::MAP_READ | wgpu::BufferUsage::COPY_SRC,
            )
            .fill_from_slice(&Self::camera_to_buffer(camera))
    }

    fn camera_to_buffer(camera: &dyn CameraAbstract) -> [[[f32; 4]; 4]; 2] {
        // Inverted view and projection matrices, packed.
        let view = camera.inverted_view_matrix();
        let proj = camera.inverted_projection_matrix();
        let mut inv_view_proj = [[[0f32; 4]; 4]; 2];
        for i in 0..16 {
            inv_view_proj[0][i / 4][i % 4] = view[i];
        }
        for i in 0..16 {
            inv_view_proj[1][i / 4][i % 4] = proj[i];
        }
        inv_view_proj
    }

    pub fn upload_from(&self, frame: &mut gpu::Frame, upload_buffer: &wgpu::Buffer) {
        frame.copy_buffer_to_buffer(upload_buffer, 0, &self.device_buffer, 0, self.buffer_size);
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
        let _raymarching_buffer = RaymarchingBuffer::new(gpu.device())?;
        Ok(())
    }
}
