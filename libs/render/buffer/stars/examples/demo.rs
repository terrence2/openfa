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
use base::{RayMarchingRenderer, RayMarchingVertex};
use camera::{ArcBallCamera, CameraAbstract};
use failure::Fallible;
use input::{InputBindings, InputSystem};
use log::trace;
use nalgebra::Matrix4;
use stars::StarsBuffers;
use std::sync::Arc;
use vulkano::{
    command_buffer::AutoCommandBufferBuilder,
    framebuffer::Subpass,
    pipeline::{GraphicsPipeline, GraphicsPipelineAbstract},
};
use window::{GraphicsConfigBuilder, GraphicsWindow};

mod vs {
    use vulkano_shaders::shader;

    shader! {
    ty: "vertex",
    include: ["./libs/renderer/base/src"],
    src: "
        #version 450

        #include \"include_raymarching.glsl\"

        layout(push_constant) uniform PushConstantData {
          mat4 inverse_view;
          mat4 inverse_projection;
        } pc;

        layout(location = 0) in vec2 position;
        layout(location = 0) out vec3 v_ray;

        void main() {
            v_ray = raymarching_view_ray(position, pc.inverse_view, pc.inverse_projection);
            gl_Position = vec4(position, 0.0, 1.0);
        }"
    }
}

mod fs {
    use vulkano_shaders::shader;

    shader! {
    ty: "fragment",
    include: ["./libs/render/buffer/stars/src"],
    src: "
        #version 450

        layout(location = 0) in vec3 v_ray;
        layout(location = 0) out vec4 f_color;

        #include \"include_stars.glsl\"
        #include \"descriptorset_stars.glsl\"
        #include \"draw_stars.glsl\"

        void main() {
            #if SHOW_BINS
                f_color = vec4(show_bins(v_ray), 1.0);
                return;
            #endif

            f_color = vec4(show_stars(v_ray), 1.0);
        }
        "
    }
}

impl vs::ty::PushConstantData {
    fn new() -> Self {
        Self {
            inverse_projection: [
                [0.0f32, 0.0f32, 0.0f32, 0.0f32],
                [0.0f32, 0.0f32, 0.0f32, 0.0f32],
                [0.0f32, 0.0f32, 0.0f32, 0.0f32],
                [0.0f32, 0.0f32, 0.0f32, 0.0f32],
            ],
            inverse_view: [
                [0.0f32, 0.0f32, 0.0f32, 0.0f32],
                [0.0f32, 0.0f32, 0.0f32, 0.0f32],
                [0.0f32, 0.0f32, 0.0f32, 0.0f32],
                [0.0f32, 0.0f32, 0.0f32, 0.0f32],
            ],
        }
    }

    fn set_inverse_projection(&mut self, mat: Matrix4<f32>) {
        self.inverse_projection[0][0] = mat[0];
        self.inverse_projection[0][1] = mat[1];
        self.inverse_projection[0][2] = mat[2];
        self.inverse_projection[0][3] = mat[3];
        self.inverse_projection[1][0] = mat[4];
        self.inverse_projection[1][1] = mat[5];
        self.inverse_projection[1][2] = mat[6];
        self.inverse_projection[1][3] = mat[7];
        self.inverse_projection[2][0] = mat[8];
        self.inverse_projection[2][1] = mat[9];
        self.inverse_projection[2][2] = mat[10];
        self.inverse_projection[2][3] = mat[11];
        self.inverse_projection[3][0] = mat[12];
        self.inverse_projection[3][1] = mat[13];
        self.inverse_projection[3][2] = mat[14];
        self.inverse_projection[3][3] = mat[15];
    }

