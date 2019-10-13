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
use crate::{
    chunk::{ChunkId, ChunkPart, ClosedChunk, OpenChunk, ShapeId},
    texture_atlas::MegaAtlas,
    upload::DrawSelection,
};
use failure::{err_msg, Fallible};
use lib::Library;
use pal::Palette;
use std::{collections::HashMap, mem};

pub struct ShapeChunkManager {
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,

    name_to_shape_map: HashMap<String, ShapeId>,
    shape_to_chunk_map: HashMap<ShapeId, ChunkId>,

    open_chunk: OpenChunk,
    closed_chunks: HashMap<ChunkId, ClosedChunk>,
}

impl ShapeChunkManager {
    pub fn new(device: &wgpu::Device) -> Fallible<Self> {
        Ok(Self {
            //pipeline,
            layout: MegaAtlas::make_bind_group_layout(device),
            sampler: MegaAtlas::make_sampler(device),
            name_to_shape_map: HashMap::new(),
            shape_to_chunk_map: HashMap::new(),
            open_chunk: OpenChunk::new()?,
            closed_chunks: HashMap::new(),
        })
    }

    pub fn finish(&mut self, gpu: &mut gpu::GPU) -> Fallible<()> {
        self.finish_open_chunk(gpu)
    }

    pub fn finish_open_chunk(&mut self, gpu: &mut gpu::GPU) -> Fallible<()> {
        if self.open_chunk.chunk_is_empty() {
            return Ok(());
        }
        let mut open_chunk = OpenChunk::new()?;
        mem::swap(&mut open_chunk, &mut self.open_chunk);
        let chunk = ClosedChunk::new(open_chunk, &self.layout, &self.sampler, gpu)?;
        self.closed_chunks.insert(chunk.chunk_id(), chunk);
        Ok(())
    }

    pub fn upload_shape(
        &mut self,
        name: &str,
        selection: DrawSelection,
        palette: &Palette,
        lib: &Library,
        gpu: &mut gpu::GPU,
    ) -> Fallible<(ChunkId, ShapeId)> {
        if let Some(&shape_id) = self.name_to_shape_map.get(name) {
            let chunk_id = self.shape_to_chunk_map[&shape_id];
            return Ok((chunk_id, shape_id));
        }
        if self.open_chunk.chunk_is_full() {
            self.finish_open_chunk(gpu)?
        }
        let shape_id = self
            .open_chunk
            .upload_shape(name, selection, palette, lib)?;
        self.name_to_shape_map.insert(name.to_owned(), shape_id);
        self.shape_to_chunk_map
            .insert(shape_id, self.open_chunk.chunk_id());
        Ok((self.open_chunk.chunk_id(), shape_id))
    }

    pub fn shape_for(&self, name: &str) -> Fallible<ShapeId> {
        Ok(*self
            .name_to_shape_map
            .get(name)
            .ok_or_else(|| err_msg("no shape for the given name"))?)
    }

    pub fn part(&self, shape_id: ShapeId) -> &ChunkPart {
        let chunk_id = self.shape_to_chunk_map[&shape_id];
        if let Some(chunk) = self.closed_chunks.get(&chunk_id) {
            chunk.part(shape_id)
        } else {
            self.open_chunk.part(shape_id)
        }
    }

    pub fn part_for(&self, name: &str) -> Fallible<&ChunkPart> {
        Ok(self.part(self.shape_for(name)?))
    }

    // NOTE: The chunk must be closed.
    pub fn chunk(&self, chunk_id: ChunkId) -> &ClosedChunk {
        &self.closed_chunks[&chunk_id]
    }

    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.layout
    }
}
