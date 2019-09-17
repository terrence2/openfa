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
use failure::{bail, err_msg, Fallible};
use log::{error, trace};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use vulkano::{
    command_buffer::{AutoCommandBuffer, DynamicState},
    descriptor::{descriptor_set::PersistentDescriptorSet, DescriptorSet},
    device::{Device, Queue, RawDeviceExtensions},
    format::Format,
    framebuffer::{Framebuffer, FramebufferAbstract, RenderPassAbstract},
    image::{attachment::AttachmentImage, traits::ImageAccess},
    instance::{Instance, PhysicalDevice},
    pipeline::{viewport::Viewport, GraphicsPipelineAbstract},
    single_pass_renderpass,
    swapchain::{
        acquire_next_image, AcquireError, PresentMode, Surface, SurfaceTransform, Swapchain,
    },
    sync,
    sync::{FlushError, GpuFuture},
};
use vulkano_win::VkSurfaceBuild;
use winit::{EventsLoop, Window, WindowBuilder};

#[derive(Debug)]
pub struct GraphicsConfig {
    device_index: usize,
    samples: usize,
}

pub struct GraphicsConfigBuilder(GraphicsConfig);

impl GraphicsConfigBuilder {
    // const DEPTH_FORMAT: Format = Format::D16Unorm;
    const DEPTH_FORMAT: Format = Format::D32Sfloat;

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
        trace!("{:?}", self.0);
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
    framebuffers: Vec<Arc<dyn FramebufferAbstract + Send + Sync>>,
    render_pass: Arc<dyn RenderPassAbstract + Send + Sync>,
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
        //for fmt in caps.supported_formats {
        //    println!("FMT: {:?}", fmt);
        //}

        let dimensions = GraphicsWindow::surface_dimensions(surface)?;

        let depth_buffer = AttachmentImage::transient(
            device.clone(),
            dimensions,
            GraphicsConfigBuilder::DEPTH_FORMAT,
        )?;

        let (swapchain, images) = Swapchain::new(
            device.clone(),
            surface.clone(),
            caps.min_image_count,
            format,
            dimensions,
            1,
            caps.supported_usage_flags,
            queue,
            SurfaceTransform::Identity,
            alpha,
            PresentMode::Relaxed,
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
                },
                depth: {
                    load: Clear,
                    store: DontCare,
                    format: GraphicsConfigBuilder::DEPTH_FORMAT,
                    samples: 1,
                }
            },
            pass: {
                color: [color],
                depth_stencil: {depth}
            }
        )?);

        let mut framebuffers = Vec::new();
        for image in &images {
            let framebuffer = Framebuffer::start(render_pass.clone())
                .add(image.clone())?
                .add(depth_buffer.clone())?
                .build()?;
            framebuffers.push(Arc::new(framebuffer) as Arc<dyn FramebufferAbstract + Send + Sync>);
        }
        trace!("created {} frame buffers", framebuffers.len());

        Ok(SizeDependent {
            swapchain,
            framebuffers,
            render_pass,
        })
    }

    fn handle_resize(
        &mut self,
        device: &Arc<Device>,
        _queue: &Arc<Queue>,
        surface: &Arc<Surface<Window>>,
    ) -> Fallible<()> {
        let dimensions = GraphicsWindow::surface_dimensions(surface)?;
        trace!(
            "resizing to dimensions: {}x{}",
            dimensions[0],
            dimensions[1]
        );

        let depth_buffer = AttachmentImage::transient(
            device.clone(),
            dimensions,
            GraphicsConfigBuilder::DEPTH_FORMAT,
        )?;

        let (swapchain, images) = self.swapchain.recreate_with_dimension(dimensions)?;
        self.swapchain = swapchain;

        let mut framebuffers = Vec::new();
        for image in &images {
            let framebuffer = Framebuffer::start(self.render_pass.clone())
                .add(image.clone())?
                .add(depth_buffer.clone())?
                .build()?;
            framebuffers.push(Arc::new(framebuffer) as Arc<dyn FramebufferAbstract + Send + Sync>);
        }
        self.framebuffers = framebuffers;

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
    pub dynamic_state: DynamicState,
    outstanding_frame: Option<Box<dyn GpuFuture>>,
    dirty_size: bool,
    pub idle_time: Duration,
    clear_color: [f32; 4],
}

