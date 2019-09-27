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
use nalgebra::{Matrix4, Point3};
use omnilib::OmniLib;
use pal::Palette;
use shape_chunk::{DrawSelection, DrawState, Vertex};
use shape_instance::{
    CoalesceSystem, FlagUpdateSystem, ShapeComponent, ShapeFlagBuffer, ShapeInstanceManager,
    ShapeTransformBuffer, ShapeXformBuffer, TransformUpdateSystem, XformUpdateSystem,
};
use specs::prelude::*;
use std::{sync::Arc, time::Instant};
use vulkano::{
    buffer::{BufferUsage, CpuAccessibleBuffer},
    command_buffer::AutoCommandBufferBuilder,
    descriptor::descriptor_set::{DescriptorSet, PersistentDescriptorSet},
    framebuffer::Subpass,
    pipeline::{
        depth_stencil::{Compare, DepthBounds, DepthStencil},
        GraphicsPipeline, GraphicsPipelineAbstract,
    },
    sync::GpuFuture,
};
use window::{GraphicsConfigBuilder, GraphicsWindow};
use world::Transform;

mod vs {
    use vulkano_shaders::shader;

    shader! {
    ty: "vertex",
    include: ["./libs/render"],
    src: "
        #version 450
        #include <common/include/include_global.glsl>
        #include <buffer/shape_chunk/src/include_shape.glsl>
        // Scene info
        layout(push_constant) uniform PushConstantData {
            mat4 view;
            mat4 projection;
        } pc;
        layout(set = 0, binding = 0) buffer GlobalData {
            int dummy;
        } globals;

        // Per shape input
        const uint MAX_XFORM_ID = 32;
        layout(set = 3, binding = 0) buffer ChunkBaseTransforms {
            float data[];
        } shape_transforms;
        layout(set = 3, binding = 1) buffer ChunkFlags {
            uint data[];
        } shape_flags;
        layout(set = 3, binding = 2) buffer ChunkXformOffsets {
            uint data[];
        } shape_xform_offsets;
//        layout(set = 4, binding = 2) buffer ChunkXforms {
//            float data[];
//        } shape_xforms;

        // Per Vertex input
        layout(location = 0) in vec3 position;
        layout(location = 1) in vec4 color;
        layout(location = 2) in vec2 tex_coord;
        layout(location = 3) in uint flags0;
        layout(location = 4) in uint flags1;
        layout(location = 5) in uint xform_id;
        layout(location = 0) smooth out vec4 v_color;
        layout(location = 1) smooth out vec2 v_tex_coord;
        layout(location = 2) flat out uint f_flags0;
        layout(location = 3) flat out uint f_flags1;
        void main() {
            uint base_transform = gl_InstanceIndex * 6;
            uint base_flag = gl_InstanceIndex * 2;
            uint base_xform = shape_xform_offsets.data[gl_InstanceIndex];
            float transform[6] = {
                shape_transforms.data[base_transform + 0],
                shape_transforms.data[base_transform + 1],
                shape_transforms.data[base_transform + 2],
                shape_transforms.data[base_transform + 3],
                shape_transforms.data[base_transform + 4],
                shape_transforms.data[base_transform + 5]
            };
            float xform[6] = {0, 0, 0, 0, 0, 0};
//            if (xform_id < MAX_XFORM_ID) {
//                xform[0] = shape_xforms.data[base_xform + 6 * xform_id + 0];
//                xform[1] = shape_xforms.data[base_xform + 6 * xform_id + 1];
//                xform[2] = shape_xforms.data[base_xform + 6 * xform_id + 2];
//                xform[3] = shape_xforms.data[base_xform + 6 * xform_id + 3];
//                xform[4] = shape_xforms.data[base_xform + 6 * xform_id + 4];
//                xform[5] = shape_xforms.data[base_xform + 6 * xform_id + 5];
//            }
            gl_Position = pc.projection *
                          pc.view *
                          matrix_for_xform(transform) *
                          matrix_for_xform(xform) *
                          vec4(position, 1.0);
            v_color = color;
            v_tex_coord = tex_coord;
            f_flags0 = flags0 & shape_flags.data[base_flag + 0];
            f_flags1 = flags1 & shape_flags.data[base_flag + 1];
        }"
    }
}

