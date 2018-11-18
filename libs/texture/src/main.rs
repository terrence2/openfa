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
extern crate gpu;
extern crate image;
extern crate vulkano;
extern crate vulkano_shaders;
extern crate vulkano_win;
extern crate winit;

use std::sync::Arc;

use failure::{bail, err_msg, Fallible};
use gpu::{GraphicsConfigBuilder, GraphicsWindow};
use image::{ImageBuffer, Rgba};
use vulkano::{
    buffer::{BufferUsage, CpuAccessibleBuffer},
    command_buffer::{AutoCommandBufferBuilder, CommandBuffer, DynamicState},
    descriptor::descriptor_set::PersistentDescriptorSet,
    device::{Device, DeviceExtensions, Features, Queue},
    format::{ClearValue, Format},
    framebuffer::{Framebuffer, FramebufferAbstract, RenderPassAbstract, Subpass},
    image::{Dimensions, StorageImage, SwapchainImage},
    impl_vertex,
    instance::{Instance, InstanceExtensions, PhysicalDevice},
    pipeline::{viewport::Viewport, ComputePipeline, GraphicsPipeline},
    single_pass_renderpass, swapchain,
    swapchain::{
        acquire_next_image, AcquireError, PresentMode, Surface, SurfaceTransform, Swapchain,
        SwapchainCreationError,
    },
    sync,
    sync::{FlushError, GpuFuture},
};
use vulkano_win::VkSurfaceBuild;

#[derive(Copy, Clone)]
struct Vertex {
    position: [f32; 2],
}

impl_vertex!(Vertex, position);

mod vs {
    use vulkano_shaders::shader;

    shader! {
        ty: "vertex",
        src: "
            #version 450

            layout(location = 0) in vec2 position;

            void main() {
                gl_Position = vec4(position, 0.0, 1.0);
            }"
    }
}

mod fs {
    use vulkano_shaders::shader;

    shader! {
        ty: "fragment",
        src: "
            #version 450

            layout(location = 0) out vec4 f_color;

            void main() {
                f_color = vec4(1.0, 0.0, 0.0, 1.0);
            }
            "
    }
}

pub fn main() -> Fallible<()> {
    let mut window = GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?;

    // Resources
    let vertex_buffer = {
        let vertex1 = Vertex {
            position: [-0.5, -0.5],
        };
        let vertex2 = Vertex {
            position: [0.0, 0.5],
        };
        let vertex3 = Vertex {
            position: [0.5, -0.25],
        };
        CpuAccessibleBuffer::from_iter(
            window.device(),
            BufferUsage::all(),
            vec![vertex1, vertex2, vertex3].into_iter(),
        )?
    };
    let vs = vs::Shader::load(window.device())?;
    let fs = fs::Shader::load(window.device())?;

    let pipeline = Arc::new(
        GraphicsPipeline::start()
            .vertex_input_single_buffer::<Vertex>()
            .vertex_shader(vs.main_entry_point(), ())
            .viewports_dynamic_scissors_irrelevant(1)
            .fragment_shader(fs.main_entry_point(), ())
            .render_pass(
                Subpass::from(window.render_pass(), 0).expect("gfx: did not find a render pass"),
            ).build(window.device())?,
    );

    //let mut recreate_swapchain = false;

    //let mut previous_frame_end = Box::new(sync::now(window.device())) as Box<GpuFuture>;

    loop {
        window.drive_frame(|command_buffer, dynamic_state| {
            Ok(command_buffer.draw(
                pipeline.clone(),
                dynamic_state,
                vertex_buffer.clone(),
                (),
                (),
            )?)
        });

//        previous_frame_end.cleanup_finished();
//
//        // Whenever the window resizes we need to recreate everything dependent on the window size.
//        // In this example that includes the swapchain, the framebuffers and the dynamic state viewport.
//        if recreate_swapchain {
//            window.handle_resize();
//        }

//        let (image_num, acquire_future) = match acquire_next_image(window.swapchain(), None) {
//            Ok(r) => r,
//            Err(AcquireError::OutOfDate) => {
//                recreate_swapchain = true;
//                continue;
//            }
//            Err(err) => panic!("{:?}", err),
//        };
//
//        let command_buffer = AutoCommandBufferBuilder::primary_one_time_submit(
//            window.device(),
//            window.queue().family(),
//        )?.begin_render_pass(
//            window.framebuffer(image_num),
//            false,
//            vec![[0.0, 0.0, 1.0, 1.0].into()],
//        )?.draw(
//            pipeline.clone(),
//            window.dynamic_state(),
//            vertex_buffer.clone(),
//            (),
//            (),
//        )?.end_render_pass()?
//        .build()?;
//
//        let future = previous_frame_end
//            .join(acquire_future)
//            .then_execute(window.queue(), command_buffer)?
//            // The color output is now expected to contain our triangle. But in order to show it on
//            // the screen, we have to *present* the image by calling `present`.
//            //
//            // This function does not actually present the image immediately. Instead it submits a
//            // present command at the end of the queue. This means that it will only be presented once
//            // the GPU has finished executing the command buffer that draws the triangle.
//            .then_swapchain_present(window.queue(), window.swapchain(), image_num)
//            .then_signal_fence_and_flush();
//
//        match future {
//            Ok(future) => {
//                previous_frame_end = Box::new(future) as Box<_>;
//            }
//            Err(FlushError::OutOfDate) => {
//                recreate_swapchain = true;
//                previous_frame_end = Box::new(sync::now(window.device())) as Box<_>;
//            }
//            Err(e) => {
//                println!("{:?}", e);
//                previous_frame_end = Box::new(sync::now(window.device())) as Box<_>;
//            }
//        }

        let mut done = false;
        window.events_loop.poll_events(|ev| match ev {
            winit::Event::WindowEvent {
                event: winit::WindowEvent::CloseRequested,
                ..
            } => done = true,
            winit::Event::WindowEvent {
                event: winit::WindowEvent::Resized(_),
                ..
            } => /*recreate_swapchain = true*/ (),
            _ => (),
        });
        if done {
            return Ok(());
        }
    }

    Ok(())
}
