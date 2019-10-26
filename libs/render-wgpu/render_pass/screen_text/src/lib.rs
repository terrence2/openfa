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
use failure::{ensure, Fallible};
use fnt::Fnt;
use gpu::GPU;
use lib::Library;
use log::trace;
//use nalgebra::{Matrix4, Vector3};
use std::{cell::RefCell, collections::HashMap, mem, rc::Rc, sync::Arc};
use text_layout::{LayoutBuffer, LayoutVertex};

/*
impl vs::ty::PushConstantData {
    fn new(m: Matrix4<f32>, c: &[f32; 4]) -> Self {
        Self {
            projection: [
                [m[0], m[1], m[1], m[3]],
                [m[4], m[5], m[6], m[7]],
                [m[8], m[9], m[7], m[11]],
                [m[12], m[13], m[14], m[15]],
            ],
            color: *c,
        }
    }
}
*/

pub struct ScreenTextRenderPass {
    pipeline: wgpu::RenderPipeline,
}

impl ScreenTextRenderPass {
    pub fn new(lib: &Arc<Box<Library>>, gpu: &mut GPU) -> Fallible<Self> {
        trace!("ScreenTextRenderPass::new");

        let layout_buffer = LayoutBuffer::new(gpu)?;

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
                vertex_buffers: &[Vertex::descriptor()],
                sample_count: 1,
                sample_mask: !0,
                alpha_to_coverage_enabled: false,
            });

        Ok(Self { pipeline })
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

#[cfg(test)]
mod test {
    use super::*;
    use omnilib::OmniLib;

    #[test]
    fn it_can_render_text() -> Fallible<()> {
        let mut input = InputSystem::new(vec![]);
        let mut gpu = GPU::new(&input, Default::default())?;
        gpu.set_clear_color(&[0f32, 0f32, 0f32, 1f32]);

        let omni = OmniLib::new_for_test_in_games(&[
            "USNF", "MF", "ATF", "ATFNATO", "ATFGOLD", "USNF97", "FA",
        ])?;
        for (game, lib) in omni.libraries() {
            println!("At: {}", game);

            let mut renderer = ScreenTextRenderPass::new(&lib, &window)?;

            renderer
                .add_screen_text(Font::HUD11, "Top Left (r)", &window)?
                .with_color(&[1f32, 0f32, 0f32, 1f32])
                .with_horizontal_position(TextPositionH::Left)
                .with_horizontal_anchor(TextAnchorH::Left)
                .with_vertical_position(TextPositionV::Top)
                .with_vertical_anchor(TextAnchorV::Top);

            renderer
                .add_screen_text(Font::HUD11, "Top Right (b)", &window)?
                .with_color(&[0f32, 0f32, 1f32, 1f32])
                .with_horizontal_position(TextPositionH::Right)
                .with_horizontal_anchor(TextAnchorH::Right)
                .with_vertical_position(TextPositionV::Top)
                .with_vertical_anchor(TextAnchorV::Top);

            renderer
                .add_screen_text(Font::HUD11, "Bottom Left (w)", &window)?
                .with_color(&[1f32, 1f32, 1f32, 1f32])
                .with_horizontal_position(TextPositionH::Left)
                .with_horizontal_anchor(TextAnchorH::Left)
                .with_vertical_position(TextPositionV::Bottom)
                .with_vertical_anchor(TextAnchorV::Bottom);

            renderer
                .add_screen_text(Font::HUD11, "Bottom Right (m)", &window)?
                .with_color(&[1f32, 0f32, 1f32, 1f32])
                .with_horizontal_position(TextPositionH::Right)
                .with_horizontal_anchor(TextAnchorH::Right)
                .with_vertical_position(TextPositionV::Bottom)
                .with_vertical_anchor(TextAnchorV::Bottom);

            let handle_clr = renderer
                .add_screen_text(Font::HUD11, "", &window)?
                .with_span("THR: AFT  1.0G   2462   LCOS   740 M61", &window)?
                .with_color(&[1f32, 0f32, 0f32, 1f32])
                .with_horizontal_position(TextPositionH::Center)
                .with_horizontal_anchor(TextAnchorH::Center)
                .with_vertical_position(TextPositionV::Bottom)
                .with_vertical_anchor(TextAnchorV::Bottom);

            let handle_fin = renderer
                .add_screen_text(Font::HUD11, "DONE: 0%", &window)?
                .with_color(&[0f32, 1f32, 0f32, 1f32])
                .with_horizontal_position(TextPositionH::Center)
                .with_horizontal_anchor(TextAnchorH::Center)
                .with_vertical_position(TextPositionV::Center)
                .with_vertical_anchor(TextAnchorV::Center);

            for i in 0..32 {
                if i < 16 {
                    handle_clr.set_color(&[0f32, i as f32 / 16f32, 0f32, 1f32])
                } else {
                    handle_clr.set_color(&[
                        (i as f32 - 16f32) / 16f32,
                        1f32,
                        (i as f32 - 16f32) / 16f32,
                        1f32,
                    ])
                };
                let msg = format!("DONE: {}%", ((i as f32 / 32f32) * 100f32) as u32);
                handle_fin.set_span(&msg, &window)?;

                {
                    let frame = window.begin_frame()?;
                    if !frame.is_valid() {
                        continue;
                    }

                    renderer.before_frame(&window)?;

                    let mut cbb = AutoCommandBufferBuilder::primary_one_time_submit(
                        window.device(),
                        window.queue().family(),
                    )?;

                    cbb = cbb.begin_render_pass(
                        frame.framebuffer(&window),
                        false,
                        vec![[0f32, 0f32, 0f32, 1f32].into(), 0f32.into()],
                    )?;

                    cbb = renderer.render(cbb, &window.dynamic_state)?;

                    cbb = cbb.end_render_pass()?;

                    let cb = cbb.build()?;

                    frame.submit(cb, &mut window)?;
                }
            }
        }
        std::mem::drop(window);
        Ok(())
    }