impl GraphicsWindow {
    pub fn new(config: &GraphicsConfig) -> Fallible<Self> {
        trace!("GraphicsWindow::new");
        let extensions = vulkano_win::required_extensions();
        let instance = Instance::new(None, &extensions, None)?;

        let events_loop = EventsLoop::new();

        trace!("creating vulcan surface");
        let surface = WindowBuilder::new().build_vk_surface(&events_loop, instance.clone())?;

        trace!("using device index {}", config.device_index);
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
            physical.supported_features(),
            RawDeviceExtensions::supported_by_device_raw(physical)?,
            [(queue_family, 0.5)].iter().cloned(),
        )?;
        let queues = queues.collect::<Vec<_>>();
        trace!("created device for {}", device.physical_device().name());

        let recreatable = SizeDependent::new(
            &device.clone(),
            &surface.clone(),
            &queues[0].clone(),
            config,
        )?;

        let outstanding_frame = Some(Box::new(sync::now(device.clone())) as Box<_>);

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
            outstanding_frame,
            dirty_size: false,
            idle_time: Default::default(),
            clear_color: [0.0, 0.0, 1.0, 1.0],
        };

        // Set initial size.
        let dim = window.dimensions()?;
        window.dynamic_state.viewports = Some(vec![Viewport {
            origin: [0.0, 0.0],
            dimensions: dim,
            depth_range: 0.0..1.0,
        }]);

        Ok(window)
    }

    pub fn set_clear_color(&mut self, clr: &[f32; 4]) {
        self.clear_color = *clr;
    }

    pub fn dimensions(&self) -> Fallible<[f32; 2]> {
        let dim = Self::surface_dimensions(&self.surface)?;
        Ok([dim[0] as f32, dim[1] as f32])
    }

    pub fn empty_descriptor_set(
        pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
        id: usize,
    ) -> Fallible<Arc<dyn DescriptorSet + Send + Sync>> {
        Ok(Arc::new(
            PersistentDescriptorSet::start(pipeline, id).build()?,
        ))
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

    // Note: width over height
    pub fn aspect_ratio(&self) -> Fallible<f32> {
        let dim = self.dimensions()?;
        Ok(dim[1] / dim[0])
    }
    pub fn aspect_ratio_f64(&self) -> Fallible<f64> {
        let dim = self.dimensions()?;
        Ok(f64::from(dim[1] / dim[0]))
    }

    pub fn now(&self) -> Box<dyn GpuFuture> {
        Box::new(sync::now(self.device())) as Box<dyn GpuFuture>
    }

    pub fn device(&self) -> Arc<Device> {
        self.device.clone()
    }

    pub fn queue(&self) -> Arc<Queue> {
        self.queues[0].clone()
    }

    pub fn center_cursor(&self) -> Fallible<()> {
        let dim = self.dimensions()?;
        use winit::dpi::LogicalPosition;
        match self
            .surface
            .window()
            .set_cursor_position(LogicalPosition::new(
                f64::from(dim[0] / 2f32),
                f64::from(dim[1] / 2f32),
            )) {
            Ok(_) => Ok(()),
            Err(s) => Err(err_msg(s)),
        }
    }

    pub fn hide_cursor(&self) -> Fallible<()> {
        self.surface.window().hide_cursor(true);
        Ok(())
    }

    pub fn render_pass(&self) -> Arc<dyn RenderPassAbstract + Send + Sync> {
        self.recreatable.render_pass.clone()
    }

    fn swapchain(&self) -> Arc<Swapchain<Window>> {
        self.recreatable.swapchain.clone()
    }

    fn framebuffer(&self, offset: usize) -> Arc<dyn FramebufferAbstract + Send + Sync> {
        self.recreatable.framebuffers[offset].clone()
    }

    pub fn note_resize(&mut self) {
        self.dirty_size = true;
    }

    pub fn handle_resize(&mut self) -> Fallible<()> {
        let dim = self.dimensions()?;

        self.dynamic_state.viewports = Some(vec![Viewport {
            origin: [0.0, 0.0],
            dimensions: dim,
            depth_range: 1.0..0.0,
        }]);

        self.recreatable
            .handle_resize(&self.device, &self.queues[0], &self.surface)
    }

    pub fn begin_frame(&mut self) -> Fallible<FrameState> {
        // Maybe resize
        if self.dirty_size {
            self.dirty_size = false;
            self.handle_resize()?;
        }

        // Grab the next image in the swapchain when available.
        let (image_num, acquire_future) = match acquire_next_image(self.swapchain(), None) {
            Ok(r) => r,
            Err(AcquireError::OutOfDate) => {
                // Nothing we can do this frame; try again next round.
                self.dirty_size = true;
                return Ok(FrameState::invalid());
            }
            Err(err) => bail!("{:?}", err),
        };

        // Copy our prior frame's future to the frame state.
        // TODO: We will need to copy this back somehow.
        let mut prior_frame_future = None;
        std::mem::swap(&mut self.outstanding_frame, &mut prior_frame_future);
        assert!(prior_frame_future.is_some());

        Ok(FrameState {
            valid: true,
            swapchain_offset: image_num,
            acquire_future: Some(Box::new(acquire_future) as Box<dyn GpuFuture>),
            prior_frame_future: Some(prior_frame_future.unwrap()),
        })
    }
}

