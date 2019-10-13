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

pub use chunk::{ChunkId, ChunkPart, ClosedChunk, OpenChunk, ShapeId};
pub use chunk_manager::ShapeChunkManager;
pub use draw_state::DrawState;
pub use upload::{DrawSelection, ShapeErrata, ShapeWidgets, Vertex};

#[cfg(test)]
mod test {
    use super::*;
    use failure::Fallible;
    use gpu::GPU;
    use input::InputSystem;
    use log::trace;
    use omnilib::OmniLib;
    use pal::Palette;

    #[test]
    fn test_load_all() -> Fallible<()> {
        let mut input = InputSystem::new(vec![])?;
        let mut gpu = GPU::new(&input, Default::default())?;

        let vert_shader =
            gpu.create_shader_module(include_bytes!("../target/example.vert.spirv"))?;
        let frag_shader =
            gpu.create_shader_module(include_bytes!("../target/example.frag.spirv"))?;

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

        let mut chunk_man = ShapeChunkManager::new(gpu.device())?;
        let mut all_shapes = Vec::new();
        for name in shapes {
            if skipped.contains(&name.as_str()) {
                continue;
            }
            let (_chunk_id, shape_id) = chunk_man.upload_shape(
                &name,
                DrawSelection::NormalModel,
                &palette,
                &lib,
                &mut gpu,
            )?;
            all_shapes.push(shape_id);
        }
        chunk_man.finish(&mut gpu)?;
        gpu.device().poll(true);

        for shape_id in &all_shapes {
            let lifetime = chunk_man.part(*shape_id).widgets();
            let widgets = lifetime.read().unwrap();
            trace!("{} - {}", widgets.num_xforms(), widgets.name());
        }

        Ok(())
    }
}
