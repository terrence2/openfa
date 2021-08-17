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
use camera::Camera;
use catalog::Catalog;
use geodesy::{GeoSurface, Graticule};
use global_data::GlobalParametersBuffer;
use gpu::{texture_format_size, ArcTextureCopyView, Gpu, OwnedBufferCopyView, UploadTracker};
use image::Rgba;
use lay::Layer;
use log::warn;
use mm::{MissionMap, TLoc};
use nalgebra::Point3;
use pal::Palette;
use parking_lot::RwLock;
use pic_uploader::PicUploader;
use shader_shared::Group;
use std::{
    collections::{HashMap, HashSet},
    mem,
    num::NonZeroU64,
    sync::Arc,
};
use t2::Terrain as T2Terrain;
use terrain::{TerrainBuffer, TileSet, VisiblePatch};
use tokio::{runtime::Runtime, sync::RwLock as AsyncRwLock};
use wgpu::{BindGroup, ComputePass, Extent3d};
use zerocopy::AsBytes;

#[derive(Debug)]
pub struct T2Adjustment {
    pub base_offset: [Angle<Degrees>; 2],
    pub span_offset: [Length<Meters>; 2],
    pub blend_factor: f32,
}

impl Default for T2Adjustment {
    fn default() -> Self {
        Self {
            base_offset: [degrees!(0); 2],
            span_offset: [meters!(0); 2],
            blend_factor: 1.0,
        }
    }
}

/// Converts between FightersAnthology cartesian offsets within a tile
/// to Geodesic coordinates for use with the nitrogen engine.
pub struct T2Mapper {
    base: [Angle<Degrees>; 2],
    extent: [Angle<Degrees>; 2],
    extent_ft: [f32; 2],
}

impl T2Mapper {
    pub fn new(t2: &T2Terrain, adjust: &T2Adjustment) -> Self {
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
        let lon = -((self.extent[1] * lon_f / lat.cos() as f32) + self.base[1]);
        Graticule::new(lat, lon, meters!(offset_from_ground))
    }
}

#[derive(Debug)]
pub struct T2LayoutInfo {
    t2_info: T2Info,
    t2: T2Terrain,
    adjust: Arc<RwLock<T2Adjustment>>,
    t2_info_buffer: Arc<Box<wgpu::Buffer>>,
    bind_group: wgpu::BindGroup,
}

impl T2LayoutInfo {
    fn new(
        t2_info: T2Info,
        t2: T2Terrain,
        adjust: Arc<RwLock<T2Adjustment>>,
        t2_info_buffer: Arc<Box<wgpu::Buffer>>,
        bind_group: wgpu::BindGroup,
    ) -> Self {
        Self {
            t2_info,
            t2,
            adjust,
            t2_info_buffer,
            bind_group,
        }
    }
}

#[derive(Debug)]
pub struct T2TileSet {
    // Shared shader routine.
    displace_height_pipeline: wgpu::ComputePipeline,
    accumulate_color_pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,

    // One bind group for each t2 we want to render.
    shared_adjustment: Arc<RwLock<T2Adjustment>>,
    layouts: HashMap<String, T2LayoutInfo>,
}

