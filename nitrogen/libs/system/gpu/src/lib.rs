// This file is part of Nitrogen.
//
// Nitrogen is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Nitrogen is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with Nitrogen.  If not, see <http://www.gnu.org/licenses/>.
use failure::{err_msg, Fallible};
use frame_graph::FrameStateTracker;
use futures::executor::block_on;
use input::InputSystem;
use log::trace;
use std::{io::Cursor, mem, sync::Arc};
use winit::dpi::PhysicalSize;
use zerocopy::{AsBytes, FromBytes};

#[repr(C)]
#[derive(AsBytes, FromBytes, Copy, Clone, Debug)]
pub struct DrawIndirectCommand {
    pub vertex_count: u32,
    pub instance_count: u32,
    pub first_vertex: u32,
    pub first_instance: u32,
}

pub struct GPUConfig {
    anisotropic_filtering: bool,
    max_bind_groups: u32,
    present_mode: wgpu::PresentMode,
}
impl Default for GPUConfig {
    fn default() -> Self {
        Self {
            anisotropic_filtering: false,
            max_bind_groups: 6,
            present_mode: wgpu::PresentMode::Mailbox,
        }
    }
}

pub struct GPU {
    surface: wgpu::Surface,
    _adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    swap_chain: wgpu::SwapChain,
    depth_texture: wgpu::TextureView,

    config: GPUConfig,
    size: PhysicalSize,

    empty_layout: wgpu::BindGroupLayout,
}

impl GPU {
    pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
    pub const SCREEN_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;

    pub fn texture_format() -> wgpu::TextureFormat {
        wgpu::TextureFormat::Bgra8Unorm
    }

    pub fn aspect_ratio(&self) -> f64 {
        self.size.height.floor() / self.size.width.floor()
    }

    pub fn aspect_ratio_f32(&self) -> f32 {
        (self.size.height.floor() / self.size.width.floor()) as f32
    }

    pub fn physical_size(&self) -> PhysicalSize {
        self.size
    }

    pub fn new(input: &InputSystem, config: GPUConfig) -> Fallible<Self> {
        block_on(Self::new_async(input, config))
    }