    #[test]
    fn it_can_render_without_a_library() -> Fallible<()> {
        let mut input = InputSystem::new(vec![]);
        let mut gpu = GPU::new(&input, Default::default())?;
        gpu.set_clear_color(&[0f32, 0f32, 0f32, 1f32]);

        let lib = Arc::new(Box::new(Library::empty()?));
        let mut renderer = ScreenTextRenderPass::new(&lib, &window)?;

        renderer
            .add_screen_text(Font::QUANTICO, "Top Left (r)", &window)?
            .with_color(&[1f32, 0f32, 0f32, 1f32])
            .with_horizontal_position(TextPositionH::Left)
            .with_horizontal_anchor(TextAnchorH::Left)
            .with_vertical_position(TextPositionV::Top)
            .with_vertical_anchor(TextAnchorV::Top);

        renderer
            .add_screen_text(Font::QUANTICO, "Top Right (b)", &window)?
            .with_color(&[0f32, 0f32, 1f32, 1f32])
            .with_horizontal_position(TextPositionH::Right)
            .with_horizontal_anchor(TextAnchorH::Right)
            .with_vertical_position(TextPositionV::Top)
            .with_vertical_anchor(TextAnchorV::Top);

        renderer
            .add_screen_text(Font::QUANTICO, "Bottom Left (w)", &window)?
            .with_color(&[1f32, 1f32, 1f32, 1f32])
            .with_horizontal_position(TextPositionH::Left)
            .with_horizontal_anchor(TextAnchorH::Left)
            .with_vertical_position(TextPositionV::Bottom)
            .with_vertical_anchor(TextAnchorV::Bottom);

        renderer
            .add_screen_text(Font::QUANTICO, "Bottom Right (m)", &window)?
            .with_color(&[1f32, 0f32, 1f32, 1f32])
            .with_horizontal_position(TextPositionH::Right)
            .with_horizontal_anchor(TextAnchorH::Right)
            .with_vertical_position(TextPositionV::Bottom)
            .with_vertical_anchor(TextAnchorV::Bottom);

        let handle_clr = renderer
            .add_screen_text(Font::QUANTICO, "", &window)?
            .with_span("THR: AFT  1.0G   2462   LCOS   740 M61", &window)?
            .with_color(&[1f32, 0f32, 0f32, 1f32])
            .with_horizontal_position(TextPositionH::Center)
            .with_horizontal_anchor(TextAnchorH::Center)
            .with_vertical_position(TextPositionV::Bottom)
            .with_vertical_anchor(TextAnchorV::Bottom);

        let handle_fin = renderer
            .add_screen_text(Font::QUANTICO, "DONE: 0%", &window)?
            .with_color(&[0f32, 1f32, 0f32, 1f32])
            .with_horizontal_position(TextPositionH::Center)
            .with_horizontal_anchor(TextAnchorH::Center)
            .with_vertical_position(TextPositionV::Center)
            .with_vertical_anchor(TextAnchorV::Center);

        for i in 0..32 {
            if i < 16 {
                handle_clr.set_color(&[0f32, i as f32 / 16f32, 0f32, 1f32])
            } else {
                handle_clr.set_color(&[
                    (i as f32 - 16f32) / 16f32,
                    1f32,
                    (i as f32 - 16f32) / 16f32,
                    1f32,
                ])
            };
            let msg = format!("DONE: {}%", ((i as f32 / 32f32) * 100f32) as u32);
            handle_fin.set_span(&msg, &window)?;

            {
                let frame = window.begin_frame()?;
                if !frame.is_valid() {
                    continue;
                }

                renderer.before_frame(&window)?;

                let mut cbb = AutoCommandBufferBuilder::primary_one_time_submit(
                    window.device(),
                    window.queue().family(),
                )?;

                cbb = cbb.begin_render_pass(
                    frame.framebuffer(&window),
                    false,
                    vec![[0f32, 0f32, 0f32, 1f32].into(), 0f32.into()],
                )?;

                cbb = renderer.render(cbb, &window.dynamic_state)?;

                cbb = cbb.end_render_pass()?;

                let cb = cbb.build()?;

                frame.submit(cb, &mut window)?;
            }
        }
        std::mem::drop(window);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
