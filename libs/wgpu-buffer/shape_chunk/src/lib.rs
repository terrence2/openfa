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
mod upload;

pub use chunk::{ChunkId, ChunkPart, ClosedChunk, DrawIndirectCommand, OpenChunk, ShapeId};
pub use chunk_manager::ShapeChunkBuffer;
pub use draw_state::DrawState;
pub use upload::{DrawSelection, ShapeErrata, ShapeWidgets, Vertex};

#[cfg(test)]
mod test {
    use super::*;
    use anyhow::Result;
    use gpu::{Gpu, UploadTracker};
    use lib::CatalogManager;
    use log::trace;
    use nitrous::Interpreter;
    use pal::Palette;
    use tokio::runtime::Runtime;
    use winit::{event_loop::EventLoop, window::Window};

    #[cfg(unix)]
    #[test]
    fn test_load_all() -> Result<()> {
        use winit::platform::unix::EventLoopExtUnix;
        let event_loop = EventLoop::<()>::new_any_thread();
        let window = Window::new(&event_loop)?;
        let interpreter = Interpreter::new();
        let gpu = Gpu::new(window, Default::default(), &mut interpreter.write())?;
        let async_rt = Runtime::new()?;

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

        let catalogs = CatalogManager::for_testing()?;

        let mut chunk_man = ShapeChunkBuffer::new(&gpu.read())?;
        let mut tracker = UploadTracker::default();
        let mut all_shapes = Vec::new();
        for (game, catalog) in catalogs.selected() {
            let palette = Palette::from_bytes(&catalog.read_name_sync("PALETTE.PAL")?)?;
            chunk_man.set_shared_palette(&palette, &gpu.read());
            for fid in catalog.find_with_extension("SH")? {
                let meta = catalog.stat_sync(fid)?;
                println!(
                    "At: {}:{:13} @ {}",
                    game.test_dir,
                    meta.name(),
                    meta.path()
                        .map(|v| v.to_string_lossy())
                        .unwrap_or_else(|| "<none>".into())
                );
                if skipped.contains(&meta.name()) {
                    continue;
                }
                let (_chunk_id, shape_id) = chunk_man.upload_shape(
                    meta.name(),
                    DrawSelection::NormalModel,
                    catalog,
                    &mut gpu.write(),
                    &async_rt,
                    &mut tracker,
                )?;
                all_shapes.push(shape_id);
            }
        }
        chunk_man.finish_open_chunks(&mut gpu.write(), &async_rt, &mut tracker)?;
        gpu.read().device().poll(wgpu::Maintain::Wait);

        for shape_id in &all_shapes {
            let lifetime = chunk_man.part(*shape_id).widgets();
            let widgets = lifetime.read();
            trace!("{} - {}", widgets.num_xforms(), widgets.name());
        }

        Ok(())
    }
}
