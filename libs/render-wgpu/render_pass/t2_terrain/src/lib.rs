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
use gpu::Gpu;
use log::trace;
use ofa_groups::Group as OfaGroup;
use shader_shared::Group;
use t2_buffer::{T2Buffer, T2Vertex};

pub struct T2TerrainRenderPass {
    pipeline: wgpu::RenderPipeline,
}

impl T2TerrainRenderPass {
    pub fn new(
        gpu: &mut Gpu,
        globals_buffer: &GlobalParametersBuffer,
        atmosphere_buffer: &AtmosphereBuffer,
        t2_buffer: &T2Buffer,
    ) -> Result<Self> {
        trace!("T2TerrainRenderPass::new");

        let vert_shader = gpu.create_shader_module(
            "t2_terrain.vert",
            include_bytes!("../target/t2_terrain.vert.spirv"),
        )?;
        let frag_shader = gpu.create_shader_module(
            "t2_terrain.frag",
            include_bytes!("../target/t2_terrain.frag.spirv"),
        )?;

        let pipeline_layout =
            gpu.device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("t2-terrain-render-pipeline-layout"),
                    push_constant_ranges: &[],
                    bind_group_layouts: &[
                        globals_buffer.bind_group_layout(),
                        atmosphere_buffer.bind_group_layout(),
                        t2_buffer.bind_group_layout(),
                    ],
                });

        let pipeline = gpu
            .device()
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("t2-terrain-render-pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &vert_shader,
                    entry_point: "main",
                    buffers: &[T2Vertex::descriptor()],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &frag_shader,
                    entry_point: "main",
                    targets: &[wgpu::ColorTargetState {
                        format: Gpu::SCREEN_FORMAT,
                        color_blend: wgpu::BlendState {
                            src_factor: wgpu::BlendFactor::SrcAlpha,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha_blend: wgpu::BlendState::REPLACE,
                        write_mask: wgpu::ColorWrite::ALL,
                    }],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleStrip,
                    strip_index_format: Some(wgpu::IndexFormat::Uint32),
                    front_face: wgpu::FrontFace::Cw,
                    cull_mode: wgpu::CullMode::Back,
                    polygon_mode: wgpu::PolygonMode::Fill,
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: Gpu::DEPTH_FORMAT,
                    depth_write_enabled: true,
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
        t2_buffer: &'a T2Buffer,
    ) -> wgpu::RenderPass<'a> {
        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(Group::Globals.index(), &globals_buffer.bind_group(), &[]);
        rpass.set_bind_group(
            Group::Atmosphere.index(),
            &atmosphere_buffer.bind_group(),
            &[],
        );
        rpass.set_bind_group(OfaGroup::T2Terrain.index(), &t2_buffer.bind_group(), &[]);
        rpass.set_index_buffer(t2_buffer.index_buffer(), wgpu::IndexFormat::Uint32);
        rpass.set_vertex_buffer(0, t2_buffer.vertex_buffer());
        rpass.draw_indexed(t2_buffer.index_range(), 0, 0..1);
        rpass
    }
}
