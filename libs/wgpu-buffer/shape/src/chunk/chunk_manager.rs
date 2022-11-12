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
    chunks::{ChunkFlags, ChunkId, ChunkPart, ClosedChunk, OpenChunk, ShapeId},
    upload::DrawSelection,
    upload::ShapeUploader,
};
use anyhow::{Context, Result};
use catalog::Catalog;
use gpu::Gpu;
use pal::Palette;
use pic_uploader::PicUploader;
use sh::RawShape;
use std::{collections::HashMap, env, mem, num::NonZeroU64, path::PathBuf};
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
pub(crate) struct ChunkManager {
    chunk_bind_group_layout: wgpu::BindGroupLayout,
    shared_sampler: wgpu::Sampler,

    name_to_shape_map: HashMap<String, ShapeId>,
    shape_to_chunk_map: HashMap<ShapeId, ChunkId>,

    pic_uploader: PicUploader,
    // TODO: does this need to be a vec of OpenChunk? Where do we actually stop having "enough" room.
    open_chunks: HashMap<ChunkFlags, OpenChunk>,
    closed_chunks: HashMap<ChunkId, ClosedChunk>,
}

impl ChunkManager {
    pub fn new(gpu: &Gpu) -> Result<Self> {
        let chunk_bind_group_layout =
            gpu.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("shape-chunk-bind-group-layout"),
                    entries: &[
                        // FIXME: use layout entries from the atlas?
                        // Texture Atlas
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT,
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
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                        // Texture Atlas Properties
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::VERTEX,
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
            pic_uploader: PicUploader::new(gpu)?,
            open_chunks: HashMap::new(),
            closed_chunks: HashMap::new(),
        })
    }

    pub(crate) fn close_open_chunks(&mut self, gpu: &Gpu, encoder: &mut wgpu::CommandEncoder) {
        //   ScriptRun phase causes various chunk.upload_shape calls
        //     pic_uploader.upload -> creates a buffer, but not filled yet
        //     atlas.upload_buffer(unfilled buffer) into the blit_list of the atlas
        //   Render Phase:
        //     PicUploader::expand_pics fills out the buffers we pushed into the blit list above
        //     AtlasPacker::encode_frame_uploads copies and/or blits those buffers into the atlas
        self.pic_uploader.expand_pics(encoder);
        for open_chunk in self.open_chunks.values_mut() {
            assert!(
                !open_chunk.chunk_is_empty(),
                "opened a chunk that didn't get written to"
            );
            open_chunk.upload_atlas(gpu, encoder);
            let closed_chunk = ClosedChunk::new(
                open_chunk,
                &self.chunk_bind_group_layout,
                &self.shared_sampler,
                gpu,
            );
            self.closed_chunks
                .insert(open_chunk.chunk_id(), closed_chunk);
        }
        self.pic_uploader.finish_expand_pass();
    }

    pub(crate) fn cleanup_open_chunks_after_render(&mut self, gpu: &mut Gpu) {
        for (_, open_chunk) in self.open_chunks.drain() {
            if let Some(path) = Self::dump_path(open_chunk.chunk_id()) {
                open_chunk.dump_atlas(gpu, &path);
            }
        }
    }

    fn dump_path(chunk_id: ChunkId) -> Option<PathBuf> {
        if env::var("DUMP") == Ok("1".to_owned()) {
            if let Ok(mut path) = env::current_dir() {
                path.push("__dump__");
                path.push("shape_chunk");
                path.push(&format!("chunk-{}.png", chunk_id));
                Some(path)
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn push_one_shape(
        &mut self,
        palette: &Palette,
        shape_file_name: &str,
        sh: &RawShape,
        selection: DrawSelection,
        catalog: &Catalog,
        gpu: &Gpu,
    ) -> Result<(ShapeId, bool)> {
        let cache_key = format!("{}:{}:{:?}", catalog.label(), shape_file_name, selection);
        if let Some(&shape_id) = self.name_to_shape_map.get(&cache_key) {
            // Note: if we are cached, we would already have loaded the damage model
            return Ok((shape_id, false));
        }

        let analysis = ShapeUploader::analyze_model(shape_file_name, sh, &selection)?;
        // NOTE: The analysis should always detect the damage model, even when visiting
        //       normal model instructions: we have to jump the other direction after all.
        //       This let's us save a costly re-analysis if there's no model there anyway.
        let has_damage_model = analysis.has_damage_model();
        let chunk_flags = ChunkFlags::for_analysis(&analysis);
        let chunk = self
            .open_chunks
            .entry(chunk_flags)
            .or_insert_with(|| OpenChunk::new(chunk_flags, gpu));
        let shape_id = chunk.upload_shape(
            shape_file_name,
            analysis,
            sh,
            &selection,
            palette,
            catalog,
            &mut self.pic_uploader,
            gpu,
        )?;

        self.name_to_shape_map.insert(cache_key, shape_id);
        self.shape_to_chunk_map.insert(shape_id, chunk.chunk_id());
        Ok((shape_id, has_damage_model))
    }

    pub fn upload_shapes(
        &mut self,
        palette: &Palette,
        shapes: &HashMap<String, RawShape>,
        catalog: &Catalog,
        gpu: &Gpu,
    ) -> Result<HashMap<String, HashMap<DrawSelection, ShapeId>>> {
        self.pic_uploader.set_shared_palette(palette, gpu);

        let mut out = HashMap::new();
        for (shape_file_name, sh) in shapes.iter() {
            out.insert(shape_file_name.to_owned(), HashMap::new());

            let (normal_shape_id, has_damage_model) = self
                .push_one_shape(
                    palette,
                    shape_file_name,
                    sh,
                    DrawSelection::NormalModel,
                    catalog,
                    gpu,
                )
                .with_context(|| format!("normal shape file {shape_file_name}"))?;
            out.get_mut(shape_file_name)
                .unwrap()
                .insert(DrawSelection::NormalModel, normal_shape_id);

            if has_damage_model {
                let (damage_shape_id, has_damage_model) = self
                    .push_one_shape(
                        palette,
                        shape_file_name,
                        sh,
                        DrawSelection::DamageModel,
                        catalog,
                        gpu,
                    )
                    .with_context(|| format!("damage shape file {shape_file_name}"))?;
                assert!(has_damage_model);
                out.get_mut(shape_file_name)
                    .unwrap()
                    .insert(DrawSelection::DamageModel, damage_shape_id);
            }
        }

        Ok(out)
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

    pub fn shape_chunk(&self, shape_id: ShapeId) -> ChunkId {
        self.shape_to_chunk_map[&shape_id]
    }

    // NOTE: The chunk must be closed.
    pub fn chunk(&self, chunk_id: ChunkId) -> &ClosedChunk {
        &self.closed_chunks[&chunk_id]
    }

    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.chunk_bind_group_layout
    }
}
