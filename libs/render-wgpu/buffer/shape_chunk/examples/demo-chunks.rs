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
use camera_parameters::CameraParametersBuffer;
use failure::Fallible;
//use global_layout::GlobalSets;
use gpu;
use gpu::GPU;
use input::{InputBindings, InputSystem};
use omnilib::OmniLib;
use pal::Palette;
use shape_chunk_wgpu::{DrawSelection, DrawState, ShapeChunkManager, Vertex};
use std::time::Instant;

fn main() -> Fallible<()> {
    let omni = OmniLib::new_for_test_in_games(&["FA"])?;
    let lib = omni.library("FA");
    let palette = Palette::from_bytes(&lib.load("PALETTE.PAL")?)?;

    let bindings = InputBindings::new("base")
        .bind("exit", "Escape")?
        .bind("exit", "q")?;
    let mut input = InputSystem::new(vec![bindings])?;
    let mut gpu = GPU::new(&input, Default::default())?;

    let camera_buffer = CameraParametersBuffer::new(gpu.device())?;

    let mut chunk_man = ShapeChunkManager::new(gpu.device())?;
    let (_chunk_id, _shape_id) = chunk_man.upload_shape(
        "F8.SH",
        DrawSelection::NormalModel,
        &palette,
        &lib,
        &mut gpu,
    )?;
    let (_chunk_id, _shape_id) = chunk_man.upload_shape(
        "F18.SH",
        DrawSelection::NormalModel,
        &palette,
        &lib,
        &mut gpu,
    )?;
    let (chunk_id, _shape_id) = chunk_man.upload_shape(
        "BNK1.SH",
        DrawSelection::NormalModel,
        &palette,
        &lib,
        &mut gpu,
    )?;
    chunk_man.finish(&mut gpu)?;
    gpu.device().poll(true);

    let f18_part = chunk_man.part_for("F18.SH")?;

    let vert_shader = gpu.create_shader_module(include_bytes!("../target/example.vert.spirv"))?;
    let frag_shader = gpu.create_shader_module(include_bytes!("../target/example.frag.spirv"))?;

    let empty_layout = gpu
        .device()
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { bindings: &[] });
    let empty_bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
        layout: &empty_layout,
        bindings: &[],
    });

    let instance_layout = gpu
        .device()
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            bindings: &[
                wgpu::BindGroupLayoutBinding {
                    binding: 0,
                    visibility: wgpu::ShaderStage::VERTEX,
                    ty: wgpu::BindingType::StorageBuffer {
                        dynamic: false,
                        readonly: true,
                    },
                },
                wgpu::BindGroupLayoutBinding {
                    binding: 1,
                    visibility: wgpu::ShaderStage::VERTEX,
                    ty: wgpu::BindingType::StorageBuffer {
                        dynamic: false,
                        readonly: true,
                    },
                },
            ],
        });

    let pipeline_layout = gpu
        .device()
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            bind_group_layouts: &[
                camera_buffer.bind_group_layout(),
                &empty_layout,
                &empty_layout,
                &instance_layout,
                &empty_layout,
                chunk_man.bind_group_layout(),
            ],
        });
    let pipeline = gpu
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
        });

    // Upload flags
    let mut draw_state: DrawState = Default::default();
    draw_state.toggle_gear(&Instant::now());
    let mut flags_arr = [0u32; 2];
    draw_state.build_mask_into(
        draw_state.time_origin(),
        f18_part.widgets().read().unwrap().errata(),
        &mut flags_arr[0..2],
    )?;
    let flags_buffer = gpu
        .device()
        .create_buffer_mapped(2, wgpu::BufferUsage::all())
        .fill_from_slice(&flags_arr);

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
    let xforms_buffer = gpu
        .device()
        .create_buffer_mapped(xforms_len, wgpu::BufferUsage::all())
        .fill_from_slice(&xforms);

    let instance_bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
        layout: &instance_layout,
        bindings: &[
            wgpu::Binding {
                binding: 0,
                resource: wgpu::BindingResource::Buffer {
                    buffer: &flags_buffer,
                    range: 0..2,
                },
            },
            wgpu::Binding {
                binding: 1,
                resource: wgpu::BindingResource::Buffer {
                    buffer: &xforms_buffer,
                    range: 0..xforms_len as wgpu::BufferAddress,
                },
            },
        ],
    });

    /*
    let indirect_buffer = CpuAccessibleBuffer::from_iter(
        window.device(),
        BufferUsage::all(),
        [f18_part.draw_command(0, 1)].iter().cloned(),
    )?;
    */

    let mut camera = ArcBallCamera::new(gpu.aspect_ratio(), 0.1, 3.4e+38);
    camera.set_distance(80.0);
    camera.on_mousebutton_down(1);

    loop {
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

        let upload_buffer = camera_buffer.make_upload_buffer(&camera, gpu.device());

        let mut frame = gpu.begin_frame();
        {
            camera_buffer.upload_from(&mut frame, &upload_buffer);

            let chunk = chunk_man.chunk(chunk_id);

            let mut rpass = frame.begin_render_pass();
            rpass.set_pipeline(&pipeline);
            rpass.set_bind_group(0, camera_buffer.bind_group(), &[]);
            rpass.set_bind_group(1, &empty_bind_group, &[]);
            rpass.set_bind_group(2, &empty_bind_group, &[]);
            rpass.set_bind_group(3, &instance_bind_group, &[]);
            rpass.set_bind_group(4, &empty_bind_group, &[]);
            rpass.set_bind_group(5, chunk.bind_group(), &[]);
            rpass.set_vertex_buffers(0, &[(chunk.vertex_buffer(), 0)]);
            let cmd = f18_part.draw_command(0, 1);
            rpass.draw(cmd.first_vertex..cmd.first_vertex + cmd.vertex_count, 0..1);
        }
        frame.finish();
    }
}
