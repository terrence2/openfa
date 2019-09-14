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
mod chunk;
mod chunk_manager;
mod draw_state;
mod texture_atlas;
mod upload;

pub use chunk::{Chunk, ChunkId, ChunkPart, ClosedChunk, OpenChunk, ShapeId};
pub use chunk_manager::ShapeChunkManager;
pub use draw_state::DrawState;
pub use upload::{DrawSelection, ShapeErrata, Vertex};

mod test_vs {
    use vulkano_shaders::shader;

    shader! {
    ty: "vertex",
    src: "
        #version 450

        // Per shape input
        layout(set = 4, binding = 0) buffer ChunkFlags {
            uint flag_data[];
        } flags;
        layout(set = 4, binding = 1) buffer ChunkTransforms {
            float xform_data[];
        } xforms;

        void main() {
        }"
    }
}

mod test_fs {
    use vulkano_shaders::shader;

    shader! {
    ty: "fragment",
    src: "
        #version 450

        layout(set = 3, binding = 0) uniform sampler2DArray mega_atlas;

        void main() {
        }"
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use failure::Fallible;
    use omnilib::OmniLib;
    use pal::Palette;
    use std::sync::Arc;
    use vulkano::{
        framebuffer::Subpass,
        pipeline::{
            depth_stencil::{Compare, DepthBounds, DepthStencil},
            GraphicsPipeline, GraphicsPipelineAbstract,
        },
        sync::GpuFuture,
    };
    use window::{GraphicsConfigBuilder, GraphicsWindow};

    #[test]
    fn test_load_all() -> Fallible<()> {
        let window = GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?;

        let vert_shader = test_vs::Shader::load(window.device())?;
        let frag_shader = test_fs::Shader::load(window.device())?;
        let pipeline = Arc::new(
            GraphicsPipeline::start()
                .vertex_input_single_buffer::<Vertex>()
                .vertex_shader(vert_shader.main_entry_point(), ())
                .triangle_list()
                .cull_mode_back()
                .front_face_clockwise()
                .viewports_dynamic_scissors_irrelevant(1)
                .fragment_shader(frag_shader.main_entry_point(), ())
                .depth_stencil(DepthStencil {
                    depth_write: true,
                    depth_compare: Compare::GreaterOrEqual,
                    depth_bounds_test: DepthBounds::Disabled,
                    stencil_front: Default::default(),
                    stencil_back: Default::default(),
                })
                .blend_alpha_blending()
                .render_pass(
                    Subpass::from(window.render_pass(), 0)
                        .expect("gfx: did not find a render pass"),
                )
                .build(window.device())?,
        ) as Arc<dyn GraphicsPipelineAbstract + Send + Sync>;

        let omni = OmniLib::new_for_test_in_games(&["FA"])?;
        let lib = omni.library("FA");
        let palette = Palette::from_bytes(&lib.load("PALETTE.PAL")?)?;

        let mut shapes = lib.find_matching("*.SH")?;
        shapes.sort();
        let skipped = vec![
            "CATGUY.SH",  // 640
            "MOON.SH",    // 41
            "SOLDIER.SH", // 320
            "CHAFF.SH",
            "CRATER.SH",
            "DEBRIS.SH",
            "EXP.SH",
            "FIRE.SH",
            "FLARE.SH",
            "MOTHB.SH",
            "SMOKE.SH",
            "WAVE1.SH",
            "WAVE2.SH",
        ];

        let mut chunk_man = ShapeChunkManager::new(pipeline, &window)?;
        for name in shapes {
            if skipped.contains(&name.as_str()) {
                continue;
            }
            let (_shape_id, _maybe_fut) = chunk_man.upload_shape(
                &name,
                DrawSelection::NormalModel,
                &palette,
                &lib,
                &window,
            )?;
        }
        let future = chunk_man.finish(&window)?;
        future.then_signal_fence_and_flush()?.wait(None)?;
        Ok(())
    }
}
