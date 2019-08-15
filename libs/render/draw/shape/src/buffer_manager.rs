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
use crate::buffer::Vertex;
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

const AVERAGE_VERTEX_BYTES: usize = 24_783;
const MAX_VERTEX_BYTES: usize = 157_968;
const VERTEX_CHUNK_SIZE: usize = AVERAGE_VERTEX_BYTES * 64; // ~1.5 MiB

const AVERAGE_INDEX_BYTES: usize = 2_065;
const MAX_INDEX_BYTES: usize = 13_164;
const INDEX_CHUNK_SIZE: usize = AVERAGE_INDEX_BYTES * 64; // ~129 KiB

pub struct BufferUploadState {
    vertex_upload_offset: usize,
    vertex_upload_buffer: Arc<CpuAccessibleBuffer<[Vertex]>>,
    index_upload_offset: usize,
    index_upload_buffer: Arc<CpuAccessibleBuffer<[u32]>>,
}

impl BufferUploadState {
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
            vertex_upload_offset: 0,
            vertex_upload_buffer,
            index_upload_offset: 0,
            index_upload_buffer,
        })
    }

    pub fn push_with_index(&mut self, vertex: Vertex) -> Fallible<()> {
        let mut vertex_buffer = self.vertex_upload_buffer.write()?;
        let mut index_buffer = self.index_upload_buffer.write()?;
        vertex_buffer[self.vertex_upload_offset] = vertex;
        index_buffer[self.index_upload_offset] = self.vertex_upload_offset as u32;
        self.vertex_upload_offset += 1;
        self.index_upload_offset += 1;
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
            std::mem::size_of::<Vertex>() * self.vertex_upload_offset
        );
        let vertex_buffer: Arc<DeviceLocalBuffer<[Vertex]>> = DeviceLocalBuffer::array(
            window.device(),
            self.vertex_upload_offset,
            BufferUsage::vertex_buffer_transfer_destination(),
            window.device().active_queue_families(),
        )?;

        println!(
            "uploading index buffer with {} bytes",
            std::mem::size_of::<u32>() * self.index_upload_offset
        );
        let index_buffer: Arc<DeviceLocalBuffer<[u32]>> = DeviceLocalBuffer::array(
            window.device(),
            self.index_upload_offset,
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
        self.vertex_upload_offset = 0;
        self.index_upload_offset = 0;

        Ok((vertex_buffer, index_buffer))
    }
}

pub struct BufferPointer {
    // Selects the vertex and index buffer.
    buffer_index: usize,
    // Indicates the offset inside the buffers to start at.
    vertex_buffer_offset: usize,
    index_buffer_offset: usize,
}

pub struct BufferManager {
    buffer_upload_state: BufferUploadState,
    chunks: Vec<(
        Arc<DeviceLocalBuffer<[Vertex]>>,
        Arc<DeviceLocalBuffer<[u32]>>,
    )>,
}

impl BufferManager {
    pub fn new(window: &GraphicsWindow) -> Fallible<Self> {
        Ok(Self {
            buffer_upload_state: BufferUploadState::new(window)?,
            chunks: vec![],
        })
    }

    pub fn buffer_pointer(&self) -> BufferPointer {
        BufferPointer {
            buffer_index: self.chunks.len(),
            vertex_buffer_offset: self.buffer_upload_state.vertex_upload_offset,
            index_buffer_offset: self.buffer_upload_state.index_upload_offset,
        }
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

    pub fn prepare_upload(&mut self) -> Fallible<(BufferPointer, &mut BufferUploadState)> {
        // TODO: Check if overflowing and move to next chunk if so.
        Ok((self.buffer_pointer(), &mut self.buffer_upload_state))
    }

    pub fn finish_loading_phase(&mut self, window: &GraphicsWindow) -> Fallible<()> {
        let (vertex_chunk, index_chunk) = self.buffer_upload_state.finish(window)?;
        self.chunks.push((vertex_chunk, index_chunk));
        Ok(())
    }
}
