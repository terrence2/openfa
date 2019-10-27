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
use fullscreen_wgpu::{FullscreenBuffer, FullscreenVertex};
use global_data::GlobalParametersBuffer;
use gpu::GPU;
use input::{InputBindings, InputSystem};
use wgpu;

fn main() -> Fallible<()> {
    let mut input = InputSystem::new(vec![InputBindings::new("base")
        .bind("exit", "Escape")?
        .bind("exit", "q")?])?;
    let mut gpu = GPU::new(&input, Default::default())?;

    let globals_buffer = GlobalParametersBuffer::new(gpu.device())?;
    let fullscreen_buffer = FullscreenBuffer::new(&globals_buffer, gpu.device())?;

    let vert_shader = gpu.create_shader_module(include_bytes!("../target/example.vert.spirv"))?;
    let frag_shader = gpu.create_shader_module(include_bytes!("../target/example.frag.spirv"))?;

    let pipeline_layout = gpu
        .device()
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            bind_group_layouts: &[globals_buffer.bind_group_layout()],
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
            vertex_buffers: &[FullscreenVertex::descriptor()],
            sample_count: 1,
            sample_mask: !0,
            alpha_to_coverage_enabled: false,
        });

    let mut camera = ArcBallCamera::new(gpu.aspect_ratio(), 0.1, 3.4e+38);
    camera.set_distance(40.0);
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

        // Prepare new camera parameters.
        let upload_buffer = globals_buffer.make_upload_buffer(&camera, gpu.device());

        let mut frame = gpu.begin_frame();
        {
            globals_buffer.upload_from(&mut frame, &upload_buffer);

            let mut rpass = frame.begin_render_pass();
            rpass.set_pipeline(&pipeline);
            rpass.set_bind_group(0, globals_buffer.bind_group(), &[]);
            rpass.set_vertex_buffers(0, &[(fullscreen_buffer.vertex_buffer(), 0)]);
            rpass.draw(0..4, 0..1);
        }
        frame.finish();
    }
}
