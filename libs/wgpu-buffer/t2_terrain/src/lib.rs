// This file is part of Nitrogen.
//
// Nitrogen is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Nitrogen is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with Nitrogen.  If not, see <http://www.gnu.org/licenses/>.
mod t2_info;

use crate::t2_info::T2Info;
use absolute_unit::{degrees, meters, radians, Angle, Degrees, Feet, Length, Meters};
use anyhow::{ensure, Result};
use atlas::AtlasPacker;
use bevy_ecs::prelude::*;
use catalog::Catalog;
use geodesy::{GeoSurface, Graticule};
use global_data::{GlobalParametersBuffer, GlobalsRenderStep};
use gpu::wgpu::CommandEncoder;
use gpu::{texture_format_size, Gpu};
use image::Rgba;
use lay::Layer;
use mmm::{MissionMap, TLoc};
use nalgebra::Point3;
use nitrous::{
    inject_nitrous_component, inject_nitrous_resource, NitrousComponent, NitrousResource,
};
use pal::Palette;
use pic_uploader::PicUploader;
use runtime::{Extension, FrameStage, Runtime};
use shader_shared::Group;
use std::{
    collections::{HashMap, HashSet},
    mem,
    num::{NonZeroU32, NonZeroU64},
};
use t2::Terrain as T2;
use terrain::{TerrainBuffer, TerrainRenderStep};
use world::WorldRenderStep;
use zerocopy::AsBytes;

#[derive(Debug)]
pub struct T2Adjustment {
    pub base_offset: [Angle<Degrees>; 2],
    pub span_offset: [Length<Meters>; 2],
    pub blend_factor: f32,
    pub dirty: bool,
}

impl Default for T2Adjustment {
    fn default() -> Self {
        Self {
            base_offset: [degrees!(0); 2],
            span_offset: [meters!(0); 2],
            blend_factor: 1.0,
            dirty: false,
        }
    }
}

/// Converts between FightersAnthology cartesian offsets within a tile
/// to Geodesic coordinates for use with the nitrogen engine.
#[derive(Clone, Debug)]
pub struct T2Mapper {
    base: [Angle<Degrees>; 2],
    extent: [Angle<Degrees>; 2],
    extent_ft: [f32; 2],
}

impl T2Mapper {
    pub fn new(t2: &T2, adjust: &T2Adjustment) -> Self {
        // Treating lat/lon span as if we're at the equator seems to work, even for tiles that
        // are far off the equator.
        let base_deg = t2.base_graticule_degrees();
        let base = [
            degrees!(base_deg[0]) + adjust.base_offset[0],
            degrees!(base_deg[1]) + adjust.base_offset[1],
        ];
        let lat_span_adj = adjust.span_offset[0].f32();
        let lon_span_adj = adjust.span_offset[1].f32();
        let extent_ft = [t2.extent_north_south_in_ft(), t2.extent_east_west_in_ft()];
        // Need to figure out what's missing here.
        // factor for ft to nautical ft: / (5280. / 6080.)
        // factor for offset from equator: * radians!(base[0]).cos() as f32
        let extent = [
            degrees!(extent_ft[0] / (364_000. + lat_span_adj)),
            degrees!(extent_ft[1] / (364_000. + lon_span_adj)),
        ];
        Self {
            base,
            extent,
            extent_ft,
        }
    }

    pub fn base_rad_f32(&self) -> [f32; 2] {
        [radians!(self.base[0]).f32(), radians!(self.base[1]).f32()]
    }

    pub fn extent_rad_f32(&self) -> [f32; 2] {
        [
            radians!(self.extent[0]).f32(),
            radians!(self.extent[1]).f32(),
        ]
    }