impl T2TileSet {
    pub fn new(
        shared_adjustment: Arc<RwLock<T2Adjustment>>,
        terrain: &TerrainBuffer,
        globals_buffer: &GlobalParametersBuffer,
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
                            visibility: wgpu::ShaderStage::COMPUTE,
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
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::Texture {
                                multisampled: false,
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::Sampler {
                                filtering: true,
                                comparison: false,
                            },
                            count: None,
                        },
                        // Atlas
                        wgpu::BindGroupLayoutEntry {
                            binding: 3,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::Texture {
                                multisampled: false,
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 4,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::Sampler {
                                filtering: true,
                                comparison: false,
                            },
                            count: None,
                        },
                        // Base color
                        wgpu::BindGroupLayoutEntry {
                            binding: 5,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::Texture {
                                multisampled: false,
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 6,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::Sampler {
                                filtering: true,
                                comparison: false,
                            },
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
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::Texture {
                                multisampled: false,
                                sample_type: wgpu::TextureSampleType::Uint,
                                view_dimension: wgpu::TextureViewDimension::D2,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 8,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::Sampler {
                                filtering: false,
                                comparison: false,
                            },
                            count: None,
                        },
                        // Frames
                        wgpu::BindGroupLayoutEntry {
                            binding: 9,
                            visibility: wgpu::ShaderStage::COMPUTE,
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
                            label: Some("terrain-displace-height-pipeline-layout"),
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
                    )?,
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
                                globals_buffer.bind_group_layout(),
                                terrain.accumulate_common_bind_group_layout(),
                                &bind_group_layout,
                            ],
                        },
                    )),
                    module: &gpu.create_shader_module(
                        "accumulate_spherical_colors.comp",
                        include_bytes!("../target/accumulate_colors.comp.spirv"),
                    )?,
                    entry_point: "main",
                });

        Ok(Self {
            displace_height_pipeline,
            accumulate_color_pipeline,
            bind_group_layout,
            shared_adjustment,
            layouts: HashMap::new(),
        })
    }

    fn _upload_heights_and_index(
        &mut self,
        palette: &Palette,
        mm: &MissionMap,
        t2: &T2Terrain,
        frame_map: &HashMap<TLoc, u16>,
        gpu: &mut Gpu,
        tracker: &mut UploadTracker,
    ) -> Result<(
        (wgpu::TextureView, wgpu::Sampler),
        (wgpu::TextureView, wgpu::Sampler),
        (wgpu::TextureView, wgpu::Sampler, [f32; 2]),
    )> {
        // Extract height samples and base colors to a buffer.
        let logical_extent = wgpu::Extent3d {
            width: t2.width(),
            height: t2.height(),
            depth: 1,
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
            depth: 1,
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
            wgpu::BufferUsage::COPY_SRC,
        );
        let height_format = wgpu::TextureFormat::R8Unorm;
        let height_texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("t2-height-map"),
            size: logical_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: height_format,
            usage: wgpu::TextureUsage::COPY_DST | wgpu::TextureUsage::SAMPLED,
        });
        let height_texture_view = height_texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("t2-height-map-view"),
            format: None,
            dimension: None,
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            level_count: None,
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
        tracker.copy_owned_buffer_to_arc_texture(
            OwnedBufferCopyView {
                buffer: height_copy_buffer,
                layout: wgpu::TextureDataLayout {
                    offset: 0,
                    bytes_per_row: texture_format_size(height_format) * logical_stride,
                    rows_per_image: t2.height(),
                },
            },
            ArcTextureCopyView {
                texture: Arc::new(Box::new(height_texture)),
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
            },
            logical_extent,
        );

        // Upload base colors
        // FIXME: mipmap this!
        let base_color_copy_buffer = gpu.push_buffer(
            "t2-base-color-upload",
            &base_colors.as_bytes(),
            wgpu::BufferUsage::COPY_SRC,
        );
        let base_color_format = wgpu::TextureFormat::Rgba8Unorm;
        let base_color_texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("t2-base-color"),
            size: logical_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: base_color_format,
            usage: wgpu::TextureUsage::COPY_DST | wgpu::TextureUsage::SAMPLED,
        });
        let base_color_texture_view =
            base_color_texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("t2-base-color-view"),
                format: None,
                dimension: None,
                aspect: wgpu::TextureAspect::All,
                base_mip_level: 0,
                level_count: None,
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
        tracker.copy_owned_buffer_to_arc_texture(
            OwnedBufferCopyView {
                buffer: base_color_copy_buffer,
                layout: wgpu::TextureDataLayout {
                    offset: 0,
                    bytes_per_row: texture_format_size(base_color_format) * logical_stride,
                    rows_per_image: t2.height(),
                },
            },
            ArcTextureCopyView {
                texture: Arc::new(Box::new(base_color_texture)),
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
            },
            logical_extent,
        );

        // Upload index
        let index_copy_buffer = gpu.push_buffer(
            "t2-index-upload",
            &indices.as_bytes(),
            wgpu::BufferUsage::COPY_SRC,
        );
        let index_format = wgpu::TextureFormat::Rg16Uint;
        let index_texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("t2-index"),
            size: index_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: index_format,
            usage: wgpu::TextureUsage::COPY_DST | wgpu::TextureUsage::SAMPLED,
        });
        let index_texture_view = index_texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("t2-index-view"),
            format: None,
            dimension: None,
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            level_count: None,
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
        tracker.copy_owned_buffer_to_arc_texture(
            OwnedBufferCopyView {
                buffer: index_copy_buffer,
                layout: wgpu::TextureDataLayout {
                    offset: 0,
                    bytes_per_row: texture_format_size(index_format) * index_stride,
                    rows_per_image: t2.height(),
                },
            },
            ArcTextureCopyView {
                texture: Arc::new(Box::new(index_texture)),
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
            },
            index_extent,
        );

        Ok((
            (height_texture_view, height_sampler),
            (base_color_texture_view, base_color_sampler),
            (
                index_texture_view,
                index_sampler,
                [(index_width) as f32, (index_height) as f32],
            ),
        ))
    }

    fn _load_palette(
        &self,
        system_palette: &Palette,
        mm: &MissionMap,
        catalog: &Catalog,
    ) -> Result<Palette> {
        let layer = Layer::from_bytes(&catalog.read_name_sync(&mm.layer_name())?, &system_palette)?;
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
        gpu: &mut Gpu,
        async_rt: &Runtime,
        tracker: &mut UploadTracker,
    ) -> Result<(
        HashMap<TLoc, u16>,
        wgpu::Buffer,
        (wgpu::TextureView, wgpu::Sampler),
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
            mm.map_name(),
            gpu,
            atlas_width,
            atlas_height,
            [0, 0, 0, 0],
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::FilterMode::Nearest, // TODO: see if we can "improve" things with filtering?
        )?;

        let mut uploader = PicUploader::new(gpu)?;
        uploader.set_shared_palette(&palette, gpu);

        // Set up upload for each TLoc, mapping it to a frame.
        let mut frames = Vec::new();
        let texture_base_name = mm.get_base_texture_name()?;
        for loc in sources.drain() {
            let name = loc.pic_file(&texture_base_name);
            let data = catalog.read_name_sync(&name)?;
            let (buffer, w, h) = uploader.upload(&data, gpu, wgpu::BufferUsage::STORAGE)?;
            let frame = atlas_builder.push_buffer(buffer, w, h, gpu)?;
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
            wgpu::BufferUsage::STORAGE,
        );

        uploader.dispatch_singleton(gpu)?;
        let (_, view, sampler) = atlas_builder.finish(gpu, async_rt, tracker)?;

        Ok((frame_refs, frame_buffer, (view, sampler)))
    }

    pub fn add_map(
        &mut self,
        system_palette: &Palette,
        mm: &MissionMap,
        catalog: &Catalog,
        gpu: &mut Gpu,
        async_rt: &Runtime,
        tracker: &mut UploadTracker,
    ) -> Result<T2Mapper> {
        let t2_data = catalog.read_name_sync(mm.t2_name())?;
        let t2 = T2Terrain::from_bytes(&t2_data)?;

        if self.layouts.contains_key(t2.name()) {
            warn!("Skipping duplicate add_map for {}", t2.name());
            let layout = &self.layouts[t2.name()];
            return Ok(T2Mapper::new(&layout.t2, &layout.adjust.read()));
        }

        let palette = self._load_palette(system_palette, mm, catalog)?;

        let (frame_map, frame_buffer, (atlas_texture_view, atlas_sampler)) =
            self._build_atlas(&palette, mm, catalog, gpu, async_rt, tracker)?;

        let (
            (height_texture_view, height_sampler),
            (base_color_texture_view, base_color_sampler),
            (index_texture_view, index_sampler, index_size),
        ) = self._upload_heights_and_index(&palette, &mm, &t2, &frame_map, gpu, tracker)?;

        let mapper = T2Mapper::new(&t2, &self.shared_adjustment.read());
        let t2_info = T2Info::new(mapper.base_rad_f32(), mapper.extent_rad_f32(), index_size);
        let t2_info_buffer = Arc::new(Box::new(gpu.push_data(
            "t2-info-buffer",
            &t2_info,
            wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST,
        )));

        let bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("t2-height-bind-group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer {
                        buffer: &t2_info_buffer,
                        offset: 0,
                        size: None,
                    },
                },
                // Heights
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&height_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&height_sampler),
                },
                // Atlas
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&atlas_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&atlas_sampler),
                },
                // Base Color
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&base_color_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(&base_color_sampler),
                },
                // Index
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(&index_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: wgpu::BindingResource::Sampler(&index_sampler),
                },
                // Frames
                wgpu::BindGroupEntry {
                    binding: 9,
                    resource: wgpu::BindingResource::Buffer {
                        buffer: &frame_buffer,
                        offset: 0,
                        size: None,
                    },
                },
            ],
        });

        self.layouts.insert(
            t2.name().to_owned(),
            T2LayoutInfo::new(
                t2_info,
                t2,
                self.shared_adjustment.clone(),
                t2_info_buffer,
                bind_group,
            ),
        );

        Ok(mapper)
    }
}

