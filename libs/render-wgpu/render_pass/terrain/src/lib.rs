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
use terrain_geo::{DebugVertex, PatchVertex, TerrainGeoBuffer};

pub struct TerrainRenderPass {
    debug_patch_pipeline: wgpu::RenderPipeline,

    #[allow(unused)]
    debug_intersect_pipeline: wgpu::RenderPipeline,
}

impl TerrainRenderPass {
    pub fn new(
        gpu: &mut GPU,
        globals_buffer: &GlobalParametersBuffer,
        _atmosphere_buffer: &AtmosphereBuffer,
        _terrain_geo_buffer: &TerrainGeoBuffer,
    ) -> Fallible<Self> {
        trace!("TerrainRenderPass::new");

        let vert_shader =
            gpu.create_shader_module(include_bytes!("../target/terrain.vert.spirv"))?;
        let frag_shader =
            gpu.create_shader_module(include_bytes!("../target/terrain.frag.spirv"))?;

        let dbg_vert_shader =
            gpu.create_shader_module(include_bytes!("../target/debug_intersection.vert.spirv"))?;
        let dbg_frag_shader =
            gpu.create_shader_module(include_bytes!("../target/debug_intersection.frag.spirv"))?;

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
                    vertex_state: wgpu::VertexStateDescriptor {
                        index_format: wgpu::IndexFormat::Uint32,
                        vertex_buffers: &[PatchVertex::descriptor()],
                    },
                    sample_count: 1,
                    sample_mask: !0,
                    alpha_to_coverage_enabled: false,
                });

        let pipeline_layout =
            gpu.device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    bind_group_layouts: &[
                        globals_buffer.bind_group_layout(),
                        // atmosphere_buffer.bind_group_layout(),
                        // terrain_geo_buffer.bind_group_layout(),
                    ],
                });

        let debug_intersect_pipeline =
            gpu.device()
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    layout: &pipeline_layout,
                    vertex_stage: wgpu::ProgrammableStageDescriptor {
                        module: &dbg_vert_shader,
                        entry_point: "main",
                    },
                    fragment_stage: Some(wgpu::ProgrammableStageDescriptor {
                        module: &dbg_frag_shader,
                        entry_point: "main",
                    }),
                    rasterization_state: Some(wgpu::RasterizationStateDescriptor {
                        front_face: wgpu::FrontFace::Ccw,
                        cull_mode: wgpu::CullMode::Back,
                        depth_bias: 0,
                        depth_bias_slope_scale: 0.0,
                        depth_bias_clamp: 0.0,
                    }),
                    primitive_topology: wgpu::PrimitiveTopology::TriangleList,
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
                        depth_write_enabled: true,
                        depth_compare: wgpu::CompareFunction::Less,
                        stencil_front: wgpu::StencilStateFaceDescriptor::IGNORE,
                        stencil_back: wgpu::StencilStateFaceDescriptor::IGNORE,
                        stencil_read_mask: 0,
                        stencil_write_mask: 0,
                    }),
                    vertex_state: wgpu::VertexStateDescriptor {
                        index_format: wgpu::IndexFormat::Uint32,
                        vertex_buffers: &[DebugVertex::descriptor()],
                    },
                    sample_count: 1,
                    sample_mask: !0,
                    alpha_to_coverage_enabled: false,
                });

        Ok(Self {
            debug_patch_pipeline,
            debug_intersect_pipeline,
        })
    }

    pub fn draw<'a>(
        &'a self,
        mut rpass: wgpu::RenderPass<'a>,
        globals_buffer: &'a GlobalParametersBuffer,
        _atmosphere_buffer: &'a AtmosphereBuffer,
        terrain_geo_buffer: &'a TerrainGeoBuffer,
    ) -> wgpu::RenderPass<'a> {
        // rpass.set_pipeline(&self.debug_intersect_pipeline);
        // rpass.set_bind_group(Group::Globals.index(), &globals_buffer.bind_group(), &[]);
        // rpass.set_index_buffer(terrain_geo_buffer.debug_index_buffer(), 0, 0);
        // rpass.set_vertex_buffer(0, &terrain_geo_buffer.debug_vertex_buffer(), 0, 0);
        // //rpass.draw_indexed(terrain_geo_buffer.debug_index_range(), 0, 0..1);
        // rpass.draw(terrain_geo_buffer.debug_index_range(), 0..1);

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
        rpass.set_index_buffer(terrain_geo_buffer.patch_index_buffer(), 0, 0);
        rpass.set_vertex_buffer(0, &terrain_geo_buffer.vertex_buffer(), 0, 0);
        for i in 0..terrain_geo_buffer.num_patches() {
            rpass.draw_indexed(terrain_geo_buffer.patch_index_range(), i * 3, 0..1);
        }
        rpass
    }
}
