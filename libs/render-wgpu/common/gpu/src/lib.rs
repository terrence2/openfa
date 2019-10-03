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
}

impl GPU {
    pub fn texture_format() -> wgpu::TextureFormat {
        wgpu::TextureFormat::Bgra8UnormSrgb
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
        })
    }

    pub fn create_shader_module(&self, spirv: &[u8]) -> Fallible<wgpu::ShaderModule> {
        let spirv_words = wgpu::read_spirv(Cursor::new(spirv))?;
        Ok(self.device.create_shader_module(&spirv_words))
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue_mut(&mut self) -> &mut wgpu::Queue {
        &mut self.queue
    }

    pub fn swap_chain_mut(&mut self) -> &mut wgpu::SwapChain {
        &mut self.swap_chain
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
