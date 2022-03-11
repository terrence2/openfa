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
mod chunk_manager;
mod chunks;
mod draw_state;
mod upload;

pub(crate) use chunk_manager::ChunkManager;
pub use chunks::{
    ChunkId, ChunkPart, ClosedChunk, DrawIndirectCommand, OpenChunk, ShapeId, ShapeIds,
};
pub use draw_state::DrawState;
pub use upload::{DrawSelection, ShapeErrata, ShapeWidgets, Vertex};

#[cfg(test)]
mod test {
    use super::*;
    use anyhow::Result;
    use gpu::Gpu;
    use lib::Libs;
    use log::trace;
    use pal::Palette;
    use sh::RawShape;
    use std::collections::HashMap;

    #[cfg(unix)]
    #[test]
    fn test_load_all() -> Result<()> {
        let mut runtime = Gpu::for_test_unix()?;

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

        let libs = Libs::for_testing()?;

        let mut chunk_man = ChunkManager::new(&runtime.resource::<Gpu>())?;
        let mut result_maps = HashMap::new();
        for (game, catalog) in libs.selected() {
            let palette = Palette::from_bytes(catalog.read_name("PALETTE.PAL")?.as_ref())?;
            let mut all_shapes = HashMap::new();
            for fid in catalog.find_with_extension("SH")? {
                let meta = catalog.stat(fid)?;

                println!("At: {}:{:13} @ {}", game.test_dir, meta.name(), meta.path());
                if skipped.contains(&meta.name()) {
                    continue;
                }
                all_shapes.insert(
                    meta.name().to_owned(),
                    RawShape::from_bytes(catalog.read_name(meta.name())?.as_ref())?,
                );
            }
            let results = chunk_man.upload_shapes(
                &palette,
                &all_shapes,
                &catalog,
                &runtime.resource::<Gpu>(),
            )?;
            result_maps.insert(game.test_dir.clone(), results);
        }

        // Manually crank a frame
        let mut encoder = runtime.resource::<Gpu>().device().create_command_encoder(
            &wgpu::CommandEncoderDescriptor {
                label: Some("test-chunk-encoder"),
            },
        );
        chunk_man.close_open_chunks(&runtime.resource::<Gpu>(), &mut encoder);
        runtime
            .resource::<Gpu>()
            .device()
            .poll(wgpu::Maintain::Wait);
        chunk_man.cleanup_open_chunks_after_render(&mut runtime.resource_mut::<Gpu>());

        for (game, mut results) in result_maps.drain() {
            for (name, mut shape_ids) in results.drain() {
                for (sel, shape_id) in shape_ids.drain() {
                    let lifetime = chunk_man.part(shape_id).widgets();
                    let widgets = lifetime.read();
                    println!(
                        "{}:{}:{:?} => {} - {}",
                        game,
                        name,
                        sel,
                        widgets.num_xforms(),
                        widgets.name()
                    );
                }
            }
        }

        Ok(())
    }
}
