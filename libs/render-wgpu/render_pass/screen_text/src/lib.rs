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
use text_layout::{LayoutBuffer, LayoutVertex};

pub struct ScreenTextRenderPass {
    _pipeline: wgpu::RenderPipeline,
}

impl ScreenTextRenderPass {
    pub fn new(layout_buffer: LayoutBuffer, gpu: &mut GPU) -> Fallible<Self> {
        trace!("ScreenTextRenderPass::new");

        let vert_shader =
            gpu.create_shader_module(include_bytes!("../target/screen_text.vert.spirv"))?;
        let frag_shader =
            gpu.create_shader_module(include_bytes!("../target/screen_text.frag.spirv"))?;

        let pipeline_layout =
            gpu.device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    bind_group_layouts: &[layout_buffer.bind_group_layout()],
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
                primitive_topology: wgpu::PrimitiveTopology::TriangleList,
                color_states: &[wgpu::ColorStateDescriptor {
                    format: GPU::texture_format(),
                    color_blend: wgpu::BlendDescriptor::REPLACE,
                    alpha_blend: wgpu::BlendDescriptor::REPLACE,
                    write_mask: wgpu::ColorWrite::ALL,
                }],
                depth_stencil_state: None,
                index_format: wgpu::IndexFormat::Uint16,
                vertex_buffers: &[LayoutVertex::descriptor()],
                sample_count: 1,
                sample_mask: !0,
                alpha_to_coverage_enabled: false,
            });

        Ok(Self {
            _pipeline: pipeline,
        })
    }

    /*
    pub fn before_frame(&mut self, window: &GraphicsWindow) -> Fallible<()> {
        self.set_projection(&window)
    }

    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder
    ) -> Fallible<AutoCommandBufferBuilder> {
        let mut render_pass = encoder.
        let mut cb = cb;
        for layout in &self.layouts {
            cb = cb.draw_indexed(
                self.screen_pipeline.clone(),
                dynamic_state,
                vec![layout.vertex_buffer()],
                layout.index_buffer(),
                layout.pds(),
                layout.push_consts()?,
            )?;
        }

        Ok(cb)
    }
    */
}
