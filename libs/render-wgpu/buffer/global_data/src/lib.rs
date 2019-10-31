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
use frame_graph::CopyBufferDescriptor;
use std::{mem, sync::Arc};
use wgpu;

pub struct GlobalParametersBuffer {
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    buffer_size: wgpu::BufferAddress,
    parameters_buffer: Arc<Box<wgpu::Buffer>>,
}

const MATRIX_COUNT: usize = 4;
const VIEW_OFFSET: usize = 0;
const PROJ_OFFSET: usize = 1;
const INVERSE_VIEW_OFFSET: usize = 2;
const INVERSE_PROJ_OFFSET: usize = 3;

impl GlobalParametersBuffer {
    pub fn new(device: &wgpu::Device) -> Fallible<Arc<Box<Self>>> {
        let buffer_size = mem::size_of::<[[[f32; 4]; 4]; MATRIX_COUNT]>() as wgpu::BufferAddress;
        let parameters_buffer = Arc::new(Box::new(device.create_buffer(&wgpu::BufferDescriptor {
            size: buffer_size,
            usage: wgpu::BufferUsage::STORAGE_READ | wgpu::BufferUsage::COPY_DST,
        })));

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

        Ok(Arc::new(Box::new(Self {
            bind_group_layout,
            bind_group,
            buffer_size,
            parameters_buffer,
        })))
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
        upload_buffers: &mut Vec<CopyBufferDescriptor>,
    ) -> Fallible<()> {
        Ok(upload_buffers.push(CopyBufferDescriptor::new(
            device
                .create_buffer_mapped::<[[f32; 4]; 4]>(
                    MATRIX_COUNT,
                    wgpu::BufferUsage::MAP_READ | wgpu::BufferUsage::COPY_SRC,
                )
                .fill_from_slice(&Self::camera_to_buffer(camera)),
            self.parameters_buffer.clone(),
            self.buffer_size,
        )))
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
        let _globals_buffer = GlobalParametersBuffer::new(gpu.device())?;
        Ok(())
    }
}
