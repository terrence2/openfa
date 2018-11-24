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
extern crate image;
extern crate texture;
extern crate vulkano;
extern crate vulkano_shaders;
extern crate window;
extern crate winit;

use failure::Fallible;
use std::{path::Path, sync::Arc};
use texture::TextureManager;
use vulkano::{
    buffer::{BufferUsage, CpuAccessibleBuffer},
    descriptor::descriptor_set::PersistentDescriptorSet,
    framebuffer::Subpass,
    impl_vertex,
    pipeline::GraphicsPipeline,
    sync::GpuFuture,
};
use window::{GraphicsConfigBuilder, GraphicsWindow};

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
            layout(set = 0, binding = 0) uniform sampler2D tex;

            void main() {
                f_color = vec4(1.0, 0.0, 0.0, 1.0);
            }
            "
    }
}

pub fn main() -> Fallible<()> {
    let library = lib::LibStack::from_dir_search(Path::new("../../test_data/unpacked/FA"))?;
    let mut window = GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?;

    let texman = TextureManager::new(&library)?;
    let (texture, tex_future) = texman.load_texture("FLARE.PIC", window.queue())?;
    tex_future.then_signal_fence_and_flush()?.cleanup_finished();
    let sampler = TextureManager::make_sampler(window.device())?;

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

    let set = Arc::new(
        PersistentDescriptorSet::start(pipeline.clone(), 0)
            .add_sampled_image(texture.clone(), sampler.clone())?
            .build()?,
    );

    loop {
        window.drive_frame(|command_buffer, dynamic_state| {
            Ok(command_buffer.draw(
                pipeline.clone(),
                dynamic_state,
                vertex_buffer.clone(),
                set.clone(),
                (),
            )?)
        })?;

        let mut done = false;
        window.events_loop.poll_events(|ev| match ev {
            winit::Event::WindowEvent {
                event: winit::WindowEvent::CloseRequested,
                ..
            } => done = true,
            winit::Event::WindowEvent {
                event: winit::WindowEvent::Resized(_),
                ..
            } =>
            /*recreate_swapchain = true*/
            {
                ()
            }
            _ => (),
        });
        if done {
            return Ok(());
        }
    }
}
