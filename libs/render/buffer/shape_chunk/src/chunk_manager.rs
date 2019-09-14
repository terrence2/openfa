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
    upload::DrawSelection,
};
use failure::{err_msg, Fallible};
use lib::Library;
use pal::Palette;
use std::{collections::HashMap, mem, sync::Arc};
use vulkano::{pipeline::GraphicsPipelineAbstract, sync::GpuFuture};
use window::GraphicsWindow;

pub struct ShapeChunkManager {
    pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,

    shape_to_chunk_map: HashMap<ShapeId, ChunkId>,

    open_chunk: OpenChunk,
    closed_chunks: HashMap<ChunkId, ClosedChunk>,
}

impl ShapeChunkManager {
    pub fn new(
        pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
        window: &GraphicsWindow,
    ) -> Fallible<Self> {
        Ok(Self {
            pipeline,
            shape_to_chunk_map: HashMap::new(),
            open_chunk: OpenChunk::new(window)?,
            closed_chunks: HashMap::new(),
        })
    }

    pub fn finish(&mut self, window: &GraphicsWindow) -> Fallible<Box<dyn GpuFuture>> {
        self.finish_open_chunk(window)
    }

    pub fn finish_open_chunk(&mut self, window: &GraphicsWindow) -> Fallible<Box<dyn GpuFuture>> {
        let mut open_chunk = OpenChunk::new(window)?;
        mem::swap(&mut open_chunk, &mut self.open_chunk);
        let (chunk, future) = ClosedChunk::new(open_chunk, self.pipeline.clone(), window)?;
        self.closed_chunks.insert(chunk.chunk_id(), chunk);
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
        self.shape_to_chunk_map
            .insert(shape_id, self.open_chunk.chunk_id());
        Ok((shape_id, future))
    }

    pub fn part(&self, shape_id: ShapeId) -> Fallible<&ChunkPart> {
        let id = self
            .shape_to_chunk_map
            .get(&shape_id)
            .ok_or_else(|| err_msg("no chunk for associated shape id"))?;
        self.closed_chunks[id].part(shape_id)
    }
}