    fn set_inverse_view(&mut self, mat: Matrix4<f32>) {
        self.inverse_view[0][0] = mat[0];
        self.inverse_view[0][1] = mat[1];
        self.inverse_view[0][2] = mat[2];
        self.inverse_view[0][3] = mat[3];
        self.inverse_view[1][0] = mat[4];
        self.inverse_view[1][1] = mat[5];
        self.inverse_view[1][2] = mat[6];
        self.inverse_view[1][3] = mat[7];
        self.inverse_view[2][0] = mat[8];
        self.inverse_view[2][1] = mat[9];
        self.inverse_view[2][2] = mat[10];
        self.inverse_view[2][3] = mat[11];
        self.inverse_view[3][0] = mat[12];
        self.inverse_view[3][1] = mat[13];
        self.inverse_view[3][2] = mat[14];
        self.inverse_view[3][3] = mat[15];
    }
}

fn main() -> Fallible<()> {
    let mut window = GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?;
    window.hide_cursor()?;
    let bindings = InputBindings::new("base")
        .bind("exit", "Escape")?
        .bind("exit", "q")?;
    let mut input = InputSystem::new(&[&bindings]);

    let vert_shader = vs::Shader::load(window.device())?;
    let frag_shader = fs::Shader::load(window.device())?;
    let pipeline = Arc::new(
        GraphicsPipeline::start()
            .vertex_input_single_buffer::<RayMarchingVertex>()
            .vertex_shader(vert_shader.main_entry_point(), ())
            .triangle_strip()
            .cull_mode_back()
            .front_face_counter_clockwise()
            .viewports_dynamic_scissors_irrelevant(1)
            .fragment_shader(frag_shader.main_entry_point(), ())
            .render_pass(
                Subpass::from(window.render_pass(), 0).expect("gfx: did not find a render pass"),
            )
            .build(window.device())?,
    ) as Arc<dyn GraphicsPipelineAbstract + Send + Sync>;
    let raymarching_renderer = RayMarchingRenderer::new(pipeline.clone(), &window)?;
    let stars_buffers = StarsBuffers::new(&raymarching_renderer, pipeline.clone(), &window)?;
    let mut push_constants = vs::ty::PushConstantData::new();

    let mut camera = ArcBallCamera::new(window.aspect_ratio()?, 0.1f32, 3.4e+38f32);
    camera.set_distance(40f32);
    camera.on_mousebutton_down(1);

    let empty0 = GraphicsWindow::empty_descriptor_set(pipeline.clone(), 0)?;
    let empty1 = GraphicsWindow::empty_descriptor_set(pipeline.clone(), 1)?;

    loop {
        for command in input.poll(&mut window.events_loop) {
            match command.name.as_str() {
                "window-resize" => {
                    window.note_resize();
                    camera.set_aspect_ratio(window.aspect_ratio()?);
                }
                "window-close" | "window-destroy" | "exit" => return Ok(()),
                "mouse-move" => camera.on_mousemove(
                    command.displacement()?.0 as f32 / 4f32,
                    command.displacement()?.1 as f32 / 4f32,
                ),
                "window-cursor-move" => {}
                _ => trace!("unhandled command: {}", command.name),
            }
        }
        window.center_cursor()?;

        {
            let frame = window.begin_frame()?;
            if !frame.is_valid() {
                continue;
            }

            push_constants.set_inverse_projection(camera.inverted_projection_matrix());
            push_constants.set_inverse_view(camera.inverted_view_matrix());

            let mut cbb = AutoCommandBufferBuilder::primary_one_time_submit(
                window.device(),
                window.queue().family(),
            )?;

            cbb = cbb.begin_render_pass(
                frame.framebuffer(&window),
                false,
                vec![[0f32, 0f32, 1f32, 1f32].into(), 0f32.into()],
            )?;

            cbb = cbb.draw_indexed(
                pipeline.clone(),
                &window.dynamic_state,
                vec![raymarching_renderer.vertex_buffer.clone()],
                raymarching_renderer.index_buffer.clone(),
                (
                    empty0.clone(),
                    empty1.clone(),
                    stars_buffers.descriptor_set(),
                ),
                push_constants,
            )?;

            cbb = cbb.end_render_pass()?;

            let cb = cbb.build()?;

            frame.submit(cb, &mut window)?;
        }
    }
}
