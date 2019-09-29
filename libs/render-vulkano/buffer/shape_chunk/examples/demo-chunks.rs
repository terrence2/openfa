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
use camera::{ArcBallCamera, CameraAbstract};
use failure::Fallible;
use global_layout::GlobalSets;
use input::{InputBindings, InputSystem};
use log::trace;
use nalgebra::Matrix4;
use omnilib::OmniLib;
use pal::Palette;
use shape_chunk::{DrawSelection, DrawState, ShapeChunkManager, Vertex};
use std::{sync::Arc, time::Instant};
use vulkano::{
    buffer::{BufferUsage, CpuAccessibleBuffer},
    command_buffer::AutoCommandBufferBuilder,
    descriptor::descriptor_set::PersistentDescriptorSet,
    framebuffer::Subpass,
    pipeline::{
        depth_stencil::{Compare, DepthBounds, DepthStencil},
        GraphicsPipeline, GraphicsPipelineAbstract,
    },
    sync::GpuFuture,
};
use window::{GraphicsConfigBuilder, GraphicsWindow};

mod vs {
    use vulkano_shaders::shader;

    shader! {
    ty: "vertex",
    include: ["./libs/render-vulkano"],
    src: "
        #version 450

        // Per Vertex input
        layout(location = 0) in vec3 position;
        layout(location = 1) in vec4 color;
        layout(location = 2) in vec2 tex_coord;
        layout(location = 3) in uint flags0;
        layout(location = 4) in uint flags1;
        layout(location = 5) in uint xform_id;

        // Per shape input
        layout(set = 3, binding = 0) buffer ChunkFlags {
            uint flag_data[];
        } flags;
        layout(set = 3, binding = 1) buffer ChunkTransforms {
            float xform_data[];
        } xforms;

        layout(push_constant) uniform PushConstantData {
            mat4 view;
            mat4 projection;
        } pc;

        #include <common/include/include_global.glsl>
        #include <buffer/shape_chunk/src/include_shape.glsl>

        layout(location = 0) smooth out vec4 v_color;
        layout(location = 1) smooth out vec2 v_tex_coord;
        layout(location = 2) flat out uint f_flags0;
        layout(location = 3) flat out uint f_flags1;

        void main() {
            uint shape_base_flag = 0;
            uint shape_base_xform = 0;
            if (gl_InstanceIndex >= 10) {
                shape_base_flag = 2;
                shape_base_xform = 24;
            }

            float xform[6] = {
                xforms.xform_data[shape_base_xform + 6 * xform_id + 0],
                xforms.xform_data[shape_base_xform + 6 * xform_id + 1],
                xforms.xform_data[shape_base_xform + 6 * xform_id + 2],
                xforms.xform_data[shape_base_xform + 6 * xform_id + 3],
                xforms.xform_data[shape_base_xform + 6 * xform_id + 4],
                xforms.xform_data[shape_base_xform + 6 * xform_id + 5],
            };

            gl_Position = pc.projection * pc.view * matrix_for_xform(xform) * vec4(position, 1.0);
            gl_Position.x += float(gl_InstanceIndex) * 10.0;
            v_color = color;
            v_tex_coord = tex_coord;

            f_flags0 = flags0 & flags.flag_data[shape_base_flag + 0];
            f_flags1 = flags1 & flags.flag_data[shape_base_flag + 1];
        }"
    }
}

mod fs {
    use vulkano_shaders::shader;

    shader! {
    ty: "fragment",
    include: ["./libs/render-vulkano"],
    src: "
        #version 450

        layout(location = 0) smooth in vec4 v_color;
        layout(location = 1) smooth in vec2 v_tex_coord;
        layout(location = 2) flat in uint f_flags0;
        layout(location = 3) flat in uint f_flags1;

        layout(location = 0) out vec4 f_color;

        layout(set = 5, binding = 0) uniform sampler2DArray mega_atlas;

        void main() {
            if ((f_flags0 & 0xFFFFFFFE) == 0 && f_flags1 == 0) {
                discard;
            } else if (v_tex_coord.x == 0.0) {
                f_color = v_color;
            } else {
                vec4 tex_color = texture(mega_atlas, vec3(v_tex_coord, 0));

                if ((f_flags0 & 1) == 1) {
                    f_color = vec4((1.0 - tex_color[3]) * v_color.xyz + tex_color[3] * tex_color.xyz, 1.0);
                } else {
                    if (tex_color.a < 0.5)
                        discard;
                    else
                        f_color = tex_color;
                }
            }
        }"
    }
}