pub struct FrameState {
    valid: bool,
    swapchain_offset: usize,
    acquire_future: Option<Box<dyn GpuFuture>>,
    prior_frame_future: Option<Box<dyn GpuFuture>>,
}

impl FrameState {
    pub fn invalid() -> Self {
        FrameState {
            valid: false,
            swapchain_offset: 0,
            acquire_future: None,
            prior_frame_future: None,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.valid
    }

    pub fn framebuffer(
        &self,
        window: &GraphicsWindow,
    ) -> Arc<dyn FramebufferAbstract + Send + Sync> {
        window.framebuffer(self.swapchain_offset)
    }

    pub fn submit(self, cb: AutoCommandBuffer, window: &mut GraphicsWindow) -> Fallible<()> {
        assert!(self.acquire_future.is_some());
        assert!(self.prior_frame_future.is_some());

        let mut prior_frame_future = self.prior_frame_future.unwrap();
        prior_frame_future.cleanup_finished();

        // Wait for the prior frame to finish and for a new backbuffer to become available.
        // Then put our command buffer into the queue.
        // Then flip, signal, fence, and flush.
        let idle_start = Instant::now();
        let next_frame_future = prior_frame_future
            .join(self.acquire_future.unwrap())
            .then_execute(window.queue(), cb)?
            .then_swapchain_present(window.queue(), window.swapchain(), self.swapchain_offset)
            .then_signal_fence_and_flush();
        window.idle_time = idle_start.elapsed();

        // But do not wait for this frame to finish; put it on the heap
        // for us to deal with later.
        let next_frame_future = match next_frame_future {
            Ok(future) => Box::new(future) as Box<_>,
            Err(FlushError::OutOfDate) => {
                window.dirty_size = true;
                Box::new(sync::now(window.device())) as Box<_>
            }
            Err(e) => {
                // FIXME: find a way to report this sanely. We do not want to bail here
                // FIXME: as then our outstanding_frames would get out of sync.
                error!("{:?}", e);
                Box::new(sync::now(window.device())) as Box<_>
            }
        };
        window.outstanding_frame = Some(next_frame_future);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulkano::command_buffer::AutoCommandBufferBuilder;

    #[test]
    fn it_works() -> Fallible<()> {
        let _window = GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?;

        Ok(())
    }

    #[test]
    fn test_frame_state() -> Fallible<()> {
        let mut window = GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?;

        for _ in 0..20 {
            let frame = window.begin_frame()?;
            if !frame.is_valid() {
                continue;
            }

            let mut cbb = AutoCommandBufferBuilder::primary_one_time_submit(
                window.device(),
                window.queue().family(),
            )?;
            cbb = cbb.begin_render_pass(
                frame.framebuffer(&window),
                false,
                vec![[0f32, 0f32, 1f32, 1f32].into(), 0f32.into()],
            )?;
            cbb = cbb.end_render_pass()?;
            let cb = cbb.build()?;

            frame.submit(cb, &mut window)?;
        }

        Ok(())
    }
}
