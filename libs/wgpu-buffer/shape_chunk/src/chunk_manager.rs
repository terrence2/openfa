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
use crate::upload::ShapeUploader;
use crate::{
    chunk::{ChunkFlags, ChunkId, ChunkPart, ClosedChunk, OpenChunk, ShapeId},
    upload::DrawSelection,
};
use anyhow::{anyhow, Result};
use catalog::Catalog;
use gpu::{Gpu, UploadTracker};
use pal::Palette;
use pic_uploader::PicUploader;
use sh::RawShape;
use std::{collections::HashMap, env, mem, num::NonZeroU64};
use tokio::runtime::Runtime;
use zerocopy::{AsBytes, FromBytes};

#[repr(C)]
#[derive(AsBytes, FromBytes, Copy, Clone, Debug)]
pub struct TextureAtlasProperties {
    width: u32,
    height: u32,
    pad: [u32; 2],
}

impl TextureAtlasProperties {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pad: [0; 2],
        }
    }
}

#[derive(Debug)]
pub struct ShapeChunkBuffer {
    chunk_bind_group_layout: wgpu::BindGroupLayout,
    shared_sampler: wgpu::Sampler,

    name_to_shape_map: HashMap<String, ShapeId>,
    shape_to_chunk_map: HashMap<ShapeId, ChunkId>,

    shared_palette: Palette,
    pic_uploader: PicUploader,
    open_chunks: HashMap<ChunkFlags, OpenChunk>,
    closed_chunks: HashMap<ChunkId, ClosedChunk>,
    dump_atlas_textures: bool,
}

