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
use absolute_unit::meters;
use camera::ArcBallCamera;
use command::Bindings;
use failure::Fallible;
use fullscreen::{FullscreenBuffer, FullscreenVertex};
use global_data::GlobalParametersBuffer;
use gpu::GPU;
use input::InputSystem;
use stars::StarsBuffer;

fn main() -> Fallible<()> {
    let system_bindings = Bindings::new("system")
        .bind("exit", "Escape")?
        .bind("exit", "q")?;
    let mut input = InputSystem::new(vec![ArcBallCamera::default_bindings()?, system_bindings])?;
    let mut gpu = GPU::new(&input, Default::default())?;

    let globals_buffer = GlobalParametersBuffer::new(gpu.device())?;
    let fullscreen_buffer = FullscreenBuffer::new(&gpu)?;
    let stars_buffers = StarsBuffer::new(&gpu)?;

    let vert_shader = gpu.create_shader_module(include_bytes!("../target/example.vert.spirv"))?;
    let frag_shader = gpu.create_shader_module(include_bytes!("../target/example.frag.spirv"))?;

    let empty_layout = gpu
        .device()
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("empty-bind-group-layout"),
            bindings: &[],
        });
    let empty_bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("empty-bind-group"),
        layout: &empty_layout,
        bindings: &[],
    });

    let pipeline_layout = gpu
        .device()
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            bind_group_layouts: &[
                globals_buffer.borrow().bind_group_layout(),
                &empty_layout,
                stars_buffers.borrow().bind_group_layout(),
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
            primitive_topology: wgpu::PrimitiveTopology::TriangleStrip,
            color_states: &[wgpu::ColorStateDescriptor {
                format: GPU::texture_format(),
                color_blend: wgpu::BlendDescriptor::REPLACE,
                alpha_blend: wgpu::BlendDescriptor::REPLACE,
                write_mask: wgpu::ColorWrite::ALL,
            }],
            depth_stencil_state: Some(wgpu::DepthStencilStateDescriptor {
                format: GPU::DEPTH_FORMAT,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Less,
                stencil_front: wgpu::StencilStateFaceDescriptor::IGNORE,
                stencil_back: wgpu::StencilStateFaceDescriptor::IGNORE,
                stencil_read_mask: 0,
                stencil_write_mask: 0,
            }),
            vertex_state: wgpu::VertexStateDescriptor {
                index_format: wgpu::IndexFormat::Uint16,
                vertex_buffers: &[FullscreenVertex::descriptor()],
            },
            sample_count: 1,
            sample_mask: !0,
            alpha_to_coverage_enabled: false,
        });

    let mut arcball = ArcBallCamera::new(gpu.aspect_ratio(), meters!(0.1), meters!(3.4e+38));
    arcball.set_distance(meters!(40.0));

    loop {
        for command in input.poll()? {
            arcball.handle_command(&command)?;
            match command.name.as_str() {
                "window-close" | "window-destroy" | "exit" => return Ok(()),
                "window-resize" => {
                    gpu.note_resize(&input);
                    arcball.camera_mut().set_aspect_ratio(gpu.aspect_ratio());
                }
                "window-cursor-move" => {}
                _ => {}
            }
        }

        // Prepare new camera parameters.
        let mut tracker = Default::default();
        globals_buffer
            .borrow()
            .make_upload_buffer(arcball.camera(), &gpu, &mut tracker)?;

        let gb_borrow = globals_buffer.borrow();
        let fs_borrow = fullscreen_buffer.borrow();
        let sb_borrow = stars_buffers.borrow();
        let mut frame = gpu.begin_frame()?;
        {
            for desc in tracker.drain_uploads() {
                frame.copy_buffer_to_buffer(
                    &desc.source,
                    desc.source_offset,
                    &desc.destination,
                    desc.destination_offset,
                    desc.copy_size,
                );
            }

            let mut rpass = frame.begin_render_pass();
            rpass.set_pipeline(&pipeline);
            rpass.set_bind_group(0, gb_borrow.bind_group(), &[]);
            rpass.set_bind_group(1, &empty_bind_group, &[]);
            rpass.set_bind_group(2, sb_borrow.bind_group(), &[]);
            rpass.set_vertex_buffer(0, fs_borrow.vertex_buffer(), 0, 0);
            rpass.draw(0..4, 0..1);
        }
        frame.finish();
    }
}
