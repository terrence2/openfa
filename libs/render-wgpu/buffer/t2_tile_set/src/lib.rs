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
use absolute_unit::{degrees, radians};
use anyhow::{ensure, Result};
use atlas::{AtlasPacker, Frame};
use catalog::Catalog;
use global_data::GlobalParametersBuffer;
use gpu::wgpu::{BindGroup, ComputePass, Extent3d};
use gpu::{Gpu, UploadTracker};
use image::{GenericImage, Rgba, RgbaImage};
use lay::Layer;
use mm::{MissionMap, TLoc};
use pal::Palette;
use pic::{Pic, PicFormat};
use shader_shared::Group;
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    num::NonZeroU64,
    sync::Arc,
};
use t2::Terrain as T2Terrain;
use terrain::{TerrainBuffer, TileSet, VisiblePatch};
use tokio::{runtime::Runtime, sync::RwLock};
use zerocopy::AsBytes;

/*
/// Upload Pic files direct from mmap to GPU and depalettize on GPU, when possible.
#[derive(Debug, Default)]
struct PicUploader {}
impl PicUploader {
    fn upload(
        &mut self,
        palette: &Palette,
        data: &Cow<u8>,
        gpu: &Gpu,
        tracker: &mut UploadTracker,
    ) -> Result<()> {
        match Pic::read_format(data.as_bytes())? {
            PicFormat::JPEG | PicFormat::Format1 => {
                // Decode on CPU
                let img = Pic::decode(palette, data.as_bytes())?.to_rgba8();
                // Ensure width matches required width stride.
                let stride = Gpu::stride_for_row_size(img.width() * 4) / 4;
                let img = if stride == img.width() {
                    img
                } else {
                    let mut aligned_img = RgbaImage::new(stride, img.height());
                    aligned_img.copy_from(&img, 0, 0)?;
                    aligned_img
                };
                // Upload to GPU and copy to a new texture.
                let img_buffer = gpu.push_buffer(
                    "pic-uploader-img-buffer",
                    img.as_bytes(),
                    wgpu::BufferUsage::STORAGE,
                );
                // Create the texture.
                let size = wgpu::Extent3d {
                    width: img.width(),
                    height: img.height(),
                    depth: 1,
                };
                let texture = Arc::new(Box::new(gpu.device().create_texture(
                    &wgpu::TextureDescriptor {
                        label: Some("pic-uploader-img-texture"),
                        size,
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        usage: wgpu::TextureUsage::COPY_DST | wgpu::TextureUsage::SAMPLED,
                    },
                )));
                tracker.upload_to_texture(
                    img_buffer,
                    texture,
                    size,
                    wgpu::TextureFormat::Rgba8Unorm,
                    1,
                    wgpu::Origin3d::ZERO,
                );
            }
            PicFormat::Format0 => {
                // Decode on GPU
                let pic = Pic::from_bytes(data.as_bytes())?;
                ensure!(pic.palette().is_none(), "detected format0 with palette");

            }
        }
        Ok(())
    }
}
 */

#[derive(Debug)]
pub struct T2HeightTileSet {
    // Shared shader routine.
    displace_height_pipeline: wgpu::ComputePipeline,
    accumulate_color_pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,

    // One bind group for each t2 we want to render.
    bind_groups: HashMap<String, wgpu::BindGroup>,
}

