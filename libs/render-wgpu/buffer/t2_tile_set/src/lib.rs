// This file is part of Nitrogen.
//
// Nitrogen is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Nitrogen is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with Nitrogen.  If not, see <http://www.gnu.org/licenses/>.
use anyhow::Result;
use catalog::Catalog;
use gpu::{UploadTracker, GPU};
use std::sync::Arc;
use t2::Terrain as T2Terrain;
use terrain::{
    tile::{DataSetCoordinates, DataSetDataKind},
    TileSet, VisiblePatch,
};
use tokio::{runtime::Runtime, sync::RwLock};

#[derive(Debug)]
pub struct T2HeightTileSet {}

impl T2HeightTileSet {
    pub fn from_t2(t2: &T2Terrain) -> Self {
        for y in 0..t2.height() {
            for x in 0..t2.width() {
                let _s = t2.sample_at(x, y);
            }
        }
        Self {}
    }
}

impl TileSet for T2HeightTileSet {
    fn kind(&self) -> DataSetDataKind {
        DataSetDataKind::Height
    }

    fn coordinates(&self) -> DataSetCoordinates {
        DataSetCoordinates::Spherical
    }

    fn begin_update(&mut self) {}

    fn note_required(&mut self, _visible_patch: &VisiblePatch) {}

    fn finish_update(
        &mut self,
        _catalog: Arc<RwLock<Catalog>>,
        _async_rt: &Runtime,
        _gpu: &GPU,
        _tracker: &mut UploadTracker,
    ) {
    }

    fn snapshot_index(&mut self, _async_rt: &Runtime, _gpu: &mut GPU) {}

    fn paint_atlas_index(&self, _encoder: &mut wgpu::CommandEncoder) {}

    fn displace_height<'a>(
        &'a self,
        _vertex_count: u32,
        _mesh_bind_group: &'a wgpu::BindGroup,
        cpass: wgpu::ComputePass<'a>,
    ) -> Result<wgpu::ComputePass<'a>> {
        Ok(cpass)
    }

    fn bind_group(&self) -> &wgpu::BindGroup {
        unimplemented!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lib::CatalogBuilder;

    #[test]
    fn it_can_load_all_t2() -> Result<()> {
        let (catalog, inputs) = CatalogBuilder::build_and_select(&["*:*.MM".to_owned()])?;
        for &fid in &inputs {
            let label = catalog.file_label(fid)?;
            let game = label.split(':').last().unwrap();
            let meta = catalog.stat_sync(fid)?;

            println!(
                "At: {}:{:13} @ {}",
                game,
                meta.name(),
                meta.path()
                    .map(|v| v.to_string_lossy())
                    .unwrap_or_else(|| "<none>".into())
            );

            let content = catalog.read_sync(fid)?;
            let t2 = T2Terrain::from_bytes(&content)?;
            let _ts = T2HeightTileSet::from_t2(&t2);
        }
        Ok(())
    }
}
