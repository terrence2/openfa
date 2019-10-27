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
use camera::ArcBallCamera;
use failure::Fallible;
use global_data::CameraParametersBuffer;
use gpu::GPU;
use input::{InputBindings, InputSystem};
use nalgebra::Point3;
use omnilib::OmniLib;
use pal::Palette;
use shape_chunk::{DrawSelection, DrawState, Vertex};
use shape_instance_wgpu::{
    CoalesceSystem, FlagUpdateSystem, ShapeComponent, ShapeFlagBuffer, ShapeInstanceManager,
    ShapeTransformBuffer, ShapeXformBuffer, TransformUpdateSystem, XformUpdateSystem,
};
use specs::prelude::*;
use std::time::Instant;
use world::Transform;

fn build_pipeline(
    gpu: &mut gpu::GPU,
    empty_layout: &wgpu::BindGroupLayout,
    camera_buffer: &CameraParametersBuffer,
    inst_man: &ShapeInstanceManager,
) -> Fallible<wgpu::RenderPipeline> {
    let vert_shader = gpu.create_shader_module(include_bytes!("../target/example.vert.spirv"))?;
    let frag_shader = gpu.create_shader_module(include_bytes!("../target/example.frag.spirv"))?;

    let pipeline_layout = gpu
        .device()
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            bind_group_layouts: &[
                camera_buffer.bind_group_layout(),
                &empty_layout,
                &empty_layout,
                inst_man.bind_group_layout(),
                &empty_layout,
                inst_man.chunk_man.bind_group_layout(),
            ],
        });

    Ok(gpu
        .device()
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            layout: &pipeline_layout,
            vertex_stage: wgpu::ProgrammableStageDescriptor {
                module: &vert_shader,
                entry_point: "main",
            },
            fragment_stage: Some(wgpu::ProgrammableStageDescriptor {
                module: &frag_shader,
                entry_point: "main",
            }),
            rasterization_state: Some(wgpu::RasterizationStateDescriptor {
                front_face: wgpu::FrontFace::Cw,
                cull_mode: wgpu::CullMode::Back,
                depth_bias: 0,
                depth_bias_slope_scale: 0.0,
                depth_bias_clamp: 0.0,
            }),
            primitive_topology: wgpu::PrimitiveTopology::TriangleList,
            color_states: &[wgpu::ColorStateDescriptor {
                format: GPU::SCREEN_FORMAT,
                color_blend: wgpu::BlendDescriptor::REPLACE,
                alpha_blend: wgpu::BlendDescriptor::REPLACE,
                write_mask: wgpu::ColorWrite::ALL,
            }],
            depth_stencil_state: Some(wgpu::DepthStencilStateDescriptor {
                format: GPU::DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil_front: wgpu::StencilStateFaceDescriptor::IGNORE,
                stencil_back: wgpu::StencilStateFaceDescriptor::IGNORE,
                stencil_read_mask: 0,
                stencil_write_mask: 0,
            }),
            index_format: wgpu::IndexFormat::Uint16,
            vertex_buffers: &[Vertex::descriptor()],
            sample_count: 1,
            sample_mask: !0,
            alpha_to_coverage_enabled: false,
        }))
}