mod fs {
    use vulkano_shaders::shader;

    shader! {
    ty: "fragment",
    include: ["./libs/render"],
    src: "
        #version 450
        layout(location = 0) smooth in vec4 v_color;
        layout(location = 1) smooth in vec2 v_tex_coord;
        layout(location = 2) flat in uint f_flags0;
        layout(location = 3) flat in uint f_flags1;
        layout(location = 0) out vec4 f_color;

        layout(set = 5, binding = 0) uniform sampler2DArray mega_atlas;

        //layout(set = 5, binding = 1) uniform sampler2DArray nose_art; NOSE\\d\\d.PIC
        //layout(set = 5, binding = 2) uniform sampler2DArray left_tail_art; LEFT\\d\\d.PIC
        //layout(set = 5, binding = 3) uniform sampler2DArray right_tail_art; RIGHT\\d\\d.PIC
        //layout(set = 5, binding = 4) uniform sampler2DArray round_art; ROUND\\d\\d.PIC
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

fn build_pipeline(
    window: &GraphicsWindow,
) -> Fallible<Arc<dyn GraphicsPipelineAbstract + Send + Sync>> {
    let vert_shader = vs::Shader::load(window.device())?;
    let frag_shader = fs::Shader::load(window.device())?;
    Ok(Arc::new(
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
    ) as Arc<dyn GraphicsPipelineAbstract + Send + Sync>)
}

fn base_descriptors(
    pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
    window: &GraphicsWindow,
) -> Fallible<[Arc<dyn DescriptorSet + Send + Sync>; 3]> {
    let global0 = Arc::new(
        PersistentDescriptorSet::start(pipeline.clone(), GlobalSets::Global.into())
            .add_buffer(CpuAccessibleBuffer::from_data(
                window.device(),
                BufferUsage::all(),
                0u32,
            )?)?
            .build()?,
    );
    let empty1 = GraphicsWindow::empty_descriptor_set(pipeline.clone(), 1)?;
    let empty2 = GraphicsWindow::empty_descriptor_set(pipeline.clone(), 2)?;
    Ok([global0, empty1, empty2])
}

fn main() -> Fallible<()> {
    let mut window = GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?;
    window.hide_cursor()?;
    let bindings = InputBindings::new("base")
        .bind("exit", "Escape")?
        .bind("exit", "q")?;
    let mut input = InputSystem::new(&[&bindings]);

    let omni = OmniLib::new_for_test_in_games(&["FA"])?;
    let lib = omni.library("FA");
    let palette = Palette::from_bytes(&lib.load("PALETTE.PAL")?)?;
    let pipeline = build_pipeline(&window)?;
    let mut push_consts = vs::ty::PushConstantData::new();

    let mut inst_man = ShapeInstanceManager::new(
        pipeline.clone(),
        base_descriptors(pipeline.clone(), &window)?,
        &window,
    )?;

    let mut world = World::new();
    world.register::<ShapeComponent>();
    world.register::<ShapeTransformBuffer>();
    world.register::<ShapeFlagBuffer>();
    world.register::<ShapeXformBuffer>();
    world.register::<Transform>();

    const CNT: i32 = 100;
    for x in -CNT / 2..CNT / 2 {
        for y in -CNT / 2..CNT / 2 {
            let (shape_id, slot_id, _future) = inst_man.upload_and_allocate_slot(
                "F18.SH",
                DrawSelection::NormalModel,
                &palette,
                &lib,
                &window,
            )?;
            let errata = inst_man
                .chunk_man
                .part(shape_id)
                .widgets()
                .read()
                .unwrap()
                .errata();
            let _ent = world
                .create_entity()
                .with(Transform::new(Point3::new(
                    f64::from(x) * 10f64,
                    f64::from(y) * 10f64,
                    0f64,
                )))
                .with(ShapeComponent::new(slot_id, shape_id, DrawState::default()))
                .with(ShapeTransformBuffer::new())
                .with(ShapeFlagBuffer::new(errata))
                //.with(ShapeXformBuffer::new())
                .build();
        }
    }

    if let Some(future) = inst_man.ensure_finished(&window)? {
        future.then_signal_fence_and_flush()?.wait(None)?;
    }

    let mut camera = ArcBallCamera::new(window.aspect_ratio_f64()?, 0.1, 3.4e+38);
    camera.set_distance(120.0);
    camera.on_mousebutton_down(1);

    let start = Instant::now();
    let mut update_dispatcher = DispatcherBuilder::new()
        .with(TransformUpdateSystem, "transform-update", &[])
        .with(FlagUpdateSystem::new(&start), "flag-update", &[])
        .with(XformUpdateSystem::new(&start), "xform-update", &[])
        .build();

    loop {
        let loop_head = Instant::now();
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
                _ => println!("unhandled command: {}", command.name),
            }
        }
        window.center_cursor()?;

        let frame = window.begin_frame()?;
        if !frame.is_valid() {
            continue;
        }

        push_consts.set_projection(&camera.projection_matrix());
        push_consts.set_view(&camera.view_matrix());

        update_dispatcher.dispatch(&world);
        {
            DispatcherBuilder::new()
                .with(CoalesceSystem::new(&mut inst_man), "coalesce", &[])
                .build()
                .dispatch(&world);
        }

        let mut cbb = AutoCommandBufferBuilder::primary_one_time_submit(
            window.device(),
            window.queue().family(),
        )?;

        cbb = inst_man.upload_buffers(cbb)?;

        /*
        let upload_head = Instant::now();
        let shape_components = world.read_storage::<ShapeComponent>();
        let transforms = world.read_storage::<Transform>();
        for (shape_component, transform) in (&shape_components, &transforms).join() {
            if inst_man.get_and_clear_dirty_bit(shape_component.slot_id) {
                let cmd = inst_man
                    .chunk_man
                    .part(shape_component.shape_id)
                    .draw_command(0, 1);
                let src = inst_man.command_buffer_pool.chunk(vec![cmd])?;
                let dst = inst_man.command_buffer_target(shape_component.slot_id);
                cbb = cbb.copy_buffer(src, dst)?;
            }

            let src = inst_man
                .transform_buffer_pool
                .chunk(vec![transform.compact()])?;
            let dst = inst_man.transform_buffer_target(shape_component.slot_id);
            cbb = cbb.copy_buffer(src, dst)?;

            let errata = inst_man
                .chunk_man
                .part(shape_component.shape_id)
                .widgets()
                .read()
                .unwrap()
                .errata();
            let mut flags = [0u32; 2];
            shape_component
                .draw_state
                .build_mask_into(&start, errata, &mut flags)?;

            let src = inst_man.flag_buffer_pool.chunk(vec![flags])?;
            let dst = inst_man.flag_buffer_target(shape_component.slot_id);
            cbb = cbb.copy_buffer(src, dst)?;
        }
        //println!("Upload Time: {:?}", upload_head.elapsed());
        */

        cbb = cbb.begin_render_pass(
            frame.framebuffer(&window),
            false,
            vec![[0f32, 0f32, 1f32, 1f32].into(), 0f32.into()],
        )?;

        //cbb = inst_man.render(cbb, &window.dynamic_state, &push_consts)?;
        for block in inst_man.blocks.values() {
            let chunk = &inst_man.chunk_man.chunk(block.chunk_id);
            cbb = cbb.draw_indirect(
                pipeline.clone(),
                &window.dynamic_state,
                vec![chunk.vertex_buffer().clone()],
                block.command_buffer.clone(),
                (
                    inst_man.base_descriptors[0].clone(),
                    inst_man.base_descriptors[1].clone(),
                    inst_man.base_descriptors[2].clone(),
                    block.descriptor_set.clone(),
                    inst_man.base_descriptors[2].clone(),
                    chunk.atlas_descriptor_set_ref(),
                ),
                push_consts,
            )?;
        }

        cbb = cbb.end_render_pass()?;
        let cb = cbb.build()?;
        frame.submit(cb, &mut window)?;

        println!("Frame time: {:?}", loop_head.elapsed());
    }
}