impl TileSet for T2TileSet {
    fn begin_update(&mut self) {}

    fn note_required(&mut self, _visible_patch: &VisiblePatch) {}

    fn finish_update(
        &mut self,
        _camera: &Camera,
        _catalog: Arc<AsyncRwLock<Catalog>>,
        _async_rt: &Runtime,
        gpu: &Gpu,
        tracker: &mut UploadTracker,
    ) {
        for layout in self.layouts.values() {
            let mapper_p = T2Mapper::new(&layout.t2, &layout.adjust.read());
            let mut info = T2Info::new(
                mapper_p.base_rad_f32(),
                mapper_p.extent_rad_f32(),
                layout.t2_info.index_size,
            );
            info.blend_factor = layout.adjust.read().blend_factor;
            let new_info_buffer = gpu.push_data(
                "t2-info-upload",
                &info,
                wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_SRC,
            );
            tracker.upload(
                new_info_buffer,
                layout.t2_info_buffer.clone(),
                mem::size_of::<T2Info>(),
            )
        }
    }

    fn snapshot_index(&mut self, _async_rt: &Runtime, _gpu: &mut Gpu) {}

    fn paint_atlas_index(&self, _encoder: &mut wgpu::CommandEncoder) {}

    fn displace_height<'a>(
        &'a self,
        vertex_count: u32,
        mesh_bind_group: &'a wgpu::BindGroup,
        mut cpass: wgpu::ComputePass<'a>,
    ) -> Result<wgpu::ComputePass<'a>> {
        cpass.set_pipeline(&self.displace_height_pipeline);
        cpass.set_bind_group(Group::TerrainDisplaceMesh.index(), mesh_bind_group, &[]);
        for layout in self.layouts.values() {
            cpass.set_bind_group(
                Group::TerrainDisplaceTileSet.index(),
                &layout.bind_group,
                &[],
            );
            cpass.dispatch(vertex_count, 1, 1);
        }
        Ok(cpass)
    }

    fn accumulate_normals<'a>(
        &'a self,
        _extent: &Extent3d,
        _globals_buffer: &'a GlobalParametersBuffer,
        _accumulate_common_bind_group: &'a BindGroup,
        cpass: ComputePass<'a>,
    ) -> Result<ComputePass<'a>> {
        Ok(cpass)
    }

    fn accumulate_colors<'a>(
        &'a self,
        extent: &Extent3d,
        globals_buffer: &'a GlobalParametersBuffer,
        accumulate_common_bind_group: &'a BindGroup,
        mut cpass: ComputePass<'a>,
    ) -> Result<ComputePass<'a>> {
        cpass.set_pipeline(&self.accumulate_color_pipeline);
        cpass.set_bind_group(Group::Globals.index(), globals_buffer.bind_group(), &[]);
        cpass.set_bind_group(
            Group::TerrainAccumulateCommon.index(),
            accumulate_common_bind_group,
            &[],
        );
        for layout in self.layouts.values() {
            cpass.set_bind_group(
                Group::TerrainAccumulateTileSet.index(),
                &layout.bind_group,
                &[],
            );
            cpass.dispatch(extent.width / 8, extent.height / 8, 1);
        }
        Ok(cpass)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lib::{from_dos_string, CatalogBuilder};
    use nitrous::Interpreter;
    use terrain::{CpuDetailLevel, GpuDetailLevel};
    use winit::{event_loop::EventLoop, window::Window};
    use xt::TypeManager;

    #[test]
    fn it_can_load_all_t2() -> Result<()> {
        env_logger::init();

        use winit::platform::unix::EventLoopExtUnix;
        let event_loop = EventLoop::<()>::new_any_thread();
        let window = Window::new(&event_loop)?;
        let interpreter = Interpreter::new();
        let gpu = Gpu::new(window, Default::default(), &mut interpreter.write())?;
        let async_rt = Runtime::new()?;

        let (mut catalog, inputs) = CatalogBuilder::build_and_select(&["*:*.MM".to_owned()])?;

        for &fid in &inputs {
            let label = catalog.file_label(fid)?;
            let game = label.split(':').last().unwrap();
            let meta = catalog.stat_sync(fid)?;

            let system_palette =
                Palette::from_bytes(&catalog.read_labeled_name_sync(&label, "PALETTE.PAL")?)?;

            if meta.name() == "$VARF.MM"
                || (game == "ATFGOLD"
                    && (meta.name() == "VIET.MM"
                        || meta.name() == "KURILE.MM"
                        || meta.name().contains("UKR")))
            {
                continue;
            }

            println!(
                "At: {}:{:13} @ {}",
                game,
                meta.name(),
                meta.path()
                    .map(|v| v.to_string_lossy())
                    .unwrap_or_else(|| "<none>".into())
            );

            let globals =
                GlobalParametersBuffer::new(gpu.read().device(), &mut interpreter.write());
            let terrain = TerrainBuffer::new(
                &catalog,
                CpuDetailLevel::Low,
                GpuDetailLevel::Low,
                &globals.read(),
                &mut gpu.write(),
                &mut interpreter.write(),
            )?;
            let t2_adjustment = Arc::new(RwLock::new(T2Adjustment::default()));
            let mut ts =
                T2TileSet::new(t2_adjustment, &terrain.read(), &globals.read(), &gpu.read())?;

            catalog.set_default_label(&label);
            let type_manager = TypeManager::empty();
            let contents = from_dos_string(catalog.read_sync(fid)?);
            let mm = MissionMap::from_str(&contents, &type_manager, &catalog)?;
            let mut tracker = Default::default();
            ts.add_map(
                &system_palette,
                &mm,
                &catalog,
                &mut gpu.write(),
                &async_rt,
                &mut tracker,
            )?;
            tracker.dispatch_uploads_one_shot(&mut gpu.write());
            terrain
                .write()
                .add_tile_set(Box::new(ts) as Box<dyn TileSet>);
        }

        Ok(())
    }
}
