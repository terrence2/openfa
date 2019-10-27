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

pub struct CameraParametersBuffer {
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    buffer_size: u64,
    parameters_buffer: wgpu::Buffer,
}

const MATRIX_COUNT: usize = 4;
const VIEW_OFFSET: usize = 0;
const PROJ_OFFSET: usize = 1;
const INVERSE_VIEW_OFFSET: usize = 2;
const INVERSE_PROJ_OFFSET: usize = 3;

impl CameraParametersBuffer {
    pub fn new(device: &wgpu::Device) -> Fallible<Self> {
        let buffer_size = mem::size_of::<[[[f32; 4]; 4]; MATRIX_COUNT]>() as u64;
        let parameters_buffer = device.create_buffer(&wgpu::BufferDescriptor {
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
                    buffer: &parameters_buffer,
                    range: 0..buffer_size,
                },
            }],
        });

        Ok(Self {
            bind_group_layout,
            bind_group,
            buffer_size,
            parameters_buffer,
        })
    }

    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    pub fn make_upload_buffer(
        &self,
        camera: &dyn CameraAbstract,
        device: &wgpu::Device,
    ) -> wgpu::Buffer {
        device
            .create_buffer_mapped::<[[f32; 4]; 4]>(
                MATRIX_COUNT,
                wgpu::BufferUsage::MAP_READ | wgpu::BufferUsage::COPY_SRC,
            )
            .fill_from_slice(&Self::camera_to_buffer(camera))
    }

    fn camera_to_buffer(camera: &dyn CameraAbstract) -> [[[f32; 4]; 4]; MATRIX_COUNT] {
        let mut parameters = [[[0f32; 4]; 4]; MATRIX_COUNT];
        {
            let inv_view = camera.inverted_view_matrix();
            for i in 0..16 {
                parameters[INVERSE_VIEW_OFFSET][i / 4][i % 4] = inv_view[i];
            }
        }
        {
            let inv_proj = camera.inverted_projection_matrix();
            for i in 0..16 {
                parameters[INVERSE_PROJ_OFFSET][i / 4][i % 4] = inv_proj[i];
            }
        }
        {
            let view = camera.view_matrix();
            for i in 0..16 {
                parameters[VIEW_OFFSET][i / 4][i % 4] = view[i];
            }
        }
        {
            let proj = camera.projection_matrix();
            for i in 0..16 {
                parameters[PROJ_OFFSET][i / 4][i % 4] = proj[i];
            }
        }
        parameters
    }

    pub fn upload_from(&self, frame: &mut gpu::Frame, upload_buffer: &wgpu::Buffer) {
        frame.copy_buffer_to_buffer(
            upload_buffer,
            0,
            &self.parameters_buffer,
            0,
            self.buffer_size,
        );
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
        let _camera_buffer = CameraParametersBuffer::new(gpu.device())?;
        Ok(())
    }
}
