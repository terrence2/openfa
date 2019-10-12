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
use atmosphere_wgpu::AtmosphereBuffer;
use camera::ArcBallCamera;
use failure::Fallible;
use gpu::GPU;
use input::{InputBindings, InputSystem};
use log::trace;
use nalgebra::{Unit, UnitQuaternion, Vector3};
use raymarching::{RaymarchingBuffer, RaymarchingVertex};
use std::{f64::consts::PI, time::Instant};
use wgpu;

fn main() -> Fallible<()> {
    let mut input = InputSystem::new(vec![InputBindings::new("base")
        .bind("+enter-move-sun", "mouse1")?
        .bind("exit", "Escape")?
        .bind("exit", "q")?])?;
    let mut gpu = GPU::new(&input, Default::default())?;

    let raymarching_buffer = RaymarchingBuffer::new(gpu.device())?;
    let atmosphere_buffer = AtmosphereBuffer::new(&mut gpu)?;

    let vert_shader = gpu.create_shader_module(include_bytes!("../target/example.vert.spirv"))?;
    let frag_shader = gpu.create_shader_module(include_bytes!("../target/example.frag.spirv"))?;

    let poll_start = Instant::now();
    gpu.device().poll(true);
    println!("poll time: {:?}", poll_start.elapsed());

    let pipeline_layout = gpu
        .device()
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            bind_group_layouts: &[
                raymarching_buffer.bind_group_layout(),
                atmosphere_buffer.bind_group_layout(),
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
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: wgpu::CullMode::Back,
                depth_bias: 0,
                depth_bias_slope_scale: 0.0,
                depth_bias_clamp: 0.0,
            }),
            primitive_topology: wgpu::PrimitiveTopology::TriangleStrip,
            color_states: &[wgpu::ColorStateDescriptor {
                format: GPU::texture_format(),
                color_blend: wgpu::BlendDescriptor::REPLACE,
                alpha_blend: wgpu::BlendDescriptor::REPLACE,
                write_mask: wgpu::ColorWrite::ALL,
            }],
            depth_stencil_state: None,
            index_format: wgpu::IndexFormat::Uint16,
            vertex_buffers: &[RaymarchingVertex::descriptor()],
            sample_count: 1,
            sample_mask: !0,
            alpha_to_coverage_enabled: false,
        });

    let mut camera = ArcBallCamera::new(gpu.aspect_ratio(), 0.1, 3.4e+38);
    camera.set_target(6_378.2, 0.0, 0.0);
    camera.set_angle(PI / 2.0, -PI / 2.0);
    camera.set_up(-Vector3::x());
    camera.set_rotation(UnitQuaternion::from_axis_angle(
        &Unit::new_normalize(Vector3::z()),
        PI / 2.0,
    ));
    camera.set_distance(0.1);
    camera.on_mousebutton_down(1);
    let mut sun_angle = 0f64;
    let mut in_sun_move = false;

    loop {
        let frame_start = Instant::now();
        for command in input.poll()? {
            match command.name.as_str() {
                "window-resize" => {
                    gpu.note_resize(&input);
                    camera.set_aspect_ratio(gpu.aspect_ratio());
                }
                "window-close" | "window-destroy" | "exit" => return Ok(()),
                "+enter-move-sun" => in_sun_move = true,
                "-enter-move-sun" => in_sun_move = false,
                "mouse-move" => {
                    if in_sun_move {
                        sun_angle += command.displacement()?.0 / (180.0 * 2.0);
                    } else {
                        camera.on_mousemove(
                            command.displacement()?.0 / 4.0,
                            command.displacement()?.1 / 4.0,
                        )
                    }
                }
                "window-cursor-move" => {}
                _ => trace!("unhandled command: {}", command.name),
            }
        }

        // Prepare new camera parameters.
        let raymarching_upload_buffer =
            raymarching_buffer.make_upload_buffer(&camera, gpu.device());
        let sun_direction = Vector3::new(sun_angle.sin() as f32, 0f32, sun_angle.cos() as f32);
        let atmosphere_upload_buffer =
            atmosphere_buffer.make_upload_buffer(&camera, sun_direction, gpu.device());

        {
            let mut frame = gpu.begin_frame();
            {
                raymarching_buffer.upload_from(&mut frame, &raymarching_upload_buffer);
                atmosphere_buffer.upload_from(&mut frame, &atmosphere_upload_buffer);

                let mut rpass = frame.begin_render_pass();
                rpass.set_pipeline(&pipeline);
                rpass.set_bind_group(0, raymarching_buffer.bind_group(), &[]);
                rpass.set_bind_group(1, &atmosphere_buffer.bind_group(), &[]);
                rpass.set_vertex_buffers(0, &[(raymarching_buffer.vertex_buffer(), 0)]);
                rpass.draw(0..4, 0..1);
            }
            frame.finish();
        }

        println!("frame time: {:?}", frame_start.elapsed());
    }
}