impl T2HeightTileSet {
    pub fn new(
        terrain: &TerrainBuffer,
        globals_buffer: &GlobalParametersBuffer,
        gpu: &Gpu,
    ) -> Result<Self> {
        let bind_group_layout =
            gpu.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("t2-height-bind-group-layout"),
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
            bind_groups: HashMap::new(),
        })
    }

    fn _upload_heights(
        &mut self,
        t2: &T2Terrain,
        gpu: &mut Gpu,
        tracker: &mut UploadTracker,
    ) -> (wgpu::TextureView, wgpu::Sampler) {
        // Extract height samples to a buffer.
        let stride = Gpu::stride_for_row_size(t2.width());
        let mut heights = vec![0u8; (stride * t2.height()) as usize];
        for y in 0..t2.height() {
            for x in 0..t2.width() {
                heights[(stride * y + x) as usize] = t2.sample_at(x, y).height;
            }
        }
        let copy_buffer = gpu.push_buffer(
            "t2-height-tile-upload",
            &heights,
            wgpu::BufferUsage::COPY_SRC,
        );
        let extent = wgpu::Extent3d {
            width: stride,
            height: t2.height(),
            depth: 1,
        };
        let format = wgpu::TextureFormat::R8Uint;
        let height_texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("t2-height-map"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
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

        tracker.upload_to_texture(
            copy_buffer,
            Arc::new(Box::new(height_texture)),
            extent,
            format,
            1,
            wgpu::Origin3d::ZERO,
        );

        (height_texture_view, height_sampler)
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

        Ok(palette)
    }

    fn _build_atlas(
        &self,
        system_palette: &Palette,
        mm: &MissionMap,
        catalog: &Catalog,
        gpu: &mut Gpu,
        async_rt: &Runtime,
        tracker: &mut UploadTracker,
    ) -> Result<(
        HashMap<TLoc, Frame>,
        (Arc<Box<wgpu::Texture>>, wgpu::TextureView, wgpu::Sampler),
    )> {
        let palette = self._load_palette(system_palette, mm, catalog)?;

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

        const PATCH_SIZE: u32 = 256;
        let atlas_width0 = (num_across * PATCH_SIZE) + num_across + 1;
        let atlas_stride = Gpu::stride_for_row_size(atlas_width0 * 4);
        let atlas_width = atlas_stride / 4;
        let atlas_height = (num_down * PATCH_SIZE) + num_down + 1;

        // Build a texture atlas.
        let mut atlas_builder = AtlasPacker::<Rgba<u8>>::new(
            mm.map_name(),
            gpu,
            atlas_width,
            atlas_height,
            [0, 0, 0, 0],
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::FilterMode::Nearest, // TODO: see if we can "improve" things with filtering?
        )?;

        let mut frames = HashMap::new();
        let texture_base_name = mm.get_base_texture_name()?;
        for loc in sources.drain() {
            let name = loc.pic_file(&texture_base_name);
            let data = catalog.read_name_sync(&name)?;
            let pic = Pic::from_bytes(&data)?;
            ensure!(pic.format() == PicFormat::Format0);
            ensure!(pic.palette().is_none());
            let palettized = &data[pic.pixel_span()];
            let pic = Pic::decode(&palette, &data)?;
            let frame = atlas_builder.push_image(&pic.into_rgba8(), gpu)?;
            frames.insert(loc, frame);
        }

        Ok((frames, atlas_builder.finish(gpu, async_rt, tracker)?))
    }

    pub fn add_map(
        &mut self,
        system_palette: &Palette,
        mm: &MissionMap,
        t2: &T2Terrain,
        catalog: &Catalog,
        gpu: &mut Gpu,
        async_rt: &Runtime,
        tracker: &mut UploadTracker,
    ) -> Result<()> {
        if self.bind_groups.contains_key(t2.name()) {
            return Ok(());
        }

        let (height_texture_view, height_sampler) = self._upload_heights(t2, gpu, tracker);

        let (frames, (atlas_texture, atlas_texture_view, atlas_sampler)) =
            self._build_atlas(system_palette, mm, catalog, gpu, async_rt, tracker)?;

        // Build an index texture. This is the size of the height map, with one sample per square
        // indicating the properties of that square. We'll use this to index into frames, which let
        // us know the bounds and to select the orientation of that frame.

        // FIXME: Test with a lat/lon span as if we're at the equator, even though that's wrong
        //        we don't know what's right yet.
        let base_deg = t2.base_graticule_degrees();
        let base = [
            radians!(degrees!(base_deg[0])).f32(),
            radians!(degrees!(base_deg[1])).f32(),
        ];
        let extent = [
            radians!(degrees!(t2.extent_north_south_in_ft() / 364_000.)).f32(),
            radians!(degrees!(t2.extent_east_west_in_ft() / 288_200.)).f32(),
        ];
        let t2_info_buffer = gpu.push_data(
            "t2-info-buffer",
            &T2Info::new(base, extent),
            wgpu::BufferUsage::UNIFORM,
        );

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
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&height_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&height_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&atlas_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&atlas_sampler),
                },
            ],
        });

        assert!(!self.bind_groups.contains_key(t2.name()));
        self.bind_groups.insert(t2.name().to_owned(), bind_group);

        Ok(())
    }
}

impl TileSet for T2HeightTileSet {
    fn begin_update(&mut self) {}

    fn note_required(&mut self, _visible_patch: &VisiblePatch) {}

    fn finish_update(
        &mut self,
        _catalog: Arc<RwLock<Catalog>>,
        _async_rt: &Runtime,
        _gpu: &Gpu,
        _tracker: &mut UploadTracker,
    ) {
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
        for bind_group in self.bind_groups.values() {
            cpass.set_bind_group(Group::TerrainDisplaceTileSet.index(), bind_group, &[]);
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
        for bind_group in self.bind_groups.values() {
            cpass.set_bind_group(Group::TerrainAccumulateTileSet.index(), bind_group, &[]);
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
        let gpu = Gpu::new(&window, Default::default(), &mut interpreter.write())?;
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
            let mut ts = T2HeightTileSet::new(&terrain.read(), &globals.read(), &gpu.read())?;

            catalog.set_default_label(&label);
            let type_manager = TypeManager::empty();
            let contents = from_dos_string(catalog.read_sync(fid)?);
            let mm = MissionMap::from_str(&contents, &type_manager, &catalog)?;

            let t2_content = catalog.read_name_sync(mm.t2_name())?;
            let t2 = T2Terrain::from_bytes(&t2_content)?;

            let mut tracker = Default::default();
            ts.add_map(
                &system_palette,
                &mm,
                &t2,
                &catalog,
                &mut gpu.write(),
                &async_rt,
                &mut tracker,
            )?;
            let mut encoder =
                gpu.read()
                    .device()
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("upload-t2-info"),
                    });
            tracker.dispatch_uploads(&mut encoder);

            terrain
                .write()
                .add_tile_set(Box::new(ts) as Box<dyn TileSet>);
        }

        Ok(())
    }
}
