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
    upload::{DrawSelection, ShapeErrata, ShapeUploader, Transformer, Vertex},
};
use failure::{ensure, err_msg, Fallible};
use global_layout::GlobalSets;
use lib::Library;
use pal::Palette;
use sh::RawShape;
use std::{collections::HashMap, mem, sync::Arc};
use vulkano::{
    buffer::{BufferUsage, CpuAccessibleBuffer, DeviceLocalBuffer},
    command_buffer::{AutoCommandBufferBuilder, CommandBuffer, DrawIndirectCommand},
    descriptor::descriptor_set::{
        DescriptorSet, PersistentDescriptorSet, PersistentDescriptorSetBuilderArray,
    },
    device::Device,
    format::Format,
    image::ImmutableImage,
    pipeline::GraphicsPipelineAbstract,
    sync::{now, GpuFuture},
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

pub enum Chunk {
    Open(OpenChunk),
    Closed(ClosedChunk),
}

// Where a shape lives in a chunk.
pub struct ChunkPart {
    vertex_start: usize,
    vertex_count: usize,
    transformers: Vec<Transformer>,
    errata: ShapeErrata,
}

impl ChunkPart {
    // TODO: make this an initializer and figure out max_transformer_values up front.

    pub fn command(&self, first_instance: u32, instance_count: u32) -> DrawIndirectCommand {
        DrawIndirectCommand {
            first_vertex: self.vertex_start as u32,
            vertex_count: self.vertex_count as u32,
            first_instance,
            instance_count,
        }
    }

    pub fn errata(&self) -> &ShapeErrata {
        &self.errata
    }

    pub fn transformers(&self) -> &[Transformer] {
        &self.transformers
    }

    // Number of floats required for all transforms in this part.
    pub fn num_transformer_values(&self) -> usize {
        (self.max_transformer_offset() + 1) * 6
    }

    fn max_transformer_offset(&self) -> usize {
        let mut max = 0;
        for transformer in &self.transformers {
            if transformer.offset() > max {
                max = transformer.offset();
            }
        }
        max
    }
}

pub struct OpenChunk {
    vertex_upload_buffer: Arc<CpuAccessibleBuffer<[Vertex]>>,
    atlas_builder: MegaAtlas,
    vertex_offset: usize,
    chunk_parts: HashMap<String, ChunkPart>,
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
            vertex_offset: 0,
            atlas_builder: MegaAtlas::new(window)?,
            vertex_upload_buffer,
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
    ) -> Fallible<()> {
        let sh = RawShape::from_bytes(&lib.load(&name)?)?;

        let start_vertex = self.vertex_offset;
        let (transformers, errata) =
            ShapeUploader::draw_model(name, &sh, selection, palette, lib, window, self)?;

        let part = ChunkPart {
            vertex_start: start_vertex,
            vertex_count: self.vertex_offset - start_vertex,
            transformers,
            errata,
        };
        self.chunk_parts.insert(name.to_owned(), part);
        Ok(())
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
}

pub struct ClosedChunk {
    vertex_buffer: Arc<DeviceLocalBuffer<[Vertex]>>,
    chunk_parts: HashMap<String, ChunkPart>,
    atlas_descriptor_set: Arc<dyn DescriptorSet + Send + Sync>,
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

        let mut cbb = AutoCommandBufferBuilder::primary_one_time_submit(
            window.device(),
            window.queue().family(),
        )?
        .copy_buffer(chunk.vertex_upload_buffer.clone(), vertex_buffer.clone())?;
        let (mut cbb, atlas_texture) = chunk.atlas_builder.finish(cbb, window)?;
        let cb = cbb.build()?;
        let upload_future = Box::new(cb.execute(window.queue())?) as Box<dyn GpuFuture>;

        let sampler = MegaAtlas::make_sampler(window.device())?;
        let mut atlas_descriptor_set = Arc::new(
            PersistentDescriptorSet::start(pipeline, GlobalSets::ShapeTextures.into())
                .add_sampled_image(atlas_texture, sampler)?
                .build()?,
        );

        Ok((
            ClosedChunk {
                vertex_buffer,
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

    pub fn part_for(&self, name: &str) -> Fallible<&ChunkPart> {
        self.chunk_parts
            .get(name)
            .ok_or_else(|| err_msg("shape not found"))
    }
}
