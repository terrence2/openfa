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
use anyhow::{ensure, Result};
use gpu::Gpu;
use pal::Palette;
use pic::{Pic, PicFormat};
use std::num::NonZeroU64;

/// Upload Pic files direct from mmap to GPU and depalettize on GPU, when possible.
#[derive(Debug)]
pub struct PicUploader {
    depalettize_layout: wgpu::BindGroupLayout,
    depalettize_pipeline: wgpu::ComputePipeline,
    depalettize: Vec<(u32, wgpu::BindGroup)>,
    shared_palette: Option<wgpu::Buffer>,
}

// FIXME: this should be a resource
impl PicUploader {
    const GROUP_SIZE: u32 = 64;

    pub fn new(gpu: &Gpu) -> Result<Self> {
        let depalettize_layout =
            gpu.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("pic-upload-bind-group-layout"),
                    entries: &[
                        // Palette
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        // Unpalettized, unaligned buffer.
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        // Target, aligned, texture
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: false },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                    ],
                });
        let depalettize_pipeline =
            gpu.device()
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("pic-upload-pipeline"),
                    layout: Some(&gpu.device().create_pipeline_layout(
                        &wgpu::PipelineLayoutDescriptor {
                            label: Some("pic-upload-pipeline-layout"),
                            bind_group_layouts: &[&depalettize_layout],
                            push_constant_ranges: &[],
                        },
                    )),
                    module: &gpu.create_shader_module(
                        "pic_depalettize.comp.glsl",
                        include_bytes!("../target/pic_depalettize.comp.spirv"),
                    ),
                    entry_point: "main",
                });

        Ok(Self {
            depalettize_layout,
            depalettize_pipeline,
            depalettize: Vec::new(),
            shared_palette: None,
        })
    }

    pub fn set_shared_palette(&mut self, palette: &Palette, gpu: &Gpu) {
        let palette_buffer = gpu.push_slice(
            "pic-upload-shared-palette",
            &palette.as_gpu_buffer(),
            wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::STORAGE,
        );
        self.shared_palette = Some(palette_buffer);
    }

    pub fn upload(
        &mut self,
        data: &[u8],
        gpu: &Gpu,
        usage: wgpu::BufferUsages,
    ) -> Result<(wgpu::Buffer, u32, u32, u32)> {
        Ok(match Pic::read_format(data)? {
            PicFormat::Jpeg => {
                panic!("pic uploader jpeg support missing")
            }
            PicFormat::Format0 | PicFormat::Format1 => {
                // Decode on GPU
                let pic = Pic::from_bytes(data)?;
                let owned;
                let palette_buffer = if let Some(own_palette) = pic.palette() {
                    owned = gpu.push_slice(
                        "pic-upload-own-palette",
                        &own_palette.as_gpu_buffer(),
                        wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::STORAGE,
                    );
                    &owned
                } else {
                    ensure!(self.shared_palette.is_some());
                    self.shared_palette.as_ref().unwrap()
                };
                let raw_buffer = gpu.push_buffer(
                    "pic-upload-palettized",
                    &pic.pixel_data()?,
                    wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::STORAGE,
                );
                let tgt_buffer_stride = Gpu::stride_for_row_size(pic.width() * 4);
                let tgt_buffer_size = (tgt_buffer_stride * pic.height()) as u64;
                let tgt_buffer = gpu.device().create_buffer(&wgpu::BufferDescriptor {
                    label: Some("pic-upload-tgt-buffer"),
                    size: tgt_buffer_size,
                    usage: usage | wgpu::BufferUsages::STORAGE,
                    mapped_at_creation: false,
                });
                let bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("pic-upload-bind-group"),
                    layout: &self.depalettize_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                buffer: palette_buffer,
                                offset: 0,
                                size: NonZeroU64::new(4 * 256),
                            }),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                buffer: &raw_buffer,
                                offset: 0,
                                size: NonZeroU64::new(pic.raw_data().len() as u64),
                            }),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                buffer: &tgt_buffer,
                                offset: 0,
                                size: NonZeroU64::new(tgt_buffer_size),
                            }),
                        },
                    ],
                });
                let group_count = (pic.raw_data().len() as u32 / 4 + Self::GROUP_SIZE - 1)
                    & !(Self::GROUP_SIZE as u32 - 1);
                self.depalettize.push((group_count, bind_group));
                (tgt_buffer, pic.width(), pic.height(), tgt_buffer_stride)
            }
        })
    }

    /// Call to encode a compute pass to expand all pics we've uploaded this frame.
    pub fn expand_pics(&self, encoder: &mut wgpu::CommandEncoder) {
        let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("pic-uploader-expand-pics-compute-pass"),
        });
        cpass.set_pipeline(&self.depalettize_pipeline);
        for (dispatch_group_count, bind_group) in self.depalettize.iter() {
            cpass.set_bind_group(0, bind_group, &[]);
            const WORKGROUP_WIDTH: u32 = 65536;
            let wg_x = (dispatch_group_count % WORKGROUP_WIDTH).max(1);
            let wg_y = (dispatch_group_count / WORKGROUP_WIDTH).max(1);
            cpass.dispatch_workgroups(wg_x, wg_y, 1)
        }
    }

    /// Call after every frame to make sure we don't re-do work next frame.
    pub fn finish_expand_pass(&mut self) {
        self.depalettize.clear();
    }

    #[cfg(test)]
    fn dispatch_singleton(&mut self, gpu: &mut Gpu) {
        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("test-encoder"),
            });
        {
            self.expand_pics(&mut encoder);
        }
        gpu.queue_mut().submit(vec![encoder.finish()]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbaImage;
    use lib::Libs;
    use parking_lot::Mutex;
    use std::{env, sync::Arc, time::Instant};
    use zerocopy::AsBytes;

    #[test]
    fn it_works_quickly() -> Result<()> {
        let mut runtime = Gpu::for_test()?;
        let mut gpu = runtime.resource_mut::<Gpu>();

        let catalogs = Libs::for_testing()?;
        let mut uploader = PicUploader::new(&gpu)?;
        let start = Instant::now();
        for (_game, palette, catalog) in catalogs.selected() {
            uploader.set_shared_palette(palette, &gpu);
            for fid in catalog.find_with_extension("PIC")? {
                let data = catalog.read(fid)?;
                let format = Pic::read_format(data.as_ref())?;
                if format == PicFormat::Jpeg {
                    // Jpeg is not interesting for PicUploader since it's primary purpose is depalettizing.
                    continue;
                }
                uploader.upload(data.as_ref(), &gpu, wgpu::BufferUsages::STORAGE)?;
            }
        }
        println!("prepare time: {:?}", start.elapsed());

        let start = Instant::now();
        uploader.dispatch_singleton(&mut gpu);
        println!("dispatch time: {:?}", start.elapsed());

        let start = Instant::now();
        gpu.device().poll(wgpu::Maintain::Wait);
        println!("execute time: {:?}", start.elapsed());

        Ok(())
    }

    #[test]
    fn round_trip() -> Result<()> {
        let mut runtime = Gpu::for_test()?;
        let mut gpu = runtime.resource_mut::<Gpu>();

        let libs = Libs::for_testing()?;
        let catalog = libs.catalog();
        let palette = libs.palette();

        let mut uploader = PicUploader::new(&gpu)?;
        uploader.set_shared_palette(palette, &gpu);
        let data = catalog.read_name("CATB.PIC")?;
        let (buffer, width, height, _stride) =
            uploader.upload(&data, &gpu, wgpu::BufferUsages::MAP_READ)?;
        uploader.dispatch_singleton(&mut gpu);
        gpu.device().poll(wgpu::Maintain::Wait);

        if env::var("DUMP") == Ok("1".to_owned()) {
            let waiter = Arc::new(Mutex::new(false));
            let waiter_ref = waiter.clone();
            buffer.slice(..).map_async(wgpu::MapMode::Read, move |err| {
                err.expect("failed to read back texture");
                *waiter_ref.lock() = true;
            });
            while !*waiter.lock() {
                gpu.device().poll(wgpu::Maintain::Wait);
            }
            let view = buffer.slice(..).get_mapped_range();
            let rgba = RgbaImage::from_raw(width, height, view.as_bytes().to_vec()).unwrap();
            rgba.save("../../../__dump__/test_pic_uploader_catb.png")
                .unwrap();
        }

        Ok(())
    }
}
