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
use nalgebra::{Matrix4, Point3};
use std::{mem, sync::Arc};
use wgpu;

pub struct GlobalParametersBuffer {
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    buffer_size: wgpu::BufferAddress,
    parameters_buffer: Arc<Box<wgpu::Buffer>>,
}

#[derive(Copy, Clone, Debug)]
struct Globals {
    view: [[f32; 4]; 4],
    proj: [[f32; 4]; 4],
    inv_view: [[f32; 4]; 4],
    inv_proj: [[f32; 4]; 4],
    camera_position: [f32; 4],
}

impl GlobalParametersBuffer {
    pub fn new(device: &wgpu::Device) -> Fallible<Arc<Box<Self>>> {
        let buffer_size = mem::size_of::<Globals>() as wgpu::BufferAddress;
        let parameters_buffer = Arc::new(Box::new(device.create_buffer(&wgpu::BufferDescriptor {
            size: buffer_size,
            usage: wgpu::BufferUsage::STORAGE_READ | wgpu::BufferUsage::COPY_DST,
        })));

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            bindings: &[wgpu::BindGroupLayoutBinding {
                binding: 0,
                visibility: wgpu::ShaderStage::all(),
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
        let globals = [Self::camera_to_buffer(camera)];
        let source = device
            .create_buffer_mapped::<Globals>(
                1,
                wgpu::BufferUsage::MAP_READ | wgpu::BufferUsage::COPY_SRC,
            )
            .fill_from_slice(&globals);
        upload_buffers.push(CopyBufferDescriptor::new(
            source,
            self.parameters_buffer.clone(),
            self.buffer_size,
        ));
        Ok(())
    }

    fn camera_to_buffer(camera: &dyn CameraAbstract) -> Globals {
        fn m2v(m: &Matrix4<f32>) -> [[f32; 4]; 4] {
            let mut v = [[0f32; 4]; 4];
            for i in 0..16 {
                v[i / 4][i % 4] = m[i];
            }
            v
        }
        fn p2v(p: &Point3<f32>) -> [f32; 4] {
            [p.x, p.y, p.z, 0f32]
        }

        Globals {
            view: m2v(&camera.view_matrix()),
            proj: m2v(&camera.projection_matrix()),
            inv_view: m2v(&camera.inverted_view_matrix()),
            inv_proj: m2v(&camera.inverted_projection_matrix()),
            camera_position: p2v(&camera.position()),
        }
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
