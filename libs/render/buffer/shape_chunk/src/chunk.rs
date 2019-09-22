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
    texture_atlas::MegaAtlas,
    upload::{DrawSelection, ShapeUploader, ShapeWidgets, Vertex},
};
use failure::{ensure, err_msg, Fallible};
use global_layout::GlobalSets;
use lazy_static::lazy_static;
use lib::Library;
use pal::Palette;
use sh::RawShape;
use std::{
    collections::HashMap,
    mem,
    sync::{Arc, Mutex, RwLock},
};
use vulkano::{
    buffer::{BufferUsage, CpuAccessibleBuffer, DeviceLocalBuffer},
    command_buffer::{AutoCommandBufferBuilder, CommandBuffer, DrawIndirectCommand},
    descriptor::descriptor_set::{DescriptorSet, PersistentDescriptorSet},
    pipeline::GraphicsPipelineAbstract,
    sync::GpuFuture,
};
use window::GraphicsWindow;

const CHUNK_MODEL_TARGET_COUNT: usize = 512;

const AVERAGE_VERTEX_BYTES: usize = 24_783;
const MAX_VERTEX_BYTES: usize = 157_968;
const VERTEX_CHUNK_HIGH_WATER_BYTES: usize = AVERAGE_VERTEX_BYTES * CHUNK_MODEL_TARGET_COUNT;
const VERTEX_CHUNK_HIGH_WATER_COUNT: usize =
    VERTEX_CHUNK_HIGH_WATER_BYTES / mem::size_of::<Vertex>();
const VERTEX_CHUNK_BYTES: usize = VERTEX_CHUNK_HIGH_WATER_BYTES + MAX_VERTEX_BYTES;
const VERTEX_CHUNK_COUNT: usize = VERTEX_CHUNK_BYTES / mem::size_of::<Vertex>();

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct ChunkId(u32);

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct ShapeId((ChunkId, u32));

lazy_static! {
    static ref GLOBAL_CHUNK_ID: Mutex<u32> = Mutex::new(0);
}

fn allocate_chunk_id() -> ChunkId {
    let mut global = GLOBAL_CHUNK_ID.lock().unwrap();
    let next_id = *global;
    assert!(next_id < std::u32::MAX, "overflowed chunk id");
    *global += 1;
    ChunkId(next_id)
}

pub enum Chunk {
    Open(OpenChunk),
    Closed(ClosedChunk),
}

// Where a shape lives in a chunk.
pub struct ChunkPart {
    vertex_start: usize,
    vertex_count: usize,
    shape_widgets: Arc<RwLock<ShapeWidgets>>,
}

impl ChunkPart {
    // TODO: make this an initializer and figure out max_transformer_values up front.
    pub fn new(
        vertex_start: usize,
        vertex_end: usize,
        shape_widgets: Arc<RwLock<ShapeWidgets>>,
    ) -> Self {
        ChunkPart {
            vertex_start,
            vertex_count: vertex_end - vertex_start,
            shape_widgets,
        }
    }

    pub fn draw_command(&self, first_instance: u32, instance_count: u32) -> DrawIndirectCommand {
        DrawIndirectCommand {
            first_vertex: self.vertex_start as u32,
            vertex_count: self.vertex_count as u32,
            first_instance,
            instance_count,
        }
    }

    pub fn widgets(&self) -> Arc<RwLock<ShapeWidgets>> {
        self.shape_widgets.clone()
    }
}

pub struct OpenChunk {
    chunk_id: ChunkId,

    vertex_upload_buffer: Arc<CpuAccessibleBuffer<[Vertex]>>,
    atlas_builder: MegaAtlas,
    vertex_offset: usize,

    // So we can give out unique ids to each shape in this chunk.
    last_shape_id: u32,

    chunk_parts: HashMap<ShapeId, ChunkPart>,
}

impl OpenChunk {
    pub fn new(window: &GraphicsWindow) -> Fallible<Self> {
        let vertex_upload_buffer: Arc<CpuAccessibleBuffer<[Vertex]>> = unsafe {
            CpuAccessibleBuffer::raw(
                window.device(),
                VERTEX_CHUNK_BYTES,
                BufferUsage::all(),
                vec![window.queue().family()],
            )?
        };

        Ok(Self {
            chunk_id: allocate_chunk_id(),
            vertex_offset: 0,
            atlas_builder: MegaAtlas::new(window)?,
            vertex_upload_buffer,
            last_shape_id: 0,
            chunk_parts: HashMap::new(),
        })
    }

