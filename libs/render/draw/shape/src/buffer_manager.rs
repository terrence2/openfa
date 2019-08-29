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
/*
use crate::upload::{ShapeBuffer, Vertex};
use failure::Fallible;
use std::{mem, sync::Arc};
use vulkano::{
    buffer::{BufferUsage, CpuAccessibleBuffer, DeviceLocalBuffer},
    command_buffer::{AutoCommandBufferBuilder, CommandBuffer},
    sync::GpuFuture,
};
use window::GraphicsWindow;

const CHUNK_MODEL_TARGET_COUNT: usize = 64;

const AVERAGE_VERTEX_BYTES: usize = 24_783;
const MAX_VERTEX_BYTES: usize = 157_968;
const VERTEX_CHUNK_HIGH_WATER_BYTES: usize = AVERAGE_VERTEX_BYTES * CHUNK_MODEL_TARGET_COUNT; // ~1.5 MiB
const VERTEX_CHUNK_HIGH_WATER_COUNT: usize =
    VERTEX_CHUNK_HIGH_WATER_BYTES / mem::size_of::<Vertex>();
const VERTEX_CHUNK_BYTES: usize = VERTEX_CHUNK_HIGH_WATER_BYTES + MAX_VERTEX_BYTES;

const AVERAGE_INDEX_BYTES: usize = 2_065;
const MAX_INDEX_BYTES: usize = 13_164;
const INDEX_CHUNK_HIGH_WATER_BYTES: usize = AVERAGE_INDEX_BYTES * CHUNK_MODEL_TARGET_COUNT; // ~129 KiB
const INDEX_CHUNK_HIGH_WATER_COUNT: usize = INDEX_CHUNK_HIGH_WATER_BYTES / mem::size_of::<u32>();
const INDEX_CHUNK_BYTES: usize = INDEX_CHUNK_HIGH_WATER_BYTES + MAX_INDEX_BYTES;

//const INDIRECT_BYTES: usize = mem::size_of::<DrawIndexedIndirectCommand>();
//const INDIRECT_CHUNK_HIGH_WATER_COUNT: usize = 5120;
//const INDIRECT_CHUNK_HIGH_WATER_BYTES: usize = INDIRECT_BYTES * INDIRECT_CHUNK_HIGH_WATER_COUNT; // 100KiB
//const INDIRECT_CHUNK_BYTES: usize = INDIRECT_CHUNK_HIGH_WATER_BYTES + INDIRECT_BYTES;

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
    used_by_buffers: Vec<Arc<ShapeBuffer>>,
}

impl BufferUploadTarget {
    pub fn new(window: &GraphicsWindow) -> Fallible<Self> {
        let vertex_upload_buffer: Arc<CpuAccessibleBuffer<[Vertex]>> = unsafe {
            CpuAccessibleBuffer::raw(
                window.device(),
                VERTEX_CHUNK_BYTES,
                BufferUsage::all(),
                vec![window.queue().family()],
            )?
        };

        let index_upload_buffer: Arc<CpuAccessibleBuffer<[u32]>> = unsafe {
            CpuAccessibleBuffer::raw(
                window.device(),
                INDEX_CHUNK_BYTES,
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
            used_by_buffers: Vec::new(),
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

    pub fn mark_buffer(&mut self, buffer: Arc<ShapeBuffer>) {
        self.used_by_buffers.push(buffer);
    }

    pub fn finish_buffer_upload(&mut self, window: &GraphicsWindow) -> Fallible<BufferChunk> {
        let vertex_buffer: Arc<DeviceLocalBuffer<[Vertex]>> = DeviceLocalBuffer::array(
            window.device(),
            self.cursor.vertex_offset,
            BufferUsage::vertex_buffer_transfer_destination(),
            window.device().active_queue_families(),
        )?;

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
        println!(
            "uploading vertex/index buffer with {} / {} ({} total) bytes",
            self.cursor.vertex_offset * std::mem::size_of::<Vertex>(),
            self.cursor.index_offset * std::mem::size_of::<u32>(),
            self.cursor.vertex_offset * std::mem::size_of::<Vertex>()
                + self.cursor.index_offset * std::mem::size_of::<u32>(),
        );

        // Link to newly created permanent buffers from every buffer that is using them.
        for buffer in self.used_by_buffers.drain(..) {
            buffer.note_buffers(vertex_buffer.clone(), index_buffer.clone());
        }

        // Note that we currently have to wait for completion because we want
        // to re-use the CpuAccessibleBuffer immediately.
        let upload_future = cb.execute(window.queue())?;
        upload_future.then_signal_fence_and_flush()?.wait(None)?;
        self.cursor = Default::default();
        Ok(BufferChunk {
            vertex_buffer,
            index_buffer,
        })
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

    pub fn mark_end_of_object_upload(&mut self) -> BufferPointer {
        let index_count = self.target.cursor.index_offset - self.start_of_object.index_offset;
        assert!(index_count <= std::u32::MAX as usize);
        assert!(self.start_of_object.index_offset <= std::u32::MAX as usize);
        assert!(self.start_of_object.vertex_offset <= std::u32::MAX as usize);
        BufferPointer {
            buffer_index: self.buffer_index,
            first_index: self.start_of_object.index_offset as u32,
            vertex_offset: self.start_of_object.vertex_offset as u32,
            index_count: index_count as u32,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BufferPointer {
    buffer_index: usize,
    first_index: u32,
    vertex_offset: u32,
    index_count: u32,
}

impl BufferPointer {
    pub fn index_count(&self) -> u32 {
        self.index_count
    }

    pub fn first_index(&self) -> u32 {
        self.first_index
    }

    pub fn vertex_offset(&self) -> u32 {
        self.vertex_offset
    }
}

pub struct BufferChunk {
    vertex_buffer: Arc<DeviceLocalBuffer<[Vertex]>>,
    index_buffer: Arc<DeviceLocalBuffer<[u32]>>,
}

impl BufferChunk {
    pub fn vertex_buffer(&self) -> Arc<DeviceLocalBuffer<[Vertex]>> {
        self.vertex_buffer.clone()
    }

    pub fn index_buffer(&self) -> Arc<DeviceLocalBuffer<[u32]>> {
        self.index_buffer.clone()
    }
}

pub struct BufferManager {
    buffer_upload_target: BufferUploadTarget,
    chunks: Vec<BufferChunk>,
    has_been_finished: bool,
}

impl<'a> BufferManager {
    pub fn new(window: &GraphicsWindow) -> Fallible<Self> {
        Ok(Self {
            buffer_upload_target: BufferUploadTarget::new(window)?,
            chunks: vec![],
            has_been_finished: false,
        })
    }

    pub fn buffers_at(&self, pointer: &BufferPointer) -> &BufferChunk {
        assert!(self.has_been_finished);
        &self.chunks[pointer.buffer_index]
    }

    pub fn prepare_to_upload_new_shape(
        &'a mut self,
        window: &GraphicsWindow,
    ) -> Fallible<BufferUploadState<'a>> {
        // Check if overflowing and move to next chunk if so.
        if self.buffer_upload_target.cursor.index_offset >= INDEX_CHUNK_HIGH_WATER_COUNT
            || self.buffer_upload_target.cursor.vertex_offset >= VERTEX_CHUNK_HIGH_WATER_COUNT
        {
            self.chunks
                .push(self.buffer_upload_target.finish_buffer_upload(window)?);
        }

        Ok(BufferUploadState {
            start_of_object: self.buffer_upload_target.cursor,
            buffer_index: self.chunks.len(),
            target: &mut self.buffer_upload_target,
        })
    }

    pub fn mark_buffer(bus: BufferUploadState<'a>, buffer: Arc<ShapeBuffer>) {
        bus.target.mark_buffer(buffer);
    }

    pub fn finish_loading_phase(&mut self, window: &GraphicsWindow) -> Fallible<()> {
        self.has_been_finished = true;
        self.chunks
            .push(self.buffer_upload_target.finish_buffer_upload(window)?);
        Ok(())
    }
}
*/
