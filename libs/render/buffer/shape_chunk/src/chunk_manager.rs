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
    chunk::{ClosedChunk, OpenChunk, ShapeId},
    upload::DrawSelection,
};
use failure::{bail, Fallible};
use lib::Library;
use pal::Palette;
use std::{mem, sync::Arc};
use vulkano::{pipeline::GraphicsPipelineAbstract, sync::GpuFuture};
use window::GraphicsWindow;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct ChunkIndex(usize);

pub struct ShapeChunkManager {
    pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,

    open_chunk: OpenChunk,
    closed_chunks: Vec<ClosedChunk>,
}

impl ShapeChunkManager {
    pub fn new(
        pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
        window: &GraphicsWindow,
    ) -> Fallible<Self> {
        Ok(Self {
            pipeline,
            open_chunk: OpenChunk::new(window)?,
            closed_chunks: Vec::new(),
        })
    }

    // pub fn create_building

    // pub fn create_airplane -- need to hook into shape state?

    pub fn finish(&mut self, window: &GraphicsWindow) -> Fallible<Box<dyn GpuFuture>> {
        self.finish_open_chunk(window)
    }

    pub fn finish_open_chunk(&mut self, window: &GraphicsWindow) -> Fallible<Box<dyn GpuFuture>> {
        let mut open_chunk = OpenChunk::new(window)?;
        mem::swap(&mut open_chunk, &mut self.open_chunk);
        let (chunk, future) = ClosedChunk::new(open_chunk, self.pipeline.clone(), window)?;
        self.closed_chunks.push(chunk);
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

    // TODO: we should maybe speed this up with a hash from shape_id to chunk_index
    pub fn find_chunk_for_shape(&self, shape_id: ShapeId) -> Fallible<ChunkIndex> {
        for (chunk_offset, chunk) in self.closed_chunks.iter().enumerate() {
            if chunk.part(shape_id).is_some() {
                return Ok(ChunkIndex(chunk_offset));
            }
        }
        bail!("shape_id {:?} has not been uploaded", shape_id)
    }

    pub fn get_chunk(&self, chunk_index: ChunkIndex) -> &ClosedChunk {
        &self.closed_chunks[chunk_index.0]
    }

    pub fn at(&self, chunk_index: ChunkIndex) -> &ClosedChunk {
        &self.closed_chunks[chunk_index.0]
    }
}