    pub fn fa2grat(
        &self,
        pos: &Point3<i32>,
        offset_from_ground: Length<Feet>,
    ) -> Graticule<GeoSurface> {
        // vec2 tile_uv = vec2(
        //     ((grat.y - t2_base.y) / t2_span.y) * cos(grat.x),
        //     1. - (t2_base.x - grat.x) / t2_span.x
        // );
        // lon_f = ((pos.lon() - base.lon) / span.lon) * cos(pos.lat)
        // lon_f / cos(pos.lat) * span.lon = (pos.lon - base.lon)
        // pos.lon = (lon_f / cos(pos.lat) * span.lon) + base.lon
        let lat_f = pos[2] as f32 / self.extent_ft[0];
        let lon_f = pos[0] as f32 / self.extent_ft[1];
        let lat = self.base[0] + (self.extent[0] * lat_f) - self.extent[0];
        let lon = -((self.extent[1] / lat.cos() as f32 * lon_f) + self.base[1]);
        Graticule::new(lat, lon, meters!(offset_from_ground))
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, SystemLabel)]
pub enum T2TerrainRenderStep {
    FinishUploads,
    EncodeUploads,
    PaintAtlasIndices,
    Tesselate,
    RenderDeferredTexture,
    AccumulateNormalsAndColor,
}

#[derive(Debug)]
pub struct TextureUploadOp {
    source: wgpu::Buffer,
    source_layout: wgpu::ImageDataLayout,
    target: wgpu::Texture,
    view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    copy_size: wgpu::Extent3d,
}

#[derive(Debug)]
pub struct TextureUploads {
    height: TextureUploadOp,
    base_color: TextureUploadOp,
    index: TextureUploadOp,
    index_size: [f32; 2],
}

#[derive(Debug)]
pub struct T2TileSetUpload {
    texture_uploads: TextureUploads,
    pic_uploader: PicUploader,
    atlas_packer: AtlasPacker<Rgba<u8>>,
}

#[derive(Debug, NitrousResource)]
pub struct T2TerrainBuffer {
    // Shared shader routine.
    displace_height_pipeline: wgpu::ComputePipeline,
    accumulate_color_pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    uploads: Vec<T2TileSetUpload>,
}

impl Extension for T2TerrainBuffer {
    fn init(runtime: &mut Runtime) -> Result<()> {
        let terrain2 = T2TerrainBuffer::new(
            runtime.resource::<TerrainBuffer>(),
            runtime.resource::<GlobalParametersBuffer>(),
            runtime.resource::<Gpu>(),
        )?;
        runtime.insert_named_resource("terrain2", terrain2);

        runtime
            .frame_stage_mut(FrameStage::Render)
            .add_system(Self::sys_finish_uploads.label(T2TerrainRenderStep::FinishUploads));

        // Ensure both relative order and ensure each step follows the equivalent base Terrain step
        runtime.frame_stage_mut(FrameStage::Render).add_system(
            Self::sys_encode_uploads
                .label(T2TerrainRenderStep::EncodeUploads)
                .after(T2TerrainRenderStep::FinishUploads)
                .after(TerrainRenderStep::EncodeUploads)
                .after(GlobalsRenderStep::EnsureUpdated),
        );
        runtime.frame_stage_mut(FrameStage::Render).add_system(
            Self::sys_paint_atlas_indices
                .label(T2TerrainRenderStep::PaintAtlasIndices)
                .after(T2TerrainRenderStep::EncodeUploads)
                .after(TerrainRenderStep::PaintAtlasIndices),
        );
        runtime.frame_stage_mut(FrameStage::Render).add_system(
            Self::sys_terrain_tesselate
                .label(T2TerrainRenderStep::Tesselate)
                .after(T2TerrainRenderStep::PaintAtlasIndices)
                .after(TerrainRenderStep::Tesselate)
                .before(TerrainRenderStep::RenderDeferredTexture),
        );
        runtime.frame_stage_mut(FrameStage::Render).add_system(
            Self::sys_deferred_texture
                .label(T2TerrainRenderStep::RenderDeferredTexture)
                .after(T2TerrainRenderStep::Tesselate)
                .after(TerrainRenderStep::RenderDeferredTexture),
        );
        runtime.frame_stage_mut(FrameStage::Render).add_system(
            Self::sys_accumulate_normal_and_color
                .label(T2TerrainRenderStep::AccumulateNormalsAndColor)
                .after(T2TerrainRenderStep::RenderDeferredTexture)
                .after(TerrainRenderStep::AccumulateNormalsAndColor)
                .before(WorldRenderStep::Render),
        );

        Ok(())
    }
}