impl vs::ty::PushConstantData {
    fn new() -> Self {
        Self {
            view: [
                [0.0f32, 0.0f32, 0.0f32, 0.0f32],
                [0.0f32, 0.0f32, 0.0f32, 0.0f32],
                [0.0f32, 0.0f32, 0.0f32, 0.0f32],
                [0.0f32, 0.0f32, 0.0f32, 0.0f32],
            ],
            projection: [
                [0.0f32, 0.0f32, 0.0f32, 0.0f32],
                [0.0f32, 0.0f32, 0.0f32, 0.0f32],
                [0.0f32, 0.0f32, 0.0f32, 0.0f32],
                [0.0f32, 0.0f32, 0.0f32, 0.0f32],
            ],
        }
    }

    fn set_view(&mut self, mat: &Matrix4<f32>) {
        self.view[0][0] = mat[0];
        self.view[0][1] = mat[1];
        self.view[0][2] = mat[2];
        self.view[0][3] = mat[3];
        self.view[1][0] = mat[4];
        self.view[1][1] = mat[5];
        self.view[1][2] = mat[6];
        self.view[1][3] = mat[7];
        self.view[2][0] = mat[8];
        self.view[2][1] = mat[9];
        self.view[2][2] = mat[10];
        self.view[2][3] = mat[11];
        self.view[3][0] = mat[12];
        self.view[3][1] = mat[13];
        self.view[3][2] = mat[14];
        self.view[3][3] = mat[15];
    }

    fn set_projection(&mut self, mat: &Matrix4<f32>) {
        self.projection[0][0] = mat[0];
        self.projection[0][1] = mat[1];
        self.projection[0][2] = mat[2];
        self.projection[0][3] = mat[3];
        self.projection[1][0] = mat[4];
        self.projection[1][1] = mat[5];
        self.projection[1][2] = mat[6];
        self.projection[1][3] = mat[7];
        self.projection[2][0] = mat[8];
        self.projection[2][1] = mat[9];
        self.projection[2][2] = mat[10];
        self.projection[2][3] = mat[11];
        self.projection[3][0] = mat[12];
        self.projection[3][1] = mat[13];
        self.projection[3][2] = mat[14];
        self.projection[3][3] = mat[15];
    }
}

