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
                            visibility: wgpu::ShaderStage::COMPUTE,
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
                            visibility: wgpu::ShaderStage::COMPUTE,
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
                            visibility: wgpu::ShaderStage::COMPUTE,
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
                    )?,
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
            wgpu::BufferUsage::COPY_SRC | wgpu::BufferUsage::STORAGE,
        );
        self.shared_palette = Some(palette_buffer);
    }

    pub fn upload(
        &mut self,
        data: &[u8],
        gpu: &Gpu,
        usage: wgpu::BufferUsage,
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
                        wgpu::BufferUsage::COPY_SRC | wgpu::BufferUsage::STORAGE,
                    );
                    &owned
                } else {
                    ensure!(self.shared_palette.is_some());
                    self.shared_palette.as_ref().unwrap()
                };
                let raw_buffer = gpu.push_buffer(
                    "pic-upload-palettized",
                    &pic.pixel_data()?,
                    wgpu::BufferUsage::COPY_SRC | wgpu::BufferUsage::STORAGE,
                );
                let tgt_buffer_stride = Gpu::stride_for_row_size(pic.width() * 4);
                let tgt_buffer_size = (tgt_buffer_stride * pic.height()) as u64;
                let tgt_buffer = gpu.device().create_buffer(&wgpu::BufferDescriptor {
                    label: Some("pic-upload-tgt-buffer"),
                    size: tgt_buffer_size,
                    usage: usage | wgpu::BufferUsage::STORAGE,
                    mapped_at_creation: false,
                });
                let bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("pic-upload-bind-group"),
                    layout: &self.depalettize_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::Buffer {
                                buffer: &palette_buffer,
                                offset: 0,
                                size: NonZeroU64::new(4 * 256),
                            },
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Buffer {
                                buffer: &raw_buffer,
                                offset: 0,
                                size: NonZeroU64::new(pic.raw_data().len() as u64),
                            },
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Buffer {
                                buffer: &tgt_buffer,
                                offset: 0,
                                size: NonZeroU64::new(tgt_buffer_size),
                            },
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

    pub fn dispatch_pass<'a>(
        &'a mut self,
        mut cpass: wgpu::ComputePass<'a>,
    ) -> Result<wgpu::ComputePass<'a>> {
        cpass.set_pipeline(&self.depalettize_pipeline);
        for (dispatch_group_count, bind_group) in self.depalettize.iter() {
            cpass.set_bind_group(0, bind_group, &[]);
            cpass.dispatch(*dispatch_group_count, 1, 1)
        }
        Ok(cpass)
    }

    pub fn dispatch_singleton(&mut self, gpu: &mut Gpu) -> Result<()> {
        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("test-encoder"),
            });
        {
            let cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("test-encoder-compute-pass"),
            });
            self.dispatch_pass(cpass)?;
        }
        gpu.queue_mut().submit(vec![encoder.finish()]);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbaImage;
    use lib::CatalogBuilder;
    use nitrous::Interpreter;
    use std::{
        env,
        time::{Duration, Instant},
    };
    use tokio::runtime::Runtime;
    use winit::{event_loop::EventLoop, window::Window};
    use zerocopy::AsBytes;

    #[test]
    fn it_works_quickly() -> Result<()> {
        env_logger::init();

        use winit::platform::unix::EventLoopExtUnix;
        let event_loop = EventLoop::<()>::new_any_thread();
        let window = Window::new(&event_loop)?;
        let interpreter = Interpreter::new();
        let gpu = Gpu::new(window, Default::default(), &mut interpreter.write())?;

        let (mut catalog, inputs) = CatalogBuilder::build_and_select(&["*:*.PIC".to_owned()])?;
        let palette = Palette::from_bytes(&catalog.read_name_sync("PALETTE.PAL")?)?;
        let start = Instant::now();
        let mut uploader = PicUploader::new(&gpu.read())?;
        uploader.set_shared_palette(&palette, &gpu.read());
        for &fid in &inputs {
            let label = catalog.file_label(fid)?;
            catalog.set_default_label(&label);
            let data = catalog.read_sync(fid)?;
            let format = Pic::read_format(&data)?;
            if format == PicFormat::Jpeg {
                // Jpeg is not interesting for PicUploader since it's primary purpose is depalettizing.
                continue;
            }
            uploader.upload(&data, &gpu.read(), wgpu::BufferUsage::STORAGE)?;
        }
        println!("prepare time: {:?}", start.elapsed());

        let start = Instant::now();
        uploader.dispatch_singleton(&mut gpu.write())?;
        println!("dispatch time: {:?}", start.elapsed());

        let start = Instant::now();
        gpu.read().device().poll(wgpu::Maintain::Wait);
        println!("execute time: {:?}", start.elapsed());

        Ok(())
    }

    #[test]
    fn round_trip() -> Result<()> {
        env_logger::init();

        use winit::platform::unix::EventLoopExtUnix;
        let event_loop = EventLoop::<()>::new_any_thread();
        let window = Window::new(&event_loop)?;
        let interpreter = Interpreter::new();
        let gpu = Gpu::new(window, Default::default(), &mut interpreter.write())?;
        let async_rt = Runtime::new()?;

        let (mut catalog, inputs) = CatalogBuilder::build_and_select(&["FA:CATB.PIC".to_owned()])?;
        let palette = Palette::from_bytes(&catalog.read_name_sync("PALETTE.PAL")?)?;
        let mut uploader = PicUploader::new(&gpu.read())?;
        uploader.set_shared_palette(&palette, &gpu.read());
        let fid = inputs.first().unwrap();
        catalog.set_default_label(&catalog.file_label(*fid)?);
        let data = catalog.read_sync(*fid)?;
        let (buffer, width, height, _stride) =
            uploader.upload(&data, &gpu.read(), wgpu::BufferUsage::MAP_READ)?;
        uploader.dispatch_singleton(&mut gpu.write())?;
        gpu.read().device().poll(wgpu::Maintain::Wait);

        if env::var("DUMP") == Ok("1".to_owned()) {
            let task = async_rt.spawn(async move {
                let slice = buffer.slice(..);
                slice.map_async(wgpu::MapMode::Read).await.unwrap();
                let view = slice.get_mapped_range();
                let rgba = RgbaImage::from_raw(width, height, view.as_bytes().to_vec()).unwrap();
                rgba.save("../../../__dump__/test_pic_uploader_catb.png")
                    .unwrap();
            });
            for _ in 0..10 {
                gpu.read().device().poll(wgpu::Maintain::Wait);
                std::thread::sleep(Duration::from_millis(16));
            }
            async_rt.block_on(task)?;
        }

        Ok(())
    }
}
