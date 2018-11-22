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
extern crate failure;
extern crate vulkano;
extern crate vulkano_win;
extern crate winit;

use failure::{bail, err_msg, Fallible};
use std::{collections::VecDeque, sync::Arc};
use vulkano::{
    command_buffer::{AutoCommandBufferBuilder, DynamicState},
    device::{Device, Features, Queue, RawDeviceExtensions},
    framebuffer::{Framebuffer, FramebufferAbstract, RenderPassAbstract},
    image::traits::ImageAccess,
    instance::{Instance, PhysicalDevice},
    pipeline::viewport::Viewport,
    single_pass_renderpass,
    swapchain::{
        acquire_next_image, AcquireError, PresentMode, Surface, SurfaceTransform, Swapchain,
    },
    sync,
    sync::{FlushError, GpuFuture},
};
use vulkano_win::VkSurfaceBuild;
use winit::{EventsLoop, Window, WindowBuilder};

pub struct GraphicsConfig {
    device_index: usize,
    samples: usize,
}

pub struct GraphicsConfigBuilder(GraphicsConfig);

impl GraphicsConfigBuilder {
    pub fn new() -> Self {
        GraphicsConfigBuilder(GraphicsConfig {
            device_index: 0,
            samples: 1,
        })
    }

    pub fn select_device(mut self, index: usize) -> Self {
        self.0.device_index = index;
        self
    }

    pub fn use_multisampling(mut self, samples: usize) -> Self {
        self.0.samples = samples;
        self
    }

    pub fn build(self) -> GraphicsConfig {
        self.0
    }
}

impl Default for GraphicsConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

struct SizeDependent {
    swapchain: Arc<Swapchain<Window>>,
    framebuffers: Vec<Arc<FramebufferAbstract + Send + Sync>>,
    render_pass: Arc<RenderPassAbstract + Send + Sync>,
}

impl SizeDependent {
    fn new(
        device: &Arc<Device>,
        surface: &Arc<Surface<Window>>,
        queue: &Arc<Queue>,
        _config: &GraphicsConfig,
    ) -> Fallible<Self> {
        let caps = surface.capabilities(device.physical_device())?;

        // FIXME: search for post-multiplied alpha (I think?).
        let alpha = caps
            .supported_composite_alpha
            .iter()
            .next()
            .expect("missing composite alpha support");

        // FIXME: search our formats for something suitable.
        let format = caps.supported_formats[0].0;

        let (swapchain, images) = Swapchain::new(
            device.clone(),
            surface.clone(),
            caps.min_image_count,
            format,
            GraphicsWindow::surface_dimensions(surface)?,
            1,
            caps.supported_usage_flags,
            queue,
            SurfaceTransform::Identity,
            alpha,
            PresentMode::Fifo,
            true,
            None,
        )?;

        // FIXME: configure this with what's in GraphicsConfig.
        // Although the render pass is not sized, the format depends on whatever we receive
        // for a swapchain, so must be created after it.
        let render_pass = Arc::new(single_pass_renderpass!(device.clone(),
            attachments: {
                color: {
                    load: Clear,
                    store: Store,
                    format: images[0].format(),
                    samples: 1,
                }
            },
            pass: {
                color: [color],
                depth_stencil: {}
            }
        )?);

        let framebuffers = images
            .iter()
            .map(|image| {
                Arc::new(
                    Framebuffer::start(render_pass.clone())
                        .add(image.clone())
                        .unwrap()
                        .build()
                        .unwrap(),
                ) as Arc<FramebufferAbstract + Send + Sync>
            }).collect::<Vec<_>>();

        Ok(SizeDependent {
            swapchain,
            framebuffers,
            render_pass,
        })
    }

    fn handle_resize(&mut self, surface: &Arc<Surface<Window>>) -> Fallible<()> {
        let dimensions = GraphicsWindow::surface_dimensions(surface)?;

        let (swapchain, images) = self.swapchain.recreate_with_dimension(dimensions)?;
        self.swapchain = swapchain;

        // Note: The format for rendering should not change with size changes. If it does we're
        // going to need to re-architect, but given that the pipelines depend on render_pass...
        //ensure!(render_pass.format() == images[0].format(), "gfx: size change modified format");

        self.framebuffers = images
            .iter()
            .map(|image| {
                Arc::new(
                    Framebuffer::start(self.render_pass.clone())
                        .add(image.clone())
                        .unwrap()
                        .build()
                        .unwrap(),
                ) as Arc<FramebufferAbstract + Send + Sync>
            }).collect::<Vec<_>>();

        Ok(())
    }
}

pub struct GraphicsWindow {
    // Permanent resources
    _instance: Arc<Instance>,
    device: Arc<Device>,
    queues: Vec<Arc<Queue>>,
    pub events_loop: EventsLoop,
    surface: Arc<Surface<winit::Window>>,

    // Size-dependent resources
    recreatable: SizeDependent,

    // Per-frame resources
    dynamic_state: DynamicState,
    outstanding_frames: VecDeque<Box<GpuFuture>>,
    dirty_size: bool,
}

