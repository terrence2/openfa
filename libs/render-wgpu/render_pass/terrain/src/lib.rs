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
use atmosphere::AtmosphereBuffer;
use failure::Fallible;
use global_data::GlobalParametersBuffer;
use gpu::GPU;
use log::trace;
use shader_globals::Group;
use terrain_geo::{PatchVertex, TerrainGeoBuffer, Vertex as TerrainVertex};
use wgpu;

pub struct TerrainRenderPass {
    debug_patch_pipeline: wgpu::RenderPipeline,
    pipeline: wgpu::RenderPipeline,
}

impl TerrainRenderPass {
    pub fn new(
        gpu: &mut GPU,
        globals_buffer: &GlobalParametersBuffer,
        atmosphere_buffer: &AtmosphereBuffer,
        terrain_geo_buffer: &TerrainGeoBuffer,
    ) -> Fallible<Self> {
        trace!("TerrainRenderPass::new");

        let vert_shader =
            gpu.create_shader_module(include_bytes!("../target/terrain.vert.spirv"))?;
        let frag_shader =
            gpu.create_shader_module(include_bytes!("../target/terrain.frag.spirv"))?;

        let debug_patch_pipeline =
            gpu.device()
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    layout: &gpu
                        .device()
                        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                            bind_group_layouts: &[globals_buffer.bind_group_layout()],
                        }),
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
                        cull_mode: wgpu::CullMode::None,
                        depth_bias: 0,
                        depth_bias_slope_scale: 0.0,
                        depth_bias_clamp: 0.0,
                    }),
                    primitive_topology: wgpu::PrimitiveTopology::LineList,
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
                    index_format: wgpu::IndexFormat::Uint32,
                    vertex_buffers: &[PatchVertex::descriptor()],
                    sample_count: 1,
                    sample_mask: !0,
                    alpha_to_coverage_enabled: false,
                });

        let pipeline_layout =
            gpu.device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    bind_group_layouts: &[
                        globals_buffer.bind_group_layout(),
                        atmosphere_buffer.bind_group_layout(),
                        // terrain_geo_buffer.bind_group_layout(),
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
                    cull_mode: wgpu::CullMode::None,
                    depth_bias: 0,
                    depth_bias_slope_scale: 0.0,
                    depth_bias_clamp: 0.0,
                }),
                primitive_topology: wgpu::PrimitiveTopology::LineList,
                color_states: &[wgpu::ColorStateDescriptor {
                    format: GPU::texture_format(),
                    //                    color_blend: wgpu::BlendDescriptor {
                    //                        src_factor: wgpu::BlendFactor::SrcAlpha,
                    //                        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                    //                        operation: wgpu::BlendOperation::Add,
                    //                    },
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
                index_format: wgpu::IndexFormat::Uint32,
                vertex_buffers: &[TerrainVertex::descriptor()],
                sample_count: 1,
                sample_mask: !0,
                alpha_to_coverage_enabled: false,
            });

        Ok(Self {
            debug_patch_pipeline,
            pipeline,
        })
    }

    pub fn draw(
        &self,
        rpass: &mut wgpu::RenderPass,
        globals_buffer: &GlobalParametersBuffer,
        atmosphere_buffer: &AtmosphereBuffer,
        terrain_geo_buffer: &TerrainGeoBuffer,
    ) {
        rpass.set_pipeline(&self.debug_patch_pipeline);
        rpass.set_bind_group(Group::Globals.index(), &globals_buffer.bind_group(), &[]);
        /*
        rpass.set_bind_group(
            Group::Atmosphere.index(),
            &atmosphere_buffer.bind_group(),
            &[],
        );
        rpass.set_bind_group(
            Group::Terrain.index(),
            &terrain_geo_buffer.block_bind_group(),
            &[],
        );
        */
        rpass.set_index_buffer(terrain_geo_buffer.patch_index_buffer(), 0);
        rpass.set_vertex_buffers(0, &[(terrain_geo_buffer.patch_vertex_buffer(), 0)]);
        for i in 0..terrain_geo_buffer.num_patches() {
            rpass.draw_indexed(terrain_geo_buffer.patch_index_range(), i * 3, 0..1);
        }
    }
}
