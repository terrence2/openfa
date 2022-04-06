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
use crate::chunk::{
    chunk_manager::TextureAtlasProperties,
    upload::{AnalysisResults, DrawSelection, ShapeMetadata, ShapeUploader, Vertex},
};
use anyhow::Result;
use atlas::AtlasPacker;
use bevy_ecs::prelude::*;
use catalog::Catalog;
use gpu::Gpu;
use image::Rgba;
use lazy_static::lazy_static;
use log::info;
use pal::Palette;
use parking_lot::RwLock;
use pic_uploader::PicUploader;
use sh::RawShape;
use smallvec::SmallVec;
use std::fmt::Formatter;
use std::{
    collections::HashMap,
    fmt::Display,
    mem,
    path::Path,
    sync::{Arc, Mutex},
};
use zerocopy::{AsBytes, FromBytes};

const AVERAGE_VERTEX_BYTES: usize = 24_783;
const VERTEX_CHUNK_COUNT: usize = AVERAGE_VERTEX_BYTES / mem::size_of::<Vertex>();

#[repr(C)]
#[derive(AsBytes, FromBytes, Copy, Clone, Debug, Default)]
pub struct DrawIndirectCommand {
    pub vertex_count: u32,
    pub instance_count: u32,
    pub first_vertex: u32,
    pub first_instance: u32,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct ChunkId(u32);

impl Display for ChunkId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Component)]
pub struct ShapeId((ChunkId, u32));

#[derive(Component, Clone, Debug)]
pub struct ShapeIds {
    normal: ShapeId,
    damage: SmallVec<[ShapeId; 1]>,
}

impl ShapeIds {
    pub(crate) fn new(normal_shape_id: ShapeId, damage_shape_ids: SmallVec<[ShapeId; 1]>) -> Self {
        Self {
            normal: normal_shape_id,
            damage: damage_shape_ids,
        }
    }

    pub fn normal(&self) -> ShapeId {
        self.normal
    }

    pub fn damage(&self) -> &[ShapeId] {
        &self.damage
    }
}

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

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ChunkFlags {
    // Passed in by the caller.
    has_transform: bool,

    // Discovered in the model analysis phase
    has_flags: bool,
    has_anim: bool,
    has_xform: bool,
}

impl ChunkFlags {
    pub(crate) fn for_analysis(analysis: &AnalysisResults) -> Self {
        Self {
            has_transform: true,
            has_flags: analysis.has_flags(),
            has_anim: analysis.has_animation(),
            has_xform: analysis.has_xforms(),
        }
    }
}

// Where a shape lives in a chunk.
#[derive(Debug)]
pub struct ChunkPart {
    vertex_start: usize,
    vertex_count: usize,
    xform_count: usize,
    metadata: Arc<RwLock<ShapeMetadata>>,
}