#[inject_nitrous_resource]
impl T2TerrainBuffer {
    pub fn new(
        terrain: &TerrainBuffer,
        globals: &GlobalParametersBuffer,
        gpu: &Gpu,
    ) -> Result<Self> {
        let bind_group_layout =
            gpu.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("t2-tile-set-bind-group-layout"),
                    entries: &[
                        // T2 Info
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: NonZeroU64::new(T2Info::mem_size()),
                            },
                            count: None,
                        },
                        // Heights
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Texture {
                                multisampled: false,
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                        // Atlas
                        wgpu::BindGroupLayoutEntry {
                            binding: 3,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Texture {
                                multisampled: false,
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 4,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                        // Base color
                        wgpu::BindGroupLayoutEntry {
                            binding: 5,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Texture {
                                multisampled: false,
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 6,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                        // Index
                        //   Color - 32
                        //   Orientation - 8
                        //   Tile offset - 16
                        //
                        //     1) find lat/lon of the index pixel by interpolating, knowing the width/height
                        //     2) use the pixel extent to compute s/t within the tile
                        //     3) use orientation to map s/t
                        //     4) look up color in tile or use index's color
                        //
                        wgpu::BindGroupLayoutEntry {
                            binding: 7,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Texture {
                                multisampled: false,
                                sample_type: wgpu::TextureSampleType::Uint,
                                view_dimension: wgpu::TextureViewDimension::D2,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 8,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                            count: None,
                        },
                        // Frames
                        wgpu::BindGroupLayoutEntry {
                            binding: 9,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                    ],
                });

        let displace_height_pipeline =
            gpu.device()
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("t2-displace-height-pipeline"),
                    layout: Some(&gpu.device().create_pipeline_layout(
                        &wgpu::PipelineLayoutDescriptor {
                            label: Some("t2-displace-height-pipeline-layout"),
                            push_constant_ranges: &[],
                            bind_group_layouts: &[
                                terrain.mesh_bind_group_layout(),
                                &bind_group_layout,
                            ],
                        },
                    )),
                    module: &gpu.create_shader_module(
                        "displace_height.comp",
                        include_bytes!("../target/displace_height.comp.spirv"),
                    ),
                    entry_point: "main",
                });

        let accumulate_color_pipeline =
            gpu.device()
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("t2-accumulate-colors-pipeline"),
                    layout: Some(&gpu.device().create_pipeline_layout(
                        &wgpu::PipelineLayoutDescriptor {
                            label: Some("t2-accumulate-colors-pipeline-layout"),
                            push_constant_ranges: &[],
                            bind_group_layouts: &[
                                globals.bind_group_layout(),
                                terrain.accumulate_common_bind_group_layout(),
                                &bind_group_layout,
                            ],
                        },
                    )),
                    module: &gpu.create_shader_module(
                        "accumulate_colors.comp",
                        include_bytes!("../target/accumulate_colors.comp.spirv"),
                    ),
                    entry_point: "main",
                });

