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

use backend::{backend, backend::Backend};
use failure::Fallible;
use gpu::Gpu;
use hal::{
    format::Format, Adapter, AdapterInfo, Instance, PresentMode, Surface, SurfaceCapabilities,
};
use winit::{
    dpi::{LogicalSize, PhysicalSize},
    EventsLoop, WindowBuilder,
};

pub struct Window {
    gpu: Option<Gpu>,
    surface: Box<Surface<Backend>>,
    instance: backend::Instance,
    window: ::winit::Window,
    event_loop: EventsLoop,
}

impl Window {
    pub fn new(width: usize, height: usize, title: &str) -> Fallible<Self> {
        let event_loop = EventsLoop::new();
        let wb = WindowBuilder::new()
            .with_dimensions(LogicalSize::from_physical(
                PhysicalSize {
                    width: width as f64,
                    height: height as f64,
                },
                1.0,
            ))
            .with_title(title.to_owned());
        let window = wb.build(&event_loop)?;

        let instance = backend::Instance::create(title, 1);
        let surface = Box::new(instance.create_surface(&window));

        return Ok(Self {
            gpu: None,
            surface,
            instance,
            window,
            event_loop,
        });
    }

    pub fn gpu(&self) -> Fallible<&Gpu> {
        if let Some(ref gpu) = self.gpu {
            return Ok(gpu);
        }
        bail!("window does not have a gpu attached");
    }

    pub fn capabilities(&self, adapter: &Adapter<Backend>) -> SurfaceCapabilities {
        let (caps, _formats, _present_modes) = self.surface.compatibility(&adapter.physical_device);
        return caps;
    }

    pub fn formats(&self, adapter: &Adapter<Backend>) -> Vec<Format> {
        let (_caps, formats, _present_modes) = self.surface.compatibility(&adapter.physical_device);
        return formats.expect("graphics contexts should have formats");
    }

    pub fn presentation_modes(&self, adapter: &Adapter<Backend>) -> Vec<PresentMode> {
        let (_caps, _formats, present_modes) = self.surface.compatibility(&adapter.physical_device);
        return present_modes;
    }

    pub fn enumerate_adapters(&self) -> Vec<Adapter<Backend>> {
        return self.instance.enumerate_adapters();
    }

    pub fn select_any_adapter(&mut self) -> Fallible<AdapterInfo> {
        let mut adapter = self.enumerate_adapters().remove(0);
        self.gpu = Some(Gpu::new(&mut adapter, &self.surface)?);
        return Ok(adapter.info);
    }

    pub fn select_adapter(&self, vendor: usize, device: usize) -> Fallible<Adapter<Backend>> {
        let mut adapters = self.enumerate_adapters();
        for adapter in adapters.drain(..) {
            if adapter.info.vendor == vendor && adapter.info.device == device {
                return Ok(adapter);
            }
        }
        bail!("no such adapter {}/{}", vendor, device);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_window() -> Fallible<()> {
        let mut win = Window::new(800, 600, "test")?;
        assert!(win.enumerate_adapters().len() > 0);
        let info0 = win.select_any_adapter()?;
        let adapter1 = win.select_adapter(info0.vendor, info0.device)?;
        return Ok(());
    }
}