impl ShapeChunkBuffer {
    pub fn new(gpu: &Gpu) -> Result<Self> {
        let dump_atlas_textures = env::var("DUMP") == Ok("1".to_owned());
        let chunk_bind_group_layout =
            gpu.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("shape-chunk-bind-group-layout"),
                    entries: &[
                        // Texture Atlas
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStage::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                multisampled: false,
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                            },
                            count: None,
                        },
                        // Texture Atlas Sampler
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStage::FRAGMENT,
                            ty: wgpu::BindingType::Sampler {
                                filtering: true,
                                comparison: false,
                            },
                            count: None,
                        },
                        // Texture Atlas Properties
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStage::VERTEX,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: NonZeroU64::new(mem::size_of::<
                                    TextureAtlasProperties,
                                >(
                                )
                                    as u64),
                            },
                            count: None,
                        },
                    ],
                });
        let shared_sampler = gpu.device().create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shape-chunk-atlas-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            lod_min_clamp: 0f32,
            lod_max_clamp: 9_999_999f32,
            anisotropy_clamp: None,
            compare: None,
            border_color: None,
        });
        Ok(Self {
            chunk_bind_group_layout,
            shared_sampler,
            name_to_shape_map: HashMap::new(),
            shape_to_chunk_map: HashMap::new(),
            shared_palette: Palette::empty(),
            pic_uploader: PicUploader::new(gpu)?,
            open_chunks: HashMap::new(),
            closed_chunks: HashMap::new(),
            dump_atlas_textures,
        })
    }

    pub fn finish_open_chunks(
        &mut self,
        gpu: &mut Gpu,
        async_rt: &Runtime,
        tracker: &mut UploadTracker,
    ) -> Result<()> {
        let keys = self.open_chunks.keys().cloned().collect::<Vec<_>>();
        for chunk_flags in &keys {
            self.finish_open_chunk(*chunk_flags, gpu, async_rt, tracker)?;
        }
        Ok(())
    }

    pub fn finish_open_chunk(
        &mut self,
        chunk_flags: ChunkFlags,
        gpu: &mut Gpu,
        async_rt: &Runtime,
        tracker: &mut UploadTracker,
    ) -> Result<()> {
        let open_chunk = self.open_chunks.remove(&chunk_flags).expect("a chunk");
        if open_chunk.chunk_is_empty() {
            return Ok(());
        }
        let dump_path = if self.dump_atlas_textures {
            let mut path = env::current_dir()?;
            path.push("__dump__");
            path.push("shape_chunk");
            path.push(&format!("chunk-{}.png", open_chunk.chunk_id()));
            Some(path)
        } else {
            None
        };
        let chunk = ClosedChunk::new(
            open_chunk,
            &self.chunk_bind_group_layout,
            &self.shared_sampler,
            dump_path,
            &mut self.pic_uploader,
            gpu,
            async_rt,
            tracker,
        )?;
        self.closed_chunks.insert(chunk.chunk_id(), chunk);
        Ok(())
    }

    /// Set the palette we will use to decode colors. Try to upload blocks of shapes with the same
    /// palette at once as it's expensive to switch.
    pub fn set_shared_palette(&mut self, palette: &Palette, gpu: &Gpu) {
        self.shared_palette = palette.to_owned();
        self.pic_uploader.set_shared_palette(palette, gpu);
    }

    pub fn upload_shape(
        &mut self,
        name: &str,
        selection: DrawSelection,
        catalog: &Catalog,
        gpu: &mut Gpu,
        async_rt: &Runtime,
        tracker: &mut UploadTracker,
    ) -> Result<(ChunkId, ShapeId)> {
        let cache_key = format!("{}:{}", catalog.label(), name);
        if let Some(&shape_id) = self.name_to_shape_map.get(&cache_key) {
            let chunk_id = self.shape_to_chunk_map[&shape_id];
            return Ok((chunk_id, shape_id));
        }

        let sh = RawShape::from_bytes(&catalog.read_name_sync(name)?)?;
        let analysis = ShapeUploader::analyze_model(name, &sh, &selection)?;
        let chunk_flags = ChunkFlags::for_analysis(&analysis);

        if let Some(chunk) = self.open_chunks.get(&chunk_flags) {
            if chunk.chunk_is_full() {
                self.finish_open_chunk(chunk_flags, gpu, async_rt, tracker)?;
                self.open_chunks
                    .insert(chunk_flags, OpenChunk::new(chunk_flags, gpu)?);
            }
        } else {
            self.open_chunks
                .insert(chunk_flags, OpenChunk::new(chunk_flags, gpu)?);
        }
        let chunk_id = self.open_chunks[&chunk_flags].chunk_id();

        let shape_id = self
            .open_chunks
            .get_mut(&chunk_flags)
            .expect("an open chunk")
            .upload_shape(
                name,
                analysis,
                &sh,
                &selection,
                &self.shared_palette,
                catalog,
                &mut self.pic_uploader,
                gpu,
            )?;

        self.name_to_shape_map.insert(cache_key, shape_id);
        self.shape_to_chunk_map.insert(shape_id, chunk_id);
        Ok((chunk_id, shape_id))
    }

    pub fn shape_for(&self, name: &str) -> Result<ShapeId> {
        Ok(*self
            .name_to_shape_map
            .get(name)
            .ok_or_else(|| anyhow!("no shape for the given name"))?)
    }

    pub fn part(&self, shape_id: ShapeId) -> &ChunkPart {
        let chunk_id = self.shape_to_chunk_map[&shape_id];
        if let Some(chunk) = self.closed_chunks.get(&chunk_id) {
            return chunk.part(shape_id);
        } else {
            for open_chunk in self.open_chunks.values() {
                if open_chunk.chunk_id() != chunk_id {
                    continue;
                }
                return open_chunk.part(shape_id);
            }
        }
        unreachable!()
    }

    pub fn part_for(&self, name: &str) -> Result<&ChunkPart> {
        Ok(self.part(self.shape_for(name)?))
    }

    // NOTE: The chunk must be closed.
    pub fn chunk(&self, chunk_id: ChunkId) -> &ClosedChunk {
        &self.closed_chunks[&chunk_id]
    }

    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.chunk_bind_group_layout
    }
}