        Ok(Self {
            displace_height_pipeline,
            accumulate_color_pipeline,
            bind_group_layout,
            uploads: Vec::new(),
            // shared_adjustment,
            // layouts: HashMap::new(),
        })
    }

    fn sys_finish_uploads(
        mut t2_terrain: ResMut<T2TerrainBuffer>,
        gpu: Res<Gpu>,
        maybe_encoder: ResMut<Option<wgpu::CommandEncoder>>,
    ) {
        if let Some(encoder) = maybe_encoder.into_inner() {
            for mut upload in t2_terrain.uploads.drain(..) {
                // do atlas management
                upload.pic_uploader.expand_pics(encoder);
                upload.pic_uploader.finish_expand_pass();
                upload.atlas_packer.encode_frame_uploads(&gpu, encoder);
                // upload heights, base colors, and index textures
                for tex_upload in &[
                    upload.texture_uploads.height,
                    upload.texture_uploads.base_color,
                    upload.texture_uploads.index,
                ] {
                    encoder.copy_buffer_to_texture(
                        wgpu::ImageCopyBuffer {
                            buffer: &tex_upload.source,
                            layout: tex_upload.source_layout,
                        },
                        wgpu::ImageCopyTexture {
                            texture: &tex_upload.target,
                            mip_level: 0,
                            origin: wgpu::Origin3d::ZERO,
                            aspect: wgpu::TextureAspect::All,
                        },
                        tex_upload.copy_size,
                    )
                }
            }
        }
    }

    fn sys_encode_uploads(
        mut query: Query<&mut T2TileSet>,
        gpu: Res<Gpu>,
        maybe_encoder: ResMut<Option<wgpu::CommandEncoder>>,
    ) {
        if let Some(encoder) = maybe_encoder.into_inner() {
            for mut tile_set in query.iter_mut() {
                tile_set.encode_uploads(&gpu, encoder);
            }
        }
    }

    fn sys_paint_atlas_indices() {}

    fn sys_terrain_tesselate(
        t2_terrain: Res<T2TerrainBuffer>,
        query: Query<&T2TileSet>,
        terrain: ResMut<TerrainBuffer>,
        maybe_encoder: ResMut<Option<wgpu::CommandEncoder>>,
    ) {
        if let Some(encoder) = maybe_encoder.into_inner() {
            for tile_set in query.iter() {
                tile_set.displace_height(
                    &t2_terrain,
                    terrain.mesh_vertex_count(),
                    terrain.mesh_bind_group(),
                    encoder,
                );
            }
        }
    }

    fn sys_deferred_texture() {}

    fn sys_accumulate_normal_and_color(
        t2_terrain: Res<T2TerrainBuffer>,
        query: Query<&T2TileSet>,
        terrain: Res<TerrainBuffer>,
        globals: Res<GlobalParametersBuffer>,
        maybe_encoder: ResMut<Option<wgpu::CommandEncoder>>,
    ) {
        if let Some(encoder) = maybe_encoder.into_inner() {
            for tile_set in query.iter() {
                tile_set.accumulate_colors(
                    &t2_terrain,
                    terrain.accumulator_extent(),
                    &globals,
                    terrain.accumulate_common_bind_group(),
                    encoder,
                );
            }
        }
    }

    pub fn add_map(
        &mut self,
        system_palette: &Palette,
        mm: &MissionMap,
        catalog: &Catalog,
        gpu: &Gpu,
    ) -> Result<T2TileSet> {
        // We can't actually do uploading until we have an encoder at render time, so this needs
        // to create gpu buffers and textures, queue up uploads and blit lists, then during
        // sys_finish_loading, trigger pic_loader and atlas shaders, then build the final bind
        // groups.
        let t2_data = catalog.read_name(&mm.map_name().t2_name())?;
        let t2 = T2::from_bytes(t2_data.as_ref())?;

        // Do some deep magic to build up a palette.
        let palette = self._load_palette(system_palette, mm, catalog)?;

        // Queue uploads for all of the texture maps
        let (frame_map, frame_buffer, pic_uploader, atlas_packer) =
            self._build_atlas(&palette, mm, catalog, gpu)?;

        let texture_uploads = self._upload_heights_and_index(&palette, mm, &t2, &frame_map, gpu)?;

        let adjust = T2Adjustment::default();
        let mapper = T2Mapper::new(&t2, &adjust);
        let t2_info = T2Info::new(
            mapper.base_rad_f32(),
            mapper.extent_rad_f32(),
            texture_uploads.index_size,
        );
        let t2_info_buffer = gpu.push_data(
            "t2-info-buffer",
            &t2_info,
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        );

        let bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("t2-height-bind-group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &t2_info_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                // Heights
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&texture_uploads.height.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&texture_uploads.height.sampler),
                },
                // Atlas
                atlas_packer.texture_binding(3),
                atlas_packer.sampler_binding(4),
                // Base Color
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&texture_uploads.base_color.view),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(&texture_uploads.base_color.sampler),
                },
                // Index
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(&texture_uploads.index.view),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: wgpu::BindingResource::Sampler(&texture_uploads.index.sampler),
                },
                // Frames
                wgpu::BindGroupEntry {
                    binding: 9,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &frame_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
            ],
        });

        self.uploads.push(T2TileSetUpload {
            texture_uploads,
            pic_uploader,
            atlas_packer,
        });

        let tile_set = T2TileSet::new(t2, adjust, mapper, t2_info, t2_info_buffer, bind_group);

        Ok(tile_set)
    }

    fn _upload_heights_and_index(
        &mut self,
        palette: &Palette,
        mm: &MissionMap,
        t2: &T2,
        frame_map: &HashMap<TLoc, u16>,
        gpu: &Gpu,
    ) -> Result<TextureUploads> {
        let mut uploads = Vec::with_capacity(3);

        // Extract height samples and base colors to a buffer.
        let logical_extent = wgpu::Extent3d {
            width: t2.width(),
            height: t2.height(),
            depth_or_array_layers: 1,
        };
        let logical_stride = Gpu::stride_for_row_size(t2.width());
        let mut heights = vec![0u8; (logical_stride * t2.height()) as usize];
        let mut base_colors = vec![0u32; (logical_stride * t2.height()) as usize];
        for y in 0..t2.height() {
            for x in 0..t2.width() {
                let sample = t2.sample_at(x, y);
                heights[(logical_stride * y + x) as usize] = sample.height;
                base_colors[(logical_stride * y + x) as usize] =
                    palette.pack_unorm(sample.color as usize);
            }
        }

        // Pull index texture from the frames.
        ensure!(t2.width() % 4 == 0);
        ensure!(t2.height() % 4 == 0);
        let index_width = t2.width() / 4;
        let index_height = t2.height() / 4;
        let index_extent = wgpu::Extent3d {
            width: index_width,
            height: index_height,
            depth_or_array_layers: 1,
        };
        let index_stride = Gpu::stride_for_row_size(index_width);
        let mut indices = vec![[0u16; 2]; (index_stride * index_height) as usize];
        for zi in 0..index_height {
            for xi in 0..index_width {
                indices[(index_stride * zi + xi) as usize] =
                    if let Some(tmap) = mm.texture_map(xi * 4, zi * 4) {
                        [frame_map[&tmap.loc], tmap.orientation.as_byte() as u16]
                    } else {
                        [0, 0]
                    };
            }
        }

        // Upload heights
        let height_copy_buffer = gpu.push_buffer(
            "t2-height-tile-upload",
            &heights,
            wgpu::BufferUsages::COPY_SRC,
        );
        let height_format = wgpu::TextureFormat::R8Unorm;
        let height_texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("t2-height-map"),
            size: logical_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: height_format,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
        });
        let height_texture_view = height_texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("t2-height-map-view"),
            format: None,
            dimension: None,
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            mip_level_count: None,
            base_array_layer: 0,
            array_layer_count: None,
        });
        let height_sampler = gpu.device().create_sampler(&wgpu::SamplerDescriptor {
            label: Some("t2-height-map-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToBorder,
            address_mode_v: wgpu::AddressMode::ClampToBorder,
            address_mode_w: wgpu::AddressMode::ClampToBorder,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            lod_min_clamp: 0.0,
            lod_max_clamp: 0.0,
            compare: None,
            anisotropy_clamp: None,
            border_color: Some(wgpu::SamplerBorderColor::TransparentBlack),
        });
        uploads.push(TextureUploadOp {
            source: height_copy_buffer,
            source_layout: wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: NonZeroU32::new(texture_format_size(height_format) * logical_stride),
                rows_per_image: NonZeroU32::new(t2.height()),
            },
            target: height_texture,
            view: height_texture_view,
            sampler: height_sampler,
            copy_size: logical_extent,
        });

        // Upload base colors
        // FIXME: mipmap this!
        let base_color_copy_buffer = gpu.push_buffer(
            "t2-base-color-upload",
            base_colors.as_bytes(),
            wgpu::BufferUsages::COPY_SRC,
        );
        let base_color_format = wgpu::TextureFormat::Rgba8Unorm;
        let base_color_texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("t2-base-color"),
            size: logical_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: base_color_format,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
        });
        let base_color_texture_view =
            base_color_texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("t2-base-color-view"),
                format: None,
                dimension: None,
                aspect: wgpu::TextureAspect::All,
                base_mip_level: 0,
                mip_level_count: None,
                base_array_layer: 0,
                array_layer_count: None,
            });
        let base_color_sampler = gpu.device().create_sampler(&wgpu::SamplerDescriptor {
            label: Some("t2-base-color-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToBorder,
            address_mode_v: wgpu::AddressMode::ClampToBorder,
            address_mode_w: wgpu::AddressMode::ClampToBorder,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            lod_min_clamp: 0.0,
            lod_max_clamp: 0.0,
            compare: None,
            anisotropy_clamp: None,
            border_color: Some(wgpu::SamplerBorderColor::TransparentBlack),
        });
        uploads.push(TextureUploadOp {
            source: base_color_copy_buffer,
            source_layout: wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: NonZeroU32::new(
                    texture_format_size(base_color_format) * logical_stride,
                ),
                rows_per_image: NonZeroU32::new(t2.height()),
            },
            target: base_color_texture,
            view: base_color_texture_view,
            sampler: base_color_sampler,
            copy_size: logical_extent,
        });

        // Upload index
        let index_copy_buffer = gpu.push_buffer(
            "t2-index-upload",
            indices.as_bytes(),
            wgpu::BufferUsages::COPY_SRC,
        );
        let index_format = wgpu::TextureFormat::Rg16Uint;
        let index_texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("t2-index"),
            size: index_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: index_format,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
        });
        let index_texture_view = index_texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("t2-index-view"),
            format: None,
            dimension: None,
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            mip_level_count: None,
            base_array_layer: 0,
            array_layer_count: None,
        });
        let index_sampler = gpu.device().create_sampler(&wgpu::SamplerDescriptor {
            label: Some("t2-index-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToBorder,
            address_mode_v: wgpu::AddressMode::ClampToBorder,
            address_mode_w: wgpu::AddressMode::ClampToBorder,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            lod_min_clamp: 0.0,
            lod_max_clamp: 0.0,
            compare: None,
            anisotropy_clamp: None,
            border_color: Some(wgpu::SamplerBorderColor::TransparentBlack),
        });
        uploads.push(TextureUploadOp {
            source: index_copy_buffer,
            source_layout: wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: NonZeroU32::new(texture_format_size(index_format) * index_stride),
                rows_per_image: NonZeroU32::new(t2.height()),
            },
            target: index_texture,
            view: index_texture_view,
            sampler: index_sampler,
            copy_size: index_extent,
        });

        let mut out = uploads.drain(..);
        Ok(TextureUploads {
            height: out.next().unwrap(),
            base_color: out.next().unwrap(),
            index: out.next().unwrap(),
            index_size: [index_extent.width as f32, index_extent.height as f32],
        })
    }

    fn _load_palette(
        &self,
        system_palette: &Palette,
        mm: &MissionMap,
        catalog: &Catalog,
    ) -> Result<Palette> {
        let layer =
            Layer::from_bytes(catalog.read_name(mm.layer_name())?.as_ref(), system_palette)?;
        let layer_index = if mm.layer_index() != 0 {
            mm.layer_index()
        } else {
            2
        };

        let layer_data = layer.for_index(layer_index)?;
        let r0 = layer_data.slice(0x00, 0x10)?;
        let r1 = layer_data.slice(0x10, 0x20)?;
        let r2 = layer_data.slice(0x20, 0x30)?;
        let r3 = layer_data.slice(0x30, 0x40)?;

        // We need to put rows r0, r1, and r2 into into 0xC0, 0xE0, 0xF0 somehow.
        let mut palette = system_palette.clone();
        palette.overlay_at(&r1, 0xF0 - 1)?;
        palette.overlay_at(&r0, 0xE0 - 1)?;
        palette.overlay_at(&r3, 0xD0)?;
        palette.overlay_at(&r2, 0xC0)?;
        palette.override_one(0xFF, [0; 4]);

        Ok(palette)
    }

    fn _build_atlas(
        &self,
        palette: &Palette,
        mm: &MissionMap,
        catalog: &Catalog,
        gpu: &Gpu,
    ) -> Result<(
        HashMap<TLoc, u16>,
        wgpu::Buffer,
        PicUploader,
        AtlasPacker<Rgba<u8>>,
    )> {
        // De-duplicate the tmaps to get all unique TLocs for upload.
        let mut sources = HashSet::new();
        for tmap in mm.texture_maps() {
            if sources.contains(&tmap.loc) {
                continue;
            }
            sources.insert(tmap.loc.clone());
        }

        let num_across = (sources.len() as f64).sqrt().ceil() as u32;
        let extra = num_across * num_across - sources.len() as u32;
        let num_down = num_across - (extra / num_across);

        let patch_size = AtlasPacker::<Rgba<u8>>::align(257);
        let atlas_width0 = (num_across * patch_size) + num_across + 1;
        let atlas_stride = Gpu::stride_for_row_size(atlas_width0 * 4);
        let atlas_width = atlas_stride / 4;
        let atlas_height = (num_down * patch_size) + num_down + 1;

        // Build a texture atlas, doing all work on the gpu.
        let mut atlas_builder = AtlasPacker::<Rgba<u8>>::new(
            mm.map_name().meta_name(),
            gpu,
            atlas_width,
            atlas_height,
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::FilterMode::Nearest, // TODO: see if we can "improve" things with filtering?
        );

        let mut pic_uploader = PicUploader::new(gpu)?;
        pic_uploader.set_shared_palette(palette, gpu);

        // Set up upload for each TLoc, mapping it to a frame.
        let mut frames = Vec::new();
        let texture_base_name = mm.map_name().base_texture_name();
        for loc in sources.drain() {
            let name = loc.pic_file(texture_base_name);
            let data = catalog.read_name(name.as_ref())?;
            let (buffer, w, h, stride) =
                pic_uploader.upload(data.as_ref(), gpu, wgpu::BufferUsages::COPY_SRC)?;
            let frame = atlas_builder.push_buffer(buffer, w, h, stride)?;
            frames.push((loc, frame));
        }

        // For each TLoc, map the frame to the final size and upload that buffer to the GPU.
        // Our index texture will contain the offset into the frames. The frame at that offset
        // has the s/t needed to extract the right TMap from the atlas_texture.
        // Note: reserve index zero for NULL TMap. We use 0 instead of MAX so that we can `mix`
        //       with the index to determine our final color.
        let mut frame_refs = HashMap::new();
        let mut frame_buf = vec![[0f32; 4]; frames.len() + 1];
        for (i, (tloc, frame)) in frames.drain(..).enumerate() {
            frame_buf[i + 1] = [
                frame.s0(atlas_builder.width()),
                frame.s1(atlas_builder.width()),
                frame.t0(atlas_builder.height()),
                frame.t1(atlas_builder.height()),
            ];
            ensure!(i + 1 < u16::MAX as usize, "too many frames");
            frame_refs.insert(tloc, (i + 1) as u16);
        }
        let frame_buffer = gpu.push_buffer(
            "t2-frames",
            frame_buf.as_bytes(),
            wgpu::BufferUsages::STORAGE,
        );

        Ok((frame_refs, frame_buffer, pic_uploader, atlas_builder))
    }
}

