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
use crate::upload::Vertex;
use failure::Fallible;
use std::sync::Arc;
use vulkano::{
    buffer::{BufferUsage, CpuAccessibleBuffer, CpuBufferPool, DeviceLocalBuffer},
    command_buffer::{AutoCommandBufferBuilder, CommandBuffer, DynamicState},
    framebuffer::Subpass,
    pipeline::{
        depth_stencil::{Compare, DepthBounds, DepthStencil},
        GraphicsPipeline, GraphicsPipelineAbstract,
    },
    sync::GpuFuture,
};
use window::GraphicsWindow;

const CHUNK_MODEL_TARGET_COUNT: usize = 64;

const AVERAGE_VERTEX_BYTES: usize = 24_783;
const MAX_VERTEX_BYTES: usize = 157_968;
const VERTEX_CHUNK_SIZE: usize = AVERAGE_VERTEX_BYTES * CHUNK_MODEL_TARGET_COUNT; // ~1.5 MiB

const AVERAGE_INDEX_BYTES: usize = 2_065;
const MAX_INDEX_BYTES: usize = 13_164;
const INDEX_CHUNK_SIZE: usize = AVERAGE_INDEX_BYTES * CHUNK_MODEL_TARGET_COUNT; // ~129 KiB

#[derive(Clone, Copy)]
pub struct BufferCursor {
    vertex_offset: usize,
    index_offset: usize,
}

impl Default for BufferCursor {
    fn default() -> Self {
        Self {
            vertex_offset: 0,
            index_offset: 0,
        }
    }
}

pub struct BufferUploadTarget {
    vertex_upload_buffer: Arc<CpuAccessibleBuffer<[Vertex]>>,
    index_upload_buffer: Arc<CpuAccessibleBuffer<[u32]>>,
    cursor: BufferCursor,
}

impl BufferUploadTarget {
    pub fn new(window: &GraphicsWindow) -> Fallible<Self> {
        let vertex_upload_buffer: Arc<CpuAccessibleBuffer<[Vertex]>> = unsafe {
            CpuAccessibleBuffer::raw(
                window.device(),
                VERTEX_CHUNK_SIZE + MAX_VERTEX_BYTES,
                BufferUsage::all(),
                vec![window.queue().family()],
            )?
        };

        let index_upload_buffer: Arc<CpuAccessibleBuffer<[u32]>> = unsafe {
            CpuAccessibleBuffer::raw(
                window.device(),
                INDEX_CHUNK_SIZE + MAX_INDEX_BYTES,
                BufferUsage::all(),
                vec![window.queue().family()],
            )?
        };

        Ok(Self {
            vertex_upload_buffer,
            index_upload_buffer,
            cursor: BufferCursor {
                vertex_offset: 0,
                index_offset: 0,
            },
        })
    }

    pub fn push_with_index(&mut self, vertex: Vertex) -> Fallible<()> {
        let mut vertex_buffer = self.vertex_upload_buffer.write()?;
        let mut index_buffer = self.index_upload_buffer.write()?;
        vertex_buffer[self.cursor.vertex_offset] = vertex;
        index_buffer[self.cursor.index_offset] = self.cursor.vertex_offset as u32;
        self.cursor.vertex_offset += 1;
        self.cursor.index_offset += 1;
        Ok(())
    }

    pub fn finish(
        &mut self,
        window: &GraphicsWindow,
    ) -> Fallible<(
        Arc<DeviceLocalBuffer<[Vertex]>>,
        Arc<DeviceLocalBuffer<[u32]>>,
    )> {
        println!(
            "uploading vertex buffer with {} bytes",
            std::mem::size_of::<Vertex>() * self.cursor.vertex_offset
        );
        let vertex_buffer: Arc<DeviceLocalBuffer<[Vertex]>> = DeviceLocalBuffer::array(
            window.device(),
            self.cursor.vertex_offset,
            BufferUsage::vertex_buffer_transfer_destination(),
            window.device().active_queue_families(),
        )?;

        println!(
            "uploading index buffer with {} bytes",
            std::mem::size_of::<u32>() * self.cursor.index_offset
        );
        let index_buffer: Arc<DeviceLocalBuffer<[u32]>> = DeviceLocalBuffer::array(
            window.device(),
            self.cursor.index_offset,
            BufferUsage::index_buffer_transfer_destination(),
            window.device().active_queue_families(),
        )?;

        let cb = AutoCommandBufferBuilder::primary_one_time_submit(
            window.device(),
            window.queue().family(),
        )?
        .copy_buffer(self.vertex_upload_buffer.clone(), vertex_buffer.clone())?
        .copy_buffer(self.index_upload_buffer.clone(), index_buffer.clone())?
        .build()?;

        // Note that we currently have to wait for completion because we want
        // to re-use the CpuAccessibleBuffer immediately.
        let upload_future = cb.execute(window.queue())?;
        upload_future.then_signal_fence_and_flush()?.wait(None)?;
        self.cursor = Default::default();
        Ok((vertex_buffer, index_buffer))
    }
}

pub struct BufferUploadState<'a> {
    target: &'a mut BufferUploadTarget,
    start_of_object: BufferCursor,
    buffer_index: usize,
}

impl<'a> BufferUploadState<'a> {
    pub fn push_with_index(&mut self, vertex: Vertex) -> Fallible<()> {
        self.target.push_with_index(vertex)
    }
}

#[derive(Clone, Copy)]
pub struct BufferPointer {
    buffer_index: usize,
    start_of_object: BufferCursor,
    pub index_count: usize,
}

pub struct BufferManager {
    buffer_upload_target: BufferUploadTarget,
    chunks: Vec<(
        Arc<DeviceLocalBuffer<[Vertex]>>,
        Arc<DeviceLocalBuffer<[u32]>>,
    )>,
}

impl<'a> BufferManager {
    pub fn new(window: &GraphicsWindow) -> Fallible<Self> {
        Ok(Self {
            buffer_upload_target: BufferUploadTarget::new(window)?,
            chunks: vec![],
        })
    }

    pub fn buffers_at(
        &self,
        pointer: &BufferPointer,
    ) -> (
        Arc<DeviceLocalBuffer<[Vertex]>>,
        Arc<DeviceLocalBuffer<[u32]>>,
    ) {
        (
            self.chunks[pointer.buffer_index].0.clone(),
            self.chunks[pointer.buffer_index].1.clone(),
        )
    }

    pub fn prepare_upload(&'a mut self) -> Fallible<BufferUploadState<'a>> {
        Ok(BufferUploadState {
            start_of_object: self.buffer_upload_target.cursor,
            buffer_index: self.chunks.len(),
            target: &mut self.buffer_upload_target,
        })
    }

    pub fn finish_upload(bus: BufferUploadState<'a>) -> BufferPointer {
        // TODO: Check if overflowing and move to next chunk if so.
        BufferPointer {
            buffer_index: bus.buffer_index,
            start_of_object: bus.start_of_object,
            index_count: bus.target.cursor.index_offset - bus.start_of_object.index_offset,
        }
    }

    pub fn finish_loading_phase(&mut self, window: &GraphicsWindow) -> Fallible<()> {
        let (vertex_chunk, index_chunk) = self.buffer_upload_target.finish(window)?;
        self.chunks.push((vertex_chunk, index_chunk));
        Ok(())
    }
}