fn main() -> Fallible<()> {
    let mut window = GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?;
    window.hide_cursor()?;
    let bindings = InputBindings::new("base")
        .bind("exit", "Escape")?
        .bind("exit", "q")?;
    let mut input = InputSystem::new(vec![bindings]);

    let vert_shader = vs::Shader::load(window.device())?;
    let frag_shader = fs::Shader::load(window.device())?;
    let pipeline = Arc::new(
        GraphicsPipeline::start()
            .vertex_input_single_buffer::<Vertex>()
            .vertex_shader(vert_shader.main_entry_point(), ())
            .triangle_list()
            .cull_mode_back()
            .front_face_clockwise()
            .viewports_dynamic_scissors_irrelevant(1)
            .fragment_shader(frag_shader.main_entry_point(), ())
            .depth_stencil(DepthStencil {
                depth_write: true,
                depth_compare: Compare::GreaterOrEqual,
                depth_bounds_test: DepthBounds::Disabled,
                stencil_front: Default::default(),
                stencil_back: Default::default(),
            })
            .blend_alpha_blending()
            .render_pass(
                Subpass::from(window.render_pass(), 0).expect("gfx: did not find a render pass"),
            )
            .build(window.device())?,
    ) as Arc<dyn GraphicsPipelineAbstract + Send + Sync>;
    let mut push_constants = vs::ty::PushConstantData::new();

    let omni = OmniLib::new_for_test_in_games(&["FA"])?;
    let lib = omni.library("FA");
    let palette = Palette::from_bytes(&lib.load("PALETTE.PAL")?)?;

    let mut chunk_man = ShapeChunkManager::new(pipeline.clone(), &window)?;
    chunk_man.upload_shape("F8.SH", DrawSelection::NormalModel, &palette, &lib, &window)?;
    let (chunk_id, _shape_id, _) = chunk_man.upload_shape(
        "F18.SH",
        DrawSelection::NormalModel,
        &palette,
        &lib,
        &window,
    )?;
    let future = chunk_man.finish(&window)?.unwrap();
    future.then_signal_fence_and_flush()?.wait(None)?;

    let f18_part = chunk_man.part_for("F18.SH")?;

    // Upload flags
    let mut draw_state: DrawState = Default::default();
    draw_state.toggle_gear(&Instant::now());
    let mut flags_arr = [0u32; 2];
    draw_state.build_mask_into(
        draw_state.time_origin(),
        f18_part.widgets().read().unwrap().errata(),
        &mut flags_arr[0..2],
    )?;
    let flags_buffer = CpuAccessibleBuffer::from_iter(
        window.device(),
        BufferUsage::all(),
        flags_arr.iter().cloned(),
    )?;

    // Upload transforms
    let now = Instant::now();
    let xforms_len = f18_part.widgets().read().unwrap().num_transformer_floats();
    let mut xforms = Vec::with_capacity(xforms_len);
    xforms.resize(xforms_len, 0f32);
    f18_part.widgets().write().unwrap().animate_into(
        &draw_state,
        draw_state.time_origin(),
        &now,
        &mut xforms[0..],
    )?;
    let xforms_buffer = CpuAccessibleBuffer::from_iter(
        window.device(),
        BufferUsage::all(),
        xforms.iter().cloned(),
    )?;

    let shape_descriptor_set = Arc::new(
        PersistentDescriptorSet::start(pipeline.clone(), GlobalSets::ShapeBuffers.into())
            .add_buffer(flags_buffer)?
            .add_buffer(xforms_buffer)?
            .build()?,
    );

    let indirect_buffer = CpuAccessibleBuffer::from_iter(
        window.device(),
        BufferUsage::all(),
        [f18_part.draw_command(0, 1)].iter().cloned(),
    )?;

    let mut camera = ArcBallCamera::new(window.aspect_ratio_f64()?, 0.1, 3.4e+38);
    camera.set_distance(80.0);
    camera.on_mousebutton_down(1);

    let empty0 = GraphicsWindow::empty_descriptor_set(pipeline.clone(), 0)?;
    loop {
        for command in input.poll(&mut window.events_loop) {
            match command.name.as_str() {
                "window-resize" => {
                    window.note_resize();
                    camera.set_aspect_ratio(window.aspect_ratio_f64()?);
                }
                "window-close" | "window-destroy" | "exit" => return Ok(()),
                "mouse-move" => camera.on_mousemove(
                    command.displacement()?.0 / 4.0,
                    command.displacement()?.1 / 4.0,
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

            push_constants.set_projection(&camera.projection_matrix());
            push_constants.set_view(&camera.view_matrix());

            let mut cbb = AutoCommandBufferBuilder::primary_one_time_submit(
                window.device(),
                window.queue().family(),
            )?;

            cbb = cbb.begin_render_pass(
                frame.framebuffer(&window),
                false,
                vec![[0f32, 0f32, 1f32, 1f32].into(), 0f32.into()],
            )?;

            let chunk = chunk_man.chunk(chunk_id);
            cbb = cbb.draw_indirect(
                pipeline.clone(),
                &window.dynamic_state,
                vec![chunk.vertex_buffer()],
                indirect_buffer.clone(),
                (
                    empty0.clone(),
                    empty0.clone(),
                    empty0.clone(),
                    shape_descriptor_set.clone(),
                    empty0.clone(),
                    chunk.atlas_descriptor_set_ref(),
                ),
                push_constants,
            )?;

            cbb = cbb.end_render_pass()?;

            let cb = cbb.build()?;

            frame.submit(cb, &mut window)?;
        }
    }
}