#[derive(Component, NitrousComponent, Debug)]
#[Name = "t2_tile_set"]
pub struct T2TileSet {
    t2: T2,
    tile_adjustment: T2Adjustment,
    tile_mapper: T2Mapper,
    tile_info: T2Info,
    tile_info_buffer: wgpu::Buffer,
    tile_bind_group: wgpu::BindGroup,
}

#[inject_nitrous_component]
impl T2TileSet {
    pub(crate) fn new(
        t2: T2,
        tile_adjustment: T2Adjustment,
        tile_mapper: T2Mapper,
        tile_info: T2Info,
        tile_info_buffer: wgpu::Buffer,
        tile_bind_group: wgpu::BindGroup,
    ) -> Self {
        Self {
            t2,
            tile_adjustment,
            tile_mapper,
            tile_info,
            tile_info_buffer,
            tile_bind_group,
        }
    }

    fn encode_uploads(&mut self, gpu: &Gpu, encoder: &mut CommandEncoder) {
        if self.tile_adjustment.dirty {
            self.tile_adjustment.dirty = false;
            let mapper_p = T2Mapper::new(&self.t2, &self.tile_adjustment);
            let mut info = T2Info::new(
                mapper_p.base_rad_f32(),
                mapper_p.extent_rad_f32(),
                self.tile_info.index_size,
            );
            info.blend_factor = self.tile_adjustment.blend_factor;
            let new_info_buffer = gpu.push_data(
                "t2-info-upload",
                &info,
                wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_SRC,
            );
            encoder.copy_buffer_to_buffer(
                &new_info_buffer,
                0,
                &self.tile_info_buffer,
                0,
                mem::size_of::<T2Info>() as wgpu::BufferAddress,
            );
        }
    }