impl ChunkPart {
    // TODO: make this an initializer and figure out max_transformer_values up front.
    pub fn new(
        vertex_start: usize,
        vertex_end: usize,
        metadata: Arc<RwLock<ShapeMetadata>>,
    ) -> Self {
        let xform_count = metadata.read().num_xforms();
        ChunkPart {
            vertex_start,
            vertex_count: vertex_end - vertex_start,
            xform_count,
            metadata,
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

    pub fn metadata(&self) -> Arc<RwLock<ShapeMetadata>> {
        self.metadata.clone()
    }

    pub fn xform_count(&self) -> usize {
        self.xform_count
    }
}

#[derive(Debug)]
pub struct OpenChunk {
    chunk_id: ChunkId,
    chunk_flags: ChunkFlags,

    vertex_upload_buffer: Vec<Vertex>,
    atlas_packer: AtlasPacker<Rgba<u8>>,

    // So we can give out unique ids to each shape in this chunk.
    last_shape_id: u32,

    chunk_parts: HashMap<ShapeId, ChunkPart>,
}

impl OpenChunk {
    pub(crate) fn new(chunk_flags: ChunkFlags, gpu: &Gpu) -> Self {
        let atlas_size0 = 1024 + 4;
        let atlas_stride = Gpu::stride_for_row_size(atlas_size0 * 4);
        let atlas_size = atlas_stride / 4;
        Self {
            chunk_id: allocate_chunk_id(),
            chunk_flags,
            atlas_packer: AtlasPacker::<Rgba<u8>>::new(
                "open-shape-chunk",
                gpu,
                atlas_size,
                atlas_size,
                wgpu::TextureFormat::Rgba8Unorm,
                wgpu::FilterMode::Nearest, // TODO: see if we can "improve" things with filtering?
            ),
            vertex_upload_buffer: Vec::with_capacity(VERTEX_CHUNK_COUNT),
            last_shape_id: 0,
            chunk_parts: HashMap::new(),
        }
    }

    pub(crate) fn upload_atlas(&mut self, gpu: &Gpu, encoder: &mut wgpu::CommandEncoder) {
        self.atlas_packer.encode_frame_uploads(gpu, encoder);
    }

    pub(crate) fn chunk_is_empty(&self) -> bool {
        self.vertex_upload_buffer.is_empty()
    }

    pub(crate) fn dump_atlas(&self, gpu: &mut Gpu, path: &Path) {
        self.atlas_packer.dump_texture(gpu, path).ok();
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn upload_shape(
        &mut self,
        name: &str,
        analysis: AnalysisResults,
        sh: &RawShape,
        selection: &DrawSelection,
        palette: &Palette,
        catalog: &Catalog,
        pic_uploader: &mut PicUploader,
        gpu: &Gpu,
    ) -> Result<ShapeId> {
        assert!(*selection != DrawSelection::DamageModel || analysis.has_damage_model());
        let start_vertex = self.vertex_upload_buffer.len();
        let (shape_widgets, mut verts) = ShapeUploader::new(name, palette, catalog).draw_model(
            sh,
            analysis,
            selection,
            pic_uploader,
            &mut self.atlas_packer,
            gpu,
        )?;
        self.vertex_upload_buffer.append(&mut verts);

        let part = ChunkPart::new(start_vertex, self.vertex_upload_buffer.len(), shape_widgets);
        let shape_id = self.allocate_shape_id();
        self.chunk_parts.insert(shape_id, part);
        Ok(shape_id)
    }

    fn allocate_shape_id(&mut self) -> ShapeId {
        let shape_index = self.last_shape_id + 1;
        self.last_shape_id = shape_index;
        ShapeId((self.chunk_id, shape_index))
    }

    pub fn chunk_id(&self) -> ChunkId {
        self.chunk_id
    }

    pub fn part(&self, shape_id: ShapeId) -> &ChunkPart {
        &self.chunk_parts[&shape_id]
    }
}

#[derive(Debug)]
pub struct ClosedChunk {
    vertex_buffer: wgpu::Buffer,
    atlas_bind_group: wgpu::BindGroup,
    chunk_parts: HashMap<ShapeId, ChunkPart>,
}

impl ClosedChunk {
    pub fn new(
        chunk: &mut OpenChunk,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        gpu: &gpu::Gpu,
    ) -> Self {
        let v_size = chunk.vertex_upload_buffer.len() * std::mem::size_of::<Vertex>();
        let a_size = chunk.atlas_packer.atlas_size();
        info!(
            "uploading vertex/atlas buffer {:?} size {} / {} ({} total) bytes",
            chunk.chunk_flags,
            v_size,
            a_size,
            v_size + a_size
        );

        let vertex_buffer = gpu.push_slice(
            "shape-chunk-vertices",
            &chunk.vertex_upload_buffer,
            wgpu::BufferUsages::VERTEX,
        );

        let atlas_properties =
            TextureAtlasProperties::new(chunk.atlas_packer.width(), chunk.atlas_packer.height());
        let atlas_properties = gpu.push_buffer(
            "chunk-atlas-properties",
            atlas_properties.as_bytes(),
            wgpu::BufferUsages::UNIFORM,
        );

        // TODO: use entries from the atlas directly
        let atlas_view = chunk.atlas_packer.texture_view();
        let atlas_bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shape-chunk-atlas-bind-group"),
            layout,
            entries: &[
                // atlas texture
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &atlas_properties,
                        offset: 0,
                        size: None,
                    }),
                },
            ],
        });

        // Note: steal chunk parts as cloning it would be expensive and we don't need it anymore.
        let mut chunk_parts = HashMap::new();
        std::mem::swap(&mut chunk_parts, &mut chunk.chunk_parts);

        ClosedChunk {
            vertex_buffer,
            atlas_bind_group,
            chunk_parts,
        }
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.atlas_bind_group
    }

    pub fn vertex_buffer(&self) -> wgpu::BufferSlice {
        self.vertex_buffer.slice(..)
    }

    pub fn part(&self, shape_id: ShapeId) -> &ChunkPart {
        &self.chunk_parts[&shape_id]
    }
}