    pub fn chunk_is_full(&self) -> bool {
        // TODO: also check on atlas?
        self.vertex_offset >= VERTEX_CHUNK_HIGH_WATER_COUNT
    }

    pub fn upload_shape(
        &mut self,
        name: &str,
        selection: DrawSelection,
        palette: &Palette,
        lib: &Library,
        window: &GraphicsWindow,
    ) -> Fallible<ShapeId> {
        let sh = RawShape::from_bytes(&lib.load(&name)?)?;

        let start_vertex = self.vertex_offset;
        let shape_widgets = Arc::new(RwLock::new(ShapeUploader::draw_model(
            name, &sh, selection, palette, lib, window, self,
        )?));

        let part = ChunkPart::new(start_vertex, self.vertex_offset, shape_widgets);
        let shape_id = self.allocate_shape_id();
        self.chunk_parts.insert(shape_id, part);
        Ok(shape_id)
    }

    fn allocate_shape_id(&mut self) -> ShapeId {
        let shape_index = self.last_shape_id + 1;
        self.last_shape_id = shape_index;
        ShapeId((self.chunk_id, shape_index))
    }

    pub fn push_vertex(&mut self, vertex: Vertex) -> Fallible<()> {
        ensure!(
            self.vertex_offset < VERTEX_CHUNK_COUNT,
            "overflowed vertex buffer"
        );
        let mut vertex_buffer = self.vertex_upload_buffer.write()?;
        vertex_buffer[self.vertex_offset] = vertex;
        self.vertex_offset += 1;
        Ok(())
    }

    pub fn atlas_mut(&mut self) -> &mut MegaAtlas {
        &mut self.atlas_builder
    }

    pub fn chunk_id(&self) -> ChunkId {
        self.chunk_id
    }

    pub fn part(&self, shape_id: ShapeId) -> &ChunkPart {
        &self.chunk_parts[&shape_id]
    }
}

pub struct ClosedChunk {
    vertex_buffer: Arc<DeviceLocalBuffer<[Vertex]>>,
    atlas_descriptor_set: Arc<dyn DescriptorSet + Send + Sync>,

    chunk_id: ChunkId,
    chunk_parts: HashMap<ShapeId, ChunkPart>,
}

impl ClosedChunk {
    pub fn new(
        chunk: OpenChunk,
        pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
        window: &GraphicsWindow,
    ) -> Fallible<(Self, Box<dyn GpuFuture>)> {
        let v_size = chunk.vertex_offset * std::mem::size_of::<Vertex>();
        let a_size = chunk.atlas_builder.atlas_size();
        println!(
            "uploading vertex/atlas buffer with {} / {} ({} total) bytes",
            v_size,
            a_size,
            v_size + a_size
        );

        let vertex_buffer: Arc<DeviceLocalBuffer<[Vertex]>> = DeviceLocalBuffer::array(
            window.device(),
            chunk.vertex_offset,
            BufferUsage::vertex_buffer_transfer_destination(),
            window.device().active_queue_families(),
        )?;

        let cbb = AutoCommandBufferBuilder::primary_one_time_submit(
            window.device(),
            window.queue().family(),
        )?
        .copy_buffer(chunk.vertex_upload_buffer.clone(), vertex_buffer.clone())?;
        let (cbb, atlas_texture) = chunk.atlas_builder.finish(cbb, window)?;
        let cb = cbb.build()?;
        let upload_future = Box::new(cb.execute(window.queue())?) as Box<dyn GpuFuture>;

        let sampler = MegaAtlas::make_sampler(window.device())?;
        let atlas_descriptor_set = Arc::new(
            PersistentDescriptorSet::start(pipeline, GlobalSets::ShapeTextures.into())
                .add_sampled_image(atlas_texture, sampler)?
                .build()?,
        );

        Ok((
            ClosedChunk {
                vertex_buffer,
                chunk_id: chunk.chunk_id,
                chunk_parts: chunk.chunk_parts,
                atlas_descriptor_set,
            },
            upload_future,
        ))
    }

    pub fn atlas_descriptor_set_ref(&self) -> Arc<dyn DescriptorSet + Send + Sync> {
        self.atlas_descriptor_set.clone()
    }

    pub fn vertex_buffer(&self) -> Arc<DeviceLocalBuffer<[Vertex]>> {
        self.vertex_buffer.clone()
    }

    pub fn chunk_id(&self) -> ChunkId {
        self.chunk_id
    }

    pub fn part(&self, shape_id: ShapeId) -> &ChunkPart {
        &self.chunk_parts[&shape_id]
    }
}
