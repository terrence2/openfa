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
use failure::Fallible;
use image::{ImageBuffer, Rgba};
use log::trace;
use pal::Palette;
use std::sync::Arc;
use vulkano::{
    buffer::{BufferUsage, CpuAccessibleBuffer},
    command_buffer::{AutoCommandBufferBuilder, DynamicState},
    descriptor::descriptor_set::{DescriptorSet, PersistentDescriptorSet},
    device::Device,
    format::Format,
    framebuffer::Subpass,
    image::{Dimensions, ImmutableImage},
    impl_vertex,
    pipeline::{GraphicsPipeline, GraphicsPipelineAbstract},
    sampler::{Filter, MipmapMode, Sampler, SamplerAddressMode},
    sync::GpuFuture,
};
use window::GraphicsWindow;

#[derive(Copy, Clone)]
struct Vertex {
    position: [f32; 3],
    tex_coord: [f32; 2],
}
impl_vertex!(Vertex, position, tex_coord);

mod vs {
    use vulkano_shaders::shader;

    shader! {
    ty: "vertex",
        src: "
            #version 450

            layout(location = 0) in vec3 position;
            layout(location = 1) in vec2 tex_coord;

            layout(location = 1) out vec2 v_tex_coord;

            void main() {
                v_tex_coord = tex_coord;
                gl_Position = vec4(position, 1.0);
            }"
    }
}

mod fs {
    use vulkano_shaders::shader;

    shader! {
    ty: "fragment",
        src: "
            #version 450

            layout(location = 1) in vec2 v_tex_coord;

            layout(location = 0) out vec4 f_color;

            layout(set = 0, binding = 0) uniform sampler2D tex;

            void main() {
                f_color = texture(tex, v_tex_coord);
            }
            "
    }
}

pub struct PalRenderer {
    pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
    vertex_buffer: Arc<CpuAccessibleBuffer<[Vertex]>>,
    index_buffer: Arc<CpuAccessibleBuffer<[u32]>>,
    pds: Option<Arc<dyn DescriptorSet + Send + Sync>>,
}

impl PalRenderer {
    pub fn new(window: &GraphicsWindow) -> Fallible<Self> {
        trace!("T2Renderer::new");
        let vs = vs::Shader::load(window.device())?;
        let fs = fs::Shader::load(window.device())?;

        let pipeline = Arc::new(
            GraphicsPipeline::start()
                .vertex_input_single_buffer::<Vertex>()
                .vertex_shader(vs.main_entry_point(), ())
                .triangle_strip()
                .cull_mode_back()
                .front_face_counter_clockwise()
                .viewports_dynamic_scissors_irrelevant(1)
                .fragment_shader(fs.main_entry_point(), ())
                .render_pass(
                    Subpass::from(window.render_pass(), 0)
                        .expect("gfx: did not find a render pass"),
                )
                .build(window.device())?,
        );

        let pad = 0.01f32;
        let size = 0.15f32;

        let vertex_buffer = CpuAccessibleBuffer::from_iter(
            window.device(),
            BufferUsage::all(),
            vec![
                Vertex {
                    position: [-1.0 + pad, -1.0 + pad, 1.0],
                    tex_coord: [0.0, 0.0],
                },
                Vertex {
                    position: [
                        -1.0 + pad,
                        -1.0 + (size / window.aspect_ratio()?) + pad,
                        1.0,
                    ],
                    tex_coord: [0.0, 1.0],
                },
                Vertex {
                    position: [-1.0 + size + pad, -1.0 + pad, 1.0],
                    tex_coord: [1.0, 0.0],
                },
                Vertex {
                    position: [
                        -1.0 + size + pad,
                        -1.0 + (size / window.aspect_ratio()?) + pad,
                        1.0,
                    ],
                    tex_coord: [1.0, 1.0],
                },
            ]
            .into_iter(),
        )?;

        let index_buffer = CpuAccessibleBuffer::from_iter(
            window.device(),
            BufferUsage::all(),
            vec![0, 1, 2, 3].into_iter(),
        )?;

        Ok(Self {
            pipeline,
            vertex_buffer,
            index_buffer,
            pds: None,
        })
    }

    pub fn update_pal_data(&mut self, pal: &Palette, window: &GraphicsWindow) -> Fallible<()> {
        let mut img = ImageBuffer::new(16, 16);
        for i in 0..256 {
            img.put_pixel(i % 16, i / 16, pal.rgba(i as usize)?);
        }

        let (texture, tex_future) = Self::upload_texture_rgba(window, img)?;
        tex_future.then_signal_fence_and_flush()?.cleanup_finished();
        let sampler = Self::make_sampler(window.device())?;

        let pds = Arc::new(
            PersistentDescriptorSet::start(self.pipeline.clone(), 0)
                .add_sampled_image(texture.clone(), sampler.clone())?
                .build()?,
        );

        self.pds = Some(pds);

        Ok(())
    }

    pub fn render(
        &self,
        command_buffer: AutoCommandBufferBuilder,
        dynamic_state: &DynamicState,
    ) -> Fallible<AutoCommandBufferBuilder> {
        Ok(command_buffer.draw_indexed(
            self.pipeline.clone(),
            dynamic_state,
            vec![self.vertex_buffer.clone()],
            self.index_buffer.clone(),
            self.pds.clone().unwrap(),
            0,
        )?)
    }

    fn upload_texture_rgba(
        window: &GraphicsWindow,
        image_buf: ImageBuffer<Rgba<u8>, Vec<u8>>,
    ) -> Fallible<(Arc<ImmutableImage<Format>>, Box<GpuFuture>)> {
        let image_dim = image_buf.dimensions();
        let image_data = image_buf.into_raw().clone();

        let dimensions = Dimensions::Dim2d {
            width: image_dim.0,
            height: image_dim.1,
        };
        let (texture, tex_future) = ImmutableImage::from_iter(
            image_data.iter().cloned(),
            dimensions,
            Format::R8G8B8A8Unorm,
            window.queue(),
        )?;
        Ok((texture, Box::new(tex_future) as Box<GpuFuture>))
    }

    fn make_sampler(device: Arc<Device>) -> Fallible<Arc<Sampler>> {
        let sampler = Sampler::new(
            device.clone(),
            Filter::Nearest,
            Filter::Nearest,
            MipmapMode::Nearest,
            SamplerAddressMode::ClampToEdge,
            SamplerAddressMode::ClampToEdge,
            SamplerAddressMode::ClampToEdge,
            0.0,
            1.0,
            0.0,
            0.0,
        )?;

        Ok(sampler)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use omnilib::OmniLib;
    use window::{GraphicsConfigBuilder, GraphicsWindow};

    #[test]
    fn create_pal_renderer() -> Fallible<()> {
        let omni = OmniLib::new_for_test_in_games(&["FA"])?;
        let lib = omni.library("FA");
        let pal_data = lib.load("PALETTE.PAL")?;
        let pal = Palette::from_bytes(&pal_data)?;
        let mut window = GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?;
        let mut pal_renderer = PalRenderer::new(&window)?;
        pal_renderer.update_pal_data(&pal, &window)?;
        window.drive_frame(|command_buffer, dynamic_state| {
            pal_renderer.render(command_buffer, dynamic_state)
        })?;
        std::mem::drop(window);
        Ok(())
    }

}
