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
use anyhow::Result;
use catalog::Catalog;
use global_data::GlobalParametersBuffer;
use gpu::wgpu::{BindGroup, ComputePass, Extent3d};
use gpu::{Gpu, UploadTracker};
use shader_shared::Group;
use std::{collections::HashMap, num::NonZeroU64, sync::Arc};
use t2::Terrain as T2Terrain;
use terrain::{TerrainBuffer, TileSet, VisiblePatch};
use tokio::{runtime::Runtime, sync::RwLock};

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
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::Texture {
                                multisampled: false,
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::Sampler {
                                filtering: true,
                                comparison: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: NonZeroU64::new(T2Info::mem_size()),
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

    pub fn add_t2(&mut self, t2: &T2Terrain, gpu: &mut Gpu) {
        if self.bind_groups.contains_key(t2.name()) {
            return;
        }

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
        let texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("t2-height-map"),
            size: wgpu::Extent3d {
                width: stride,
                height: t2.height(),
                depth: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Uint,
            usage: wgpu::TextureUsage::COPY_DST | wgpu::TextureUsage::SAMPLED,
        });
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("t2-height-map-view"),
            format: None,
            dimension: None,
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            level_count: None,
            base_array_layer: 0,
            array_layer_count: None,
        });
        let sampler = gpu.device().create_sampler(&wgpu::SamplerDescriptor {
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

        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("upload-t2-height-tile"),
            });
        encoder.copy_buffer_to_texture(
            wgpu::BufferCopyView {
                buffer: &copy_buffer,
                layout: wgpu::TextureDataLayout {
                    offset: 0,
                    bytes_per_row: stride,
                    rows_per_image: t2.height(),
                },
            },
            wgpu::TextureCopyView {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x: 0, y: 0, z: 0 },
            },
            wgpu::Extent3d {
                width: stride,
                height: t2.height(),
                depth: 1,
            },
        );
        gpu.queue_mut().submit(vec![encoder.finish()]);

        let extent = [t2.extent_north_south_in_ft(), t2.extent_east_west_in_ft()];
        let t2_info_buffer = gpu.push_data(
            "t2-info-buffer",
            &T2Info::new([0.0, 0.0], extent),
            wgpu::BufferUsage::UNIFORM,
        );

        let bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("t2-height-bind-group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Buffer {
                        buffer: &t2_info_buffer,
                        offset: 0,
                        size: None,
                    },
                },
            ],
        });

        assert!(!self.bind_groups.contains_key(t2.name()));
        self.bind_groups.insert(t2.name().to_owned(), bind_group);
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
    use lib::CatalogBuilder;
    use nitrous::Interpreter;
    use terrain::{CpuDetailLevel, GpuDetailLevel};
    use winit::{event_loop::EventLoop, window::Window};

    #[test]
    fn it_can_load_all_t2() -> Result<()> {
        use winit::platform::unix::EventLoopExtUnix;
        let event_loop = EventLoop::<()>::new_any_thread();
        let window = Window::new(&event_loop)?;
        let interpreter = Interpreter::new();
        let gpu = Gpu::new(&window, Default::default(), &mut interpreter.write())?;

        let (catalog, inputs) = CatalogBuilder::build_and_select(&["*:*.T2".to_owned()])?;

        let globals = GlobalParametersBuffer::new(gpu.read().device(), &mut interpreter.write());
        let terrain = TerrainBuffer::new(
            &catalog,
            CpuDetailLevel::Low,
            GpuDetailLevel::Low,
            &globals.read(),
            &mut gpu.write(),
            &mut interpreter.write(),
        )?;
        let mut ts = T2HeightTileSet::new(&terrain.read(), &globals.read(), &gpu.read())?;

        for &fid in &inputs {
            let label = catalog.file_label(fid)?;
            let game = label.split(':').last().unwrap();
            let meta = catalog.stat_sync(fid)?;

            println!(
                "At: {}:{:13} @ {}",
                game,
                meta.name(),
                meta.path()
                    .map(|v| v.to_string_lossy())
                    .unwrap_or_else(|| "<none>".into())
            );

            let content = catalog.read_sync(fid)?;
            let t2 = T2Terrain::from_bytes(&content)?;
            ts.add_t2(&t2, &mut gpu.write())
        }

        terrain
            .write()
            .add_tile_set(Box::new(ts) as Box<dyn TileSet>);
        Ok(())
    }
}
