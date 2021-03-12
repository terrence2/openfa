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
use anyhow::Result;
use atmosphere::AtmosphereBuffer;
use global_data::GlobalParametersBuffer;
use gpu::GPU;
use ofa_groups::Group as LocalGroup;
use shader_shared::Group;
use shape_chunk::Vertex;
use shape_instance::ShapeInstanceBuffer;

pub struct ShapeRenderPass {
    pipeline: wgpu::RenderPipeline,
}

impl ShapeRenderPass {
    pub fn new(
        gpu: &GPU,
        globals_buffer: &GlobalParametersBuffer,
        atmosphere_buffer: &AtmosphereBuffer,
        inst_man: &ShapeInstanceBuffer,
    ) -> Result<Self> {
        let vert_shader =
            gpu.create_shader_module("shape.vert", include_bytes!("../target/shape.vert.spirv"))?;
        let frag_shader =
            gpu.create_shader_module("shape.frag", include_bytes!("../target/shape.frag.spirv"))?;

        let pipeline_layout =
            gpu.device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("shape-render-pipeline-layout"),
                    push_constant_ranges: &[],
                    bind_group_layouts: &[
                        globals_buffer.bind_group_layout(),
                        atmosphere_buffer.bind_group_layout(),
                        inst_man.chunk_man.bind_group_layout(),
                        inst_man.bind_group_layout(),
                    ],
                });

        let pipeline = gpu
            .device()
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("shape-render-pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &vert_shader,
                    entry_point: "main",
                    buffers: &[Vertex::descriptor()],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &frag_shader,
                    entry_point: "main",
                    targets: &[wgpu::ColorTargetState {
                        format: GPU::SCREEN_FORMAT,
                        color_blend: wgpu::BlendState::REPLACE,
                        alpha_blend: wgpu::BlendState::REPLACE,
                        write_mask: wgpu::ColorWrite::ALL,
                    }],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Cw,
                    cull_mode: wgpu::CullMode::Back,
                    polygon_mode: wgpu::PolygonMode::Fill,
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: GPU::DEPTH_FORMAT,
                    depth_write_enabled: true,
                    // FIXME: do we need to swap this for inverted depth?
                    depth_compare: wgpu::CompareFunction::Less,
                    stencil: wgpu::StencilState {
                        front: wgpu::StencilFaceState::IGNORE,
                        back: wgpu::StencilFaceState::IGNORE,
                        read_mask: 0,
                        write_mask: 0,
                    },
                    bias: wgpu::DepthBiasState {
                        constant: 0,
                        slope_scale: 0.0,
                        clamp: 0.0,
                    },
                    clamp_depth: false,
                }),
                multisample: wgpu::MultisampleState {
                    count: 1,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
            });

        Ok(Self { pipeline })
    }

    pub fn draw<'a>(
        &'a self,
        mut rpass: wgpu::RenderPass<'a>,
        globals_buffer: &'a GlobalParametersBuffer,
        atmosphere_buffer: &'a AtmosphereBuffer,
        shape_instance_buffer: &'a ShapeInstanceBuffer,
    ) -> Result<wgpu::RenderPass<'a>> {
        assert_ne!(LocalGroup::ShapeChunk.index(), Group::Globals.index());
        assert_ne!(LocalGroup::ShapeChunk.index(), Group::Atmosphere.index());
        assert_ne!(LocalGroup::ShapeBlock.index(), Group::Globals.index());
        assert_ne!(LocalGroup::ShapeBlock.index(), Group::Atmosphere.index());
        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(Group::Globals.index(), globals_buffer.bind_group(), &[]);
        rpass.set_bind_group(
            Group::Atmosphere.index(),
            atmosphere_buffer.bind_group(),
            &[],
        );

        for block in shape_instance_buffer.blocks.values() {
            let chunk = shape_instance_buffer.chunk_man.chunk(block.chunk_id());

            // FIXME: reorganize blocks by chunk so that we can avoid thrashing this bind group
            rpass.set_bind_group(LocalGroup::ShapeChunk.index(), chunk.bind_group(), &[]);
            rpass.set_bind_group(LocalGroup::ShapeBlock.index(), block.bind_group(), &[]);
            rpass.set_vertex_buffer(0, chunk.vertex_buffer());
            for i in 0..block.len() {
                //rpass.draw_indirect(block.command_buffer(), i as u64);
                let cmd = block.command_buffer_scratch[i];
                #[allow(clippy::range_plus_one)]
                rpass.draw(
                    cmd.first_vertex..cmd.first_vertex + cmd.vertex_count,
                    i as u32..i as u32 + 1,
                );
            }
        }
        Ok(rpass)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nitrous::Interpreter;
    use winit::{event_loop::EventLoop, window::Window};

    #[cfg(unix)]
    #[test]
    fn it_works() -> Result<()> {
        use winit::platform::unix::EventLoopExtUnix;
        let event_loop = EventLoop::<()>::new_any_thread();
        let window = Window::new(&event_loop)?;
        let interpreter = Interpreter::new();
        let gpu = GPU::new(&window, Default::default(), &mut interpreter.write())?;
        let atmosphere_buffer = AtmosphereBuffer::new(false, &mut gpu.write())?;
        let globals_buffer =
            GlobalParametersBuffer::new(gpu.read().device(), &mut interpreter.write());
        let inst_man = ShapeInstanceBuffer::new(gpu.read().device())?;
        let _ = ShapeRenderPass::new(
            &gpu.read(),
            &globals_buffer.read(),
            &atmosphere_buffer,
            &inst_man,
        )?;

        Ok(())
    }
}