impl GraphicsWindow {
    pub fn new(config: &GraphicsConfig) -> Fallible<Self> {
        let instance = Instance::new(None, &vulkano_win::required_extensions(), None)?;

        let events_loop = EventsLoop::new();

        let surface = WindowBuilder::new().build_vk_surface(&events_loop, instance.clone())?;

        let physical = PhysicalDevice::from_index(&instance, config.device_index)
            .ok_or_else(|| err_msg(format!("gpu: no device at index {}", config.device_index)))?;

        let queue_family = physical
            .queue_families()
            .find(|&q| q.supports_graphics())
            .ok_or_else(|| {
                err_msg(format!(
                    "gpu: device '{}' has no graphics support",
                    physical.name()
                ))
            })?;

        let (device, queues) = Device::new(
            physical,
            &Features::none(),
            RawDeviceExtensions::supported_by_device_raw(physical)?,
            [(queue_family, 0.5)].iter().cloned(),
        )?;
        let queues = queues.collect::<Vec<_>>();

        let recreatable =
            SizeDependent::new(&device.clone(), &surface.clone(), &queues[0].clone(), config)?;

        let mut window = GraphicsWindow {
            _instance: instance.clone(),
            device,
            queues,
            events_loop,
            surface: surface.clone(),
            recreatable,

            dynamic_state: DynamicState {
                ..DynamicState::none()
            },
            outstanding_frames: VecDeque::with_capacity(4),
            dirty_size: false,
        };

        // Push a fake first frame so that we have something wait on in our frame driver.
        let fake_frame = Box::new(sync::now(window.device())) as Box<GpuFuture>;
        window.outstanding_frames.push_back(fake_frame);

        // Set initial size.
        let dim = window.dimensions()?;
        window.dynamic_state.viewports = Some(vec![Viewport {
            origin: [0.0, 0.0],
            dimensions: dim,
            depth_range: 0.0..1.0,
        }]);

        Ok(window)
    }

    pub fn dimensions(&self) -> Fallible<[f32; 2]> {
        let dim = Self::surface_dimensions(&self.surface)?;
        Ok([dim[0] as f32, dim[1] as f32])
    }

    pub fn surface_dimensions(surface: &Arc<Surface<Window>>) -> Fallible<[u32; 2]> {
        if let Some(dimensions) = surface.window().get_inner_size() {
            let dim: (u32, u32) = dimensions
                .to_physical(surface.window().get_hidpi_factor())
                .into();
            return Ok([dim.0, dim.1]);
        }
        bail!("unable to get window size")
    }

    pub fn device(&self) -> Arc<Device> {
        self.device.clone()
    }

    pub fn queue(&self) -> Arc<Queue> {
        self.queues[0].clone()
    }

    pub fn dynamic_state(&self) -> &DynamicState {
        &self.dynamic_state
    }

    pub fn render_pass(&self) -> Arc<RenderPassAbstract + Send + Sync> {
        self.recreatable.render_pass.clone()
    }

    pub fn swapchain(&self) -> Arc<Swapchain<Window>> {
        self.recreatable.swapchain.clone()
    }

    pub fn framebuffer(&self, offset: usize) -> Arc<FramebufferAbstract + Send + Sync> {
        self.recreatable.framebuffers[offset].clone()
    }

    pub fn handle_resize(&mut self) -> Fallible<()> {
        let dim = self.dimensions()?;

        self.dynamic_state.viewports = Some(vec![Viewport {
            origin: [0.0, 0.0],
            dimensions: dim,
            depth_range: 0.0..1.0,
        }]);

        self.recreatable.handle_resize(&self.surface)
    }

    pub fn drive_frame<F>(&mut self, draw: F) -> Fallible<()>
    where
        F: Fn(AutoCommandBufferBuilder, &DynamicState) -> Fallible<AutoCommandBufferBuilder>,
    {
        // Cleanup finished
        for f in self.outstanding_frames.iter_mut() {
            f.cleanup_finished();
        }

        // Maybe resize
        if self.dirty_size {
            self.dirty_size = false;
            self.handle_resize()?;
        }

        // Grab the next image in the swapchain.
        // Note: blocks until a frame is available.
        let (image_num, acquire_future) = match acquire_next_image(self.swapchain(), None) {
            Ok(r) => r,
            Err(AcquireError::OutOfDate) => {
                // Nothing we can do this frame; try again next round.
                self.dirty_size = true;
                return Ok(());
            }
            Err(err) => bail!("{:?}", err),
        };

        let command_buffer = AutoCommandBufferBuilder::primary_one_time_submit(
            self.device(),
            self.queue().family(),
        )?.begin_render_pass(
            self.framebuffer(image_num),
            false,
            vec![[0.0, 0.0, 1.0, 1.0].into()],
        )?;
        let command_buffer = draw(command_buffer, &self.dynamic_state)?
            .end_render_pass()?
            .build()?;

        // Wait for our oldest frame to finish, submit the new command buffer, then send
        // it down the next beam.
        let next_frame_future = self
            .outstanding_frames
            .pop_front()
            .expect("gfx: no prior frames")
            .join(acquire_future)
            .then_execute(self.queue(), command_buffer)?
            .then_swapchain_present(self.queue(), self.swapchain(), image_num)
            .then_signal_fence_and_flush();

        // But do not wait for this frame to finish; put it on the heap
        // for us to deal with later.
        let next_frame_future = match next_frame_future {
            Ok(future) => Box::new(future) as Box<_>,
            Err(FlushError::OutOfDate) => {
                self.dirty_size = true;
                Box::new(sync::now(self.device())) as Box<_>
            }
            Err(e) => {
                // FIXME: find a way to report this sanely. We do not want to bail here
                // FIXME: as then our outstanding_frames would get out of sync.
                println!("{:?}", e);
                Box::new(sync::now(self.device())) as Box<_>
            }
        };
        self.outstanding_frames.push_back(next_frame_future);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() -> Fallible<()> {
        let gpu = GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?;

        Ok(())
    }
}
