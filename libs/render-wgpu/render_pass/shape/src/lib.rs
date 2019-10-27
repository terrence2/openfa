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
use global_data::GlobalParametersBuffer;
use gpu::{Frame, GPU};
use shape_chunk::Vertex;
use shape_instance::ShapeInstanceManager;
use wgpu;

pub struct ShapeRenderPass {
    pipeline: wgpu::RenderPipeline,
}

impl ShapeRenderPass {
    pub fn new(
        gpu: &GPU,
        camera_buffer: &GlobalParametersBuffer,
        inst_man: &ShapeInstanceManager,
    ) -> Fallible<Self> {
        let vert_shader = gpu.create_shader_module(include_bytes!("../target/shape.vert.spirv"))?;
        let frag_shader = gpu.create_shader_module(include_bytes!("../target/shape.frag.spirv"))?;

        let pipeline_layout =
            gpu.device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    bind_group_layouts: &[
                        camera_buffer.bind_group_layout(),
                        &gpu.empty_layout(),
                        &gpu.empty_layout(),
                        inst_man.bind_group_layout(),
                        &gpu.empty_layout(),
                        inst_man.chunk_man.bind_group_layout(),
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

        Ok(Self { pipeline })
    }

    pub fn render(
        &self,
        empty_bind_group: &wgpu::BindGroup,
        camera_buffer: &GlobalParametersBuffer,
        inst_man: &ShapeInstanceManager,
        frame: &mut Frame,
    ) -> Fallible<()> {
        let mut rpass = frame.begin_render_pass();
        rpass.set_pipeline(&self.pipeline);
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

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use input::InputSystem;

    #[test]
    fn it_works() -> Fallible<()> {
        let input = InputSystem::new(vec![])?;
        let gpu = GPU::new(&input, Default::default())?;
        let camera_buffer = GlobalParametersBuffer::new(gpu.device())?;
        let inst_man = ShapeInstanceManager::new(&gpu.device())?;

        let _ = ShapeRenderPass::new(&gpu, &camera_buffer, &inst_man)?;

        Ok(())
    }
}