fn main() -> Fallible<()> {
    let bindings = InputBindings::new("base")
        .bind("exit", "Escape")?
        .bind("exit", "q")?;
    let mut input = InputSystem::new(vec![bindings])?;
    let mut gpu = GPU::new(&input, Default::default())?;

    let omni = OmniLib::new_for_test_in_games(&["FA"])?;
    let lib = omni.library("FA");
    let palette = Palette::from_bytes(&lib.load("PALETTE.PAL")?)?;

    let camera_buffer = CameraParametersBuffer::new(gpu.device())?;
    let mut inst_man = ShapeInstanceManager::new(&gpu.device())?;

    let mut world = World::new();
    world.register::<ShapeComponent>();
    world.register::<ShapeTransformBuffer>();
    world.register::<ShapeFlagBuffer>();
    world.register::<ShapeXformBuffer>();
    world.register::<Transform>();

    const CNT: i32 = 50;
    for x in -CNT / 2..CNT / 2 {
        for y in -CNT / 2..CNT / 2 {
            let (shape_id, slot_id) = inst_man.upload_and_allocate_slot(
                "F18.SH",
                DrawSelection::NormalModel,
                &palette,
                &lib,
                &mut gpu,
            )?;
            let _ent = world
                .create_entity()
                .with(Transform::new(Point3::new(
                    f64::from(x) * 100f64,
                    0f64,
                    f64::from(y) * 100f64,
                )))
                .with(ShapeComponent::new(slot_id, shape_id, DrawState::default()))
                .with(ShapeTransformBuffer::new())
                .with(ShapeFlagBuffer::new(inst_man.errata(shape_id)))
                //.with(ShapeXformBuffer::new())
                .build();
        }
    }

    inst_man.ensure_uploaded(&mut gpu)?;
    gpu.device().poll(true);

    let empty_layout = gpu
        .device()
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { bindings: &[] });
    let empty_bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
        layout: &empty_layout,
        bindings: &[],
    });

    let pipeline = build_pipeline(&mut gpu, &empty_layout, &camera_buffer, &inst_man)?;

    let mut camera = ArcBallCamera::new(gpu.aspect_ratio(), 0.1, 3.4e+38);
    camera.set_distance(1500.0);
    camera.on_mousebutton_down(1);

    let start = Instant::now();
    let mut update_dispatcher = DispatcherBuilder::new()
        .with(TransformUpdateSystem, "transform-update", &[])
        .with(FlagUpdateSystem::new(&start), "flag-update", &[])
        .with(XformUpdateSystem::new(&start), "xform-update", &[])
        .build();

    loop {
        let loop_head = Instant::now();
        for command in input.poll()? {
            match command.name.as_str() {
                "window-resize" => {
                    gpu.note_resize(&input);
                    camera.set_aspect_ratio(gpu.aspect_ratio());
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

        let camera_upload_buffer = camera_buffer.make_upload_buffer(&camera, gpu.device());

        update_dispatcher.dispatch(&world);
        {
            DispatcherBuilder::new()
                .with(CoalesceSystem::new(&mut inst_man), "coalesce", &[])
                .build()
                .dispatch(&world);
        }
        let instance_upload_buffers = inst_man.make_upload_buffer(gpu.device());

        let mut frame = gpu.begin_frame();
        {
            camera_buffer.upload_from(&mut frame, &camera_upload_buffer);
            inst_man.upload_from(&mut frame, &instance_upload_buffers);

            let mut rpass = frame.begin_render_pass();
            rpass.set_pipeline(&pipeline);
            rpass.set_bind_group(0, camera_buffer.bind_group(), &[]);
            rpass.set_bind_group(1, &empty_bind_group, &[]);
            rpass.set_bind_group(2, &empty_bind_group, &[]);
            rpass.set_bind_group(4, &empty_bind_group, &[]);

            for block in inst_man.blocks.values() {
                let chunk = inst_man.chunk_man.chunk(block.chunk_id());

                let f18_part = inst_man.chunk_man.part_for("F18.SH")?;
                let cmd = f18_part.draw_command(0, 1);
                rpass.set_bind_group(3, block.bind_group(), &[]);
                rpass.set_bind_group(5, chunk.bind_group(), &[]);
                rpass.set_vertex_buffers(0, &[(chunk.vertex_buffer(), 0)]);
                rpass.draw(
                    cmd.first_vertex..cmd.first_vertex + cmd.vertex_count,
                    0..block.len() as u32,
                );
            }
        }
        frame.finish();

        println!("frame time: {:?}", loop_head.elapsed());
    }
}
