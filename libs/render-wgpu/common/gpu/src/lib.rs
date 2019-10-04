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
use failure::{err_msg, Fallible};
use input_wgpu::InputSystem;
use std::io::Cursor;
use wgpu;
use winit::dpi::PhysicalSize;

pub struct GPUConfig {
    anisotropic_filtering: bool,
    max_bind_groups: u32,
    preset_mode: wgpu::PresentMode,
}
impl Default for GPUConfig {
    fn default() -> Self {
        Self {
            anisotropic_filtering: false,
            max_bind_groups: 6,
            preset_mode: wgpu::PresentMode::Vsync,
        }
    }
}

pub struct GPU {
    surface: wgpu::Surface,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    swap_chain: wgpu::SwapChain,

    config: GPUConfig,
    size: PhysicalSize,
}

impl GPU {
    pub fn texture_format() -> wgpu::TextureFormat {
        wgpu::TextureFormat::Bgra8UnormSrgb
    }

    pub fn aspect_ratio(&self) -> f64 {
        self.size.height.floor() / self.size.width.floor()
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
            present_mode: self.config.preset_mode,
        };
        self.swap_chain = self.device.create_swap_chain(&self.surface, &sc_desc);
    }

    pub fn new(input: &InputSystem, config: GPUConfig) -> Fallible<Self> {
        input.window().set_title("OpenFA");
        let surface = wgpu::Surface::create(input.window());

        let adapter = wgpu::Adapter::request(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            backends: wgpu::BackendBit::PRIMARY,
        })
        .ok_or_else(|| err_msg("no suitable graphics adapter"))?;

        let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor {
            extensions: wgpu::Extensions {
                anisotropic_filtering: config.anisotropic_filtering,
            },
            limits: wgpu::Limits {
                max_bind_groups: config.max_bind_groups,
            },
        });

        let size = input
            .window()
            .inner_size()
            .to_physical(input.window().hidpi_factor());
        let sc_desc = wgpu::SwapChainDescriptor {
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
            format: Self::texture_format(),
            width: size.width.floor() as u32,
            height: size.height.floor() as u32,
            present_mode: config.preset_mode,
        };
        let swap_chain = device.create_swap_chain(&surface, &sc_desc);

        Ok(Self {
            surface,
            adapter,
            device,
            queue,
            swap_chain,
            config,
            size,
        })
    }

    pub fn create_shader_module(&self, spirv: &[u8]) -> Fallible<wgpu::ShaderModule> {
        let spirv_words = wgpu::read_spirv(Cursor::new(spirv))?;
        Ok(self.device.create_shader_module(&spirv_words))
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn begin_frame(&mut self) -> Frame {
        Frame {
            queue: &mut self.queue,
            encoder: self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor { todo: 0 }),
            color_attachment: self.swap_chain.get_next_texture(),
        }
    }
}

pub struct Frame<'a> {
    queue: &'a mut wgpu::Queue,
    encoder: wgpu::CommandEncoder,
    color_attachment: wgpu::SwapChainOutput<'a>,
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
            depth_stencil_attachment: None,
        })
    }

    pub fn finish(mut self) {
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
        let gpu = GPU::new(&input, Default::default())?;
        Ok(())
    }
}
