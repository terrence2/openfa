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
use gpu::GPU;
use log::trace;
use shader_globals::Group;
use text_layout::{LayoutBuffer, LayoutVertex};

pub struct ScreenTextRenderPass {
    pipeline: wgpu::RenderPipeline,
}

impl ScreenTextRenderPass {
    pub fn new(gpu: &mut GPU, layout_buffer: &LayoutBuffer) -> Fallible<Self> {
        trace!("ScreenTextRenderPass::new");

        let vert_shader =
            gpu.create_shader_module(include_bytes!("../target/screen_text.vert.spirv"))?;
        let frag_shader =
            gpu.create_shader_module(include_bytes!("../target/screen_text.frag.spirv"))?;

        let pipeline_layout =
            gpu.device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    bind_group_layouts: &[
                        layout_buffer.glyph_bind_group_layout(),
                        layout_buffer.layout_bind_group_layout(),
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
                    cull_mode: wgpu::CullMode::None,
                    depth_bias: 0,
                    depth_bias_slope_scale: 0.0,
                    depth_bias_clamp: 0.0,
                }),
                primitive_topology: wgpu::PrimitiveTopology::TriangleList,
                color_states: &[wgpu::ColorStateDescriptor {
                    format: GPU::texture_format(),
                    alpha_blend: wgpu::BlendDescriptor::REPLACE,
                    color_blend: wgpu::BlendDescriptor {
                        src_factor: wgpu::BlendFactor::SrcAlpha,
                        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                        operation: wgpu::BlendOperation::Add,
                    },
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
                vertex_buffers: &[LayoutVertex::descriptor()],
                sample_count: 1,
                sample_mask: !0,
                alpha_to_coverage_enabled: false,
            });

        Ok(Self { pipeline })
    }

    pub fn draw(&self, rpass: &mut wgpu::RenderPass, layout_buffer: &LayoutBuffer) {
        rpass.set_pipeline(&self.pipeline);
        for (&font, layouts) in layout_buffer.layouts() {
            let glyph_cache = layout_buffer.glyph_cache(font);
            rpass.set_bind_group(Group::GlyphCache.index(), &glyph_cache.bind_group(), &[]);
            for layout in layouts {
                rpass.set_bind_group(Group::TextLayout.index(), &layout.bind_group(), &[]);

                rpass.set_index_buffer(&layout.index_buffer(), 0);
                rpass.set_vertex_buffers(0, &[(&layout.vertex_buffer(), 0)]);
                rpass.draw_indexed(layout.index_range(), 0, 0..1);
            }
        }
    }
}