    fn displace_height(
        &self,
        t2_terrain: &T2TerrainBuffer,
        vertex_count: u32,
        mesh_bind_group: &wgpu::BindGroup,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("t2-displace-height-cpass"),
        });
        cpass.set_pipeline(&t2_terrain.displace_height_pipeline);
        cpass.set_bind_group(Group::TerrainDisplaceMesh.index(), mesh_bind_group, &[]);
        cpass.set_bind_group(
            Group::TerrainDisplaceTileSet.index(),
            &self.tile_bind_group,
            &[],
        );
        // FIXME: common this up somehow
        const WORKGROUP_WIDTH: u32 = 65536;
        let wg_x = (vertex_count % WORKGROUP_WIDTH).max(1);
        let wg_y = (vertex_count / WORKGROUP_WIDTH).max(1);
        cpass.dispatch(wg_x, wg_y, 1);
    }

    fn accumulate_colors(
        &self,
        t2_terrain: &T2TerrainBuffer,
        extent: &wgpu::Extent3d,
        globals: &GlobalParametersBuffer,
        accumulate_common_bind_group: &wgpu::BindGroup,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("t2-colors-acc-cpass"),
        });
        cpass.set_pipeline(&t2_terrain.accumulate_color_pipeline);
        cpass.set_bind_group(Group::Globals.index(), globals.bind_group(), &[]);
        cpass.set_bind_group(
            Group::TerrainAccumulateCommon.index(),
            accumulate_common_bind_group,
            &[],
        );
        cpass.set_bind_group(
            Group::TerrainAccumulateTileSet.index(),
            &self.tile_bind_group,
            &[],
        );
        cpass.dispatch(extent.width / 8, extent.height / 8, 1);
    }

    pub fn mapper(&self) -> &T2Mapper {
        &self.tile_mapper
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lib::{from_dos_string, Libs};
    use xt::TypeManager;

    #[test]
    fn it_can_load_all_t2() -> Result<()> {
        env_logger::init();

        let mut runtime = Gpu::for_test_unix()?;
        runtime
            .load_extension::<Libs>()?
            .load_extension::<GlobalParametersBuffer>()?
            .load_extension::<TerrainBuffer>()?;

        let libs = Libs::for_testing()?;
        for (game, palette, catalog) in libs.selected() {
            let mut ts = T2TerrainBuffer::new(
                runtime.resource::<TerrainBuffer>(),
                runtime.resource::<GlobalParametersBuffer>(),
                runtime.resource::<Gpu>(),
            )?;
            let type_manager = TypeManager::empty();

            for fid in catalog.find_with_extension("MM")? {
                let meta = catalog.stat(fid)?;
                println!("{}:{:13} @ {}", game.test_dir, meta.name(), meta.path());
                println!(
                    "{}",
                    "=".repeat(1 + game.test_dir.len() + meta.name().len())
                );
                if meta.name() == "$VARF.MM"
                    || (game.test_dir == "ATFGOLD"
                        && (meta.name() == "VIET.MM"
                            || meta.name() == "KURILE.MM"
                            || meta.name().contains("UKR")))
                {
                    println!("skipping broken asset");
                    continue;
                }

                let contents = from_dos_string(catalog.read(fid)?);
                let mm = MissionMap::from_str(contents.as_ref(), &type_manager, catalog)?;
                ts.add_map(palette, &mm, catalog, runtime.resource::<Gpu>())?;
            }
        }

        Ok(())
    }
}
