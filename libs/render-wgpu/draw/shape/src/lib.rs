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
use camera_parameters::CameraParametersBuffer;
use failure::Fallible;
use gpu::GPU;
use shape_chunk::Vertex;
use shape_instance::ShapeInstanceManager;
use wgpu;

pub struct ShapeRenderPass {
    pipeline: wgpu::RenderPipeline,
}

impl ShapeRenderPass {
    pub fn new(
        gpu: &GPU,
        empty_layout: &wgpu::BindGroupLayout,
        camera_buffer: &CameraParametersBuffer,
        inst_man: &ShapeInstanceManager,
    ) -> Fallible<Self> {
        let vert_shader = gpu.create_shader_module(include_bytes!("../target/shape.vert.spirv"))?;
        let frag_shader = gpu.create_shader_module(include_bytes!("../target/shape.frag.spirv"))?;

        let pipeline_layout =
            gpu.device()
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use input::InputSystem;

    #[test]
    fn it_works() -> Fallible<()> {
        let input = InputSystem::new(vec![])?;
        let gpu = GPU::new(&input, Default::default())?;
        let camera_buffer = CameraParametersBuffer::new(gpu.device())?;
        let inst_man = ShapeInstanceManager::new(&gpu.device())?;

        let empty_layout = gpu
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { bindings: &[] });
        let empty_bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &empty_layout,
            bindings: &[],
        });
        let _ = ShapeRenderPass::new(&gpu, &empty_layout, &camera_buffer, &inst_man)?;

        Ok(())
    }
}
