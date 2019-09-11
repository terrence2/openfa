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
    chunk::{ChunkPart, ClosedChunk, OpenChunk, ShapeId},
    upload::DrawSelection,
};
use failure::Fallible;
use lib::Library;
use pal::Palette;
use std::{collections::HashMap, mem, sync::Arc};
use vulkano::{pipeline::GraphicsPipelineAbstract, sync::GpuFuture};
use window::GraphicsWindow;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct ChunkIndex(pub usize);

pub struct ShapeChunkManager {
    pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,

    open_chunk: OpenChunk,
    next_chunk_index: usize,
    closed_chunks: HashMap<ChunkIndex, ClosedChunk>,
    shape_map: HashMap<ShapeId, ChunkIndex>,
}

impl ShapeChunkManager {
    pub fn new(
        pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
        window: &GraphicsWindow,
    ) -> Fallible<Self> {
        Ok(Self {
            pipeline,
            open_chunk: OpenChunk::new(window)?,
            next_chunk_index: 0,
            closed_chunks: HashMap::new(),
            shape_map: HashMap::new(),
        })
    }

    pub fn finish(&mut self, window: &GraphicsWindow) -> Fallible<Box<dyn GpuFuture>> {
        self.finish_open_chunk(window)
    }

    pub fn finish_open_chunk(&mut self, window: &GraphicsWindow) -> Fallible<Box<dyn GpuFuture>> {
        let mut open_chunk = OpenChunk::new(window)?;
        mem::swap(&mut open_chunk, &mut self.open_chunk);

        let chunk_index = ChunkIndex(self.next_chunk_index);
        self.next_chunk_index += 1;
        let (chunk, future) =
            ClosedChunk::new(open_chunk, chunk_index, self.pipeline.clone(), window)?;
        for shape_id in chunk.all_shapes() {
            self.shape_map.insert(shape_id, chunk_index);
        }
        self.closed_chunks.insert(chunk_index, chunk);

        Ok(future)
    }

    pub fn upload_shape(
        &mut self,
        name: &str,
        selection: DrawSelection,
        palette: &Palette,
        lib: &Library,
        window: &GraphicsWindow,
    ) -> Fallible<(ShapeId, Option<Box<dyn GpuFuture>>)> {
        let future = if self.open_chunk.chunk_is_full() {
            Some(self.finish_open_chunk(window)?)
        } else {
            None
        };
        let shape_id = self
            .open_chunk
            .upload_shape(name, selection, palette, lib, window)?;
        Ok((shape_id, future))
    }

    pub fn part(&self, shape_id: ShapeId) -> &ChunkPart {
        self.get_chunk_for_shape(shape_id).part(shape_id)
    }

    pub fn get_chunk_for_shape(&self, shape_id: ShapeId) -> &ClosedChunk {
        let chunk_id = self.shape_map[&shape_id];
        &self.closed_chunks[&chunk_id]
    }

    pub fn get_chunk(&self, chunk_index: ChunkIndex) -> &ClosedChunk {
        &self.closed_chunks[&chunk_index]
    }
}