    pub async fn new_async(input: &InputSystem, config: GPUConfig) -> Fallible<Self> {
        input.window().set_title("Nitrogen");
        let surface = wgpu::Surface::create(input.window());

        let adapter = wgpu::Adapter::request(
            &wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
            },
            wgpu::BackendBit::PRIMARY,
        )
        .await
        .ok_or_else(|| err_msg("no suitable graphics adapter"))?;

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                extensions: wgpu::Extensions {
                    anisotropic_filtering: config.anisotropic_filtering,
                },
                limits: wgpu::Limits {
                    max_bind_groups: config.max_bind_groups,
                },
            })
            .await;

        let size = input
            .window()
            .inner_size()
            .to_physical(input.window().hidpi_factor());
        let sc_desc = wgpu::SwapChainDescriptor {
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
            format: Self::texture_format(),
            width: size.width.floor() as u32,
            height: size.height.floor() as u32,
            present_mode: config.present_mode,
        };
        let swap_chain = device.create_swap_chain(&surface, &sc_desc);
        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth-texture"),
            size: wgpu::Extent3d {
                width: sc_desc.width,
                height: sc_desc.height,
                depth: 1,
            },
            array_layer_count: 1,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::DEPTH_FORMAT,
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
        });

        let empty_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("empty-layout"),
            bindings: &[],
        });

        Ok(Self {
            surface,
            _adapter: adapter,
            device,
            queue,
            swap_chain,
            depth_texture: depth_texture.create_default_view(),
            config,
            size,
            empty_layout,
        })
    }

    pub fn note_resize(&mut self, input: &InputSystem) {
        self.size = input
            .window()
            .inner_size()
            .to_physical(input.window().hidpi_factor());
        let sc_desc = wgpu::SwapChainDescriptor {
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
            format: Self::texture_format(),
            width: self.size.width.floor() as u32,
            height: self.size.height.floor() as u32,
            present_mode: self.config.present_mode,
        };
        self.swap_chain = self.device.create_swap_chain(&self.surface, &sc_desc);
        self.depth_texture = self
            .device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("depth-texture"),
                size: wgpu::Extent3d {
                    width: sc_desc.width,
                    height: sc_desc.height,
                    depth: 1,
                },
                array_layer_count: 1,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: Self::DEPTH_FORMAT,
                usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
            })
            .create_default_view();
    }

    pub fn maybe_push_buffer(
        &self,
        label: &'static str,
        data: &[u8],
        usage: wgpu::BufferUsage,
    ) -> Option<wgpu::Buffer> {
        if data.is_empty() {
            return None;
        }
        let size = data.len() as wgpu::BufferAddress;
        trace!("uploading {} with {} bytes", label, size);
        let cpu_buffer = self.device.create_buffer_mapped(&wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage,
        });
        cpu_buffer.data.copy_from_slice(data);
        Some(cpu_buffer.finish())
    }

    pub fn maybe_push_slice<T: AsBytes>(
        &self,
        label: &'static str,
        data: &[T],
        usage: wgpu::BufferUsage,
    ) -> Option<wgpu::Buffer> {
        if data.is_empty() {
            return None;
        }
        let size = (mem::size_of::<T>() * data.len()) as wgpu::BufferAddress;
        trace!("uploading {} with {} bytes", label, size);
        let cpu_buffer = self.device.create_buffer_mapped(&wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage,
        });
        cpu_buffer.data.copy_from_slice(data.as_bytes());
        Some(cpu_buffer.finish())
    }

    pub fn push_buffer(
        &self,
        label: &'static str,
        data: &[u8],
        usage: wgpu::BufferUsage,
    ) -> wgpu::Buffer {
        self.maybe_push_buffer(label, data, usage)
            .expect("push non-empty buffer")
    }

    pub fn push_slice<T: AsBytes>(
        &self,
        label: &'static str,
        data: &[T],
        usage: wgpu::BufferUsage,
    ) -> wgpu::Buffer {
        self.maybe_push_slice(label, data, usage)
            .expect("push non-empty slice")
    }

    pub fn push_data<T: AsBytes>(
        &self,
        label: &'static str,
        data: &T,
        usage: wgpu::BufferUsage,
    ) -> wgpu::Buffer {
        let size = mem::size_of::<T>() as wgpu::BufferAddress;
        trace!("uploading {} with {} bytes", label, size);
        let cpu_buffer = self.device.create_buffer_mapped(&wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage,
        });
        cpu_buffer.data.copy_from_slice(data.as_bytes());
        cpu_buffer.finish()
    }

    pub fn upload_slice_to<T: AsBytes>(
        &self,
        label: &'static str,
        data: &[T],
        target: Arc<Box<wgpu::Buffer>>,
        usage: wgpu::BufferUsage,
        tracker: &mut FrameStateTracker,
    ) {
        if let Some(source) = self.maybe_push_slice(label, data, usage) {
            tracker.upload(source, target, mem::size_of::<T>() * data.len());
        }
    }

    pub fn create_shader_module(&self, spirv: &[u8]) -> Fallible<wgpu::ShaderModule> {
        let spirv_words = wgpu::read_spirv(Cursor::new(spirv))?;
        Ok(self.device.create_shader_module(&spirv_words))
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn device_mut(&mut self) -> &mut wgpu::Device {
        &mut self.device
    }

    pub fn queue_mut(&mut self) -> &mut wgpu::Queue {
        &mut self.queue
    }

    pub fn device_and_queue_mut(&mut self) -> (&mut wgpu::Device, &mut wgpu::Queue) {
        (&mut self.device, &mut self.queue)
    }

    pub fn empty_layout(&self) -> &wgpu::BindGroupLayout {
        &self.empty_layout
    }

    pub fn begin_frame(&mut self) -> Fallible<Frame> {
        let color_attachment = self
            .swap_chain
            .get_next_texture()
            .map_err(|_| err_msg("failed to get next swap chain image"))?;
        Ok(Frame {
            queue: &mut self.queue,
            encoder: self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("frame-encoder"),
                }),
            color_attachment,
            depth_attachment: &self.depth_texture,
        })
    }
}

pub struct Frame<'a> {
    queue: &'a mut wgpu::Queue,
    encoder: wgpu::CommandEncoder,
    color_attachment: wgpu::SwapChainOutput,
    depth_attachment: &'a wgpu::TextureView,
}

impl<'a> Frame<'a> {
    pub fn begin_render_pass(&mut self) -> wgpu::RenderPass {
        self.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                attachment: &self.color_attachment.view,
                resolve_target: None,
                load_op: wgpu::LoadOp::Clear,
                store_op: wgpu::StoreOp::Store,
                clear_color: wgpu::Color::GREEN,
            }],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachmentDescriptor {
                attachment: self.depth_attachment,
                depth_load_op: wgpu::LoadOp::Clear,
                depth_store_op: wgpu::StoreOp::Store,
                clear_depth: 1f32,
                stencil_load_op: wgpu::LoadOp::Clear,
                stencil_store_op: wgpu::StoreOp::Store,
                clear_stencil: 0,
            }),
        })
    }

    pub fn begin_compute_pass(&mut self) -> wgpu::ComputePass {
        self.encoder.begin_compute_pass()
    }

    pub fn finish(self) {
        self.queue.submit(&[self.encoder.finish()]);
    }

    pub fn copy_buffer_to_buffer(
        &mut self,
        source: &wgpu::Buffer,
        source_offset: wgpu::BufferAddress,
        destination: &wgpu::Buffer,
        destination_offset: wgpu::BufferAddress,
        copy_size: wgpu::BufferAddress,
    ) {
        self.encoder.copy_buffer_to_buffer(
            source,
            source_offset,
            destination,
            destination_offset,
            copy_size,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create() -> Fallible<()> {
        let input = InputSystem::new(vec![])?;
        let _gpu = GPU::new(&input, Default::default())?;
        Ok(())
    }
}
