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
use anyhow::Result;
use gpu::{Gpu, UploadTracker};
use image::{GenericImage, RgbaImage};
use pal::Palette;
use pic::{Pic, PicFormat};
use std::{num::NonZeroU64, sync::Arc};
use zerocopy::AsBytes;

#[repr(C)]
#[derive(AsBytes, Debug, Copy, Clone)]
struct PicUploadInfo {
    width: u32,
    height: u32,
    target_stride_px: u32,
}

/// Upload Pic files direct from mmap to GPU and depalettize on GPU, when possible.
#[derive(Debug)]
pub struct PicUploader {
    depalettize_layout: wgpu::BindGroupLayout,
    depalettize_pipeline: wgpu::ComputePipeline,
    depalettize: Vec<(PicUploadInfo, wgpu::BindGroup)>,
}

impl PicUploader {
    pub fn new(gpu: &Gpu) -> Result<Self> {
        let depalettize_layout =
            gpu.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("pic-upload-bind-group-layout"),
                    entries: &[
                        // Info
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        // Palette
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
                        // Unpalettized, unaligned buffer.
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
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
                            binding: 3,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::StorageTexture {
                                access: wgpu::StorageTextureAccess::WriteOnly,
                                format: wgpu::TextureFormat::Rgba8Unorm,
                                view_dimension: wgpu::TextureViewDimension::D2,
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
        })
    }

    fn upload_cpu(
        &self,
        base_palette: &Palette,
        data: &[u8],
        gpu: &Gpu,
        tracker: &mut UploadTracker,
    ) -> Result<()> {
        // Decode on CPU
        let img = Pic::decode(base_palette, data)?.to_rgba8();
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
            wgpu::BufferUsage::COPY_SRC | wgpu::BufferUsage::STORAGE,
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
        Ok(())
    }

    pub fn upload(
        &mut self,
        base_palette: &Palette,
        data: &[u8],
        gpu: &Gpu,
        tracker: &mut UploadTracker,
    ) -> Result<()> {
        match Pic::read_format(data)? {
            PicFormat::Jpeg | PicFormat::Format1 => {
                self.upload_cpu(base_palette, data, gpu, tracker)?;
            }
            PicFormat::Format0 => {
                // Decode on GPU
                let pic = Pic::from_bytes(data)?;
                if let Some(_palette) = pic.palette() {
                    // TODO: own-palette version
                    self.upload_cpu(base_palette, data, gpu, tracker)?;
                } else {
                    let info = PicUploadInfo {
                        width: pic.width(),
                        height: pic.height(),
                        target_stride_px: Gpu::stride_for_row_size(pic.width() * 4) / 4,
                    };
                    let info_buffer = gpu.push_data(
                        "pic-upload-info",
                        &info,
                        wgpu::BufferUsage::COPY_SRC | wgpu::BufferUsage::UNIFORM,
                    );
                    let palette_buffer = gpu.push_slice(
                        "pic-upload-palette",
                        &base_palette.as_gpu_buffer(),
                        wgpu::BufferUsage::COPY_SRC | wgpu::BufferUsage::STORAGE,
                    );
                    let raw_buffer = gpu.push_buffer(
                        "pic-upload-palettized",
                        pic.raw_data(),
                        wgpu::BufferUsage::COPY_SRC | wgpu::BufferUsage::STORAGE,
                    );
                    let texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
                        label: Some("pic-upload-texture"),
                        size: wgpu::Extent3d {
                            width: info.target_stride_px,
                            height: pic.height(),
                            depth: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        usage: wgpu::TextureUsage::STORAGE | wgpu::TextureUsage::SAMPLED,
                    });
                    let bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("pic-upload-bind-group"),
                        layout: &self.depalettize_layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::Buffer {
                                    buffer: &info_buffer,
                                    offset: 0,
                                    size: None,
                                },
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::Buffer {
                                    buffer: &palette_buffer,
                                    offset: 0,
                                    size: NonZeroU64::new(4 * 256),
                                },
                            },
                            wgpu::BindGroupEntry {
                                binding: 2,
                                resource: wgpu::BindingResource::Buffer {
                                    buffer: &raw_buffer,
                                    offset: 0,
                                    size: NonZeroU64::new(pic.raw_data().len() as u64),
                                },
                            },
                            wgpu::BindGroupEntry {
                                binding: 3,
                                resource: wgpu::BindingResource::TextureView(&texture.create_view(
                                    &wgpu::TextureViewDescriptor {
                                        label: Some("pic-upload-target-texture-view"),
                                        format: None,
                                        dimension: None,
                                        aspect: wgpu::TextureAspect::All,
                                        base_mip_level: 0,
                                        level_count: None,
                                        base_array_layer: 0,
                                        array_layer_count: None,
                                    },
                                )),
                            },
                        ],
                    });
                    self.depalettize.push((info, bind_group));
                }
            }
        }
        Ok(())
    }

    pub fn dispatch_pass<'a>(
        &'a mut self,
        mut cpass: wgpu::ComputePass<'a>,
    ) -> Result<wgpu::ComputePass<'a>> {
        cpass.set_pipeline(&self.depalettize_pipeline);
        for (info, bind_group) in self.depalettize.iter() {
            cpass.set_bind_group(0, bind_group, &[]);
            cpass.dispatch(info.target_stride_px / 16, info.height / 16 + 1, 1)
        }
        Ok(cpass)
    }

    pub fn dispatch_singleton(&mut self, gpu: &mut Gpu, tracker: UploadTracker) -> Result<()> {
        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("test-encoder"),
            });
        tracker.dispatch_uploads(&mut encoder);
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
    use image::GenericImageView;
    use lib::CatalogBuilder;
    use nitrous::Interpreter;
    use std::time::Instant;
    use winit::{event_loop::EventLoop, window::Window};

    #[test]
    fn it_works() -> Result<()> {
        env_logger::init();

        use winit::platform::unix::EventLoopExtUnix;
        let event_loop = EventLoop::<()>::new_any_thread();
        let window = Window::new(&event_loop)?;
        let interpreter = Interpreter::new();
        let gpu = Gpu::new(&window, Default::default(), &mut interpreter.write())?;
        //let async_rt = Runtime::new()?;

        let mut printed_value = 0;

        let (mut catalog, inputs) = CatalogBuilder::build_and_select(&["*:*.PIC".to_owned()])?;
        let palette = Palette::from_bytes(&catalog.read_name_sync("PALETTE.PAL")?)?;
        let start = Instant::now();
        let mut uploader = PicUploader::new(&gpu.read())?;
        let mut tracker = UploadTracker::default();
        for &fid in &inputs {
            let label = catalog.file_label(fid)?;
            catalog.set_default_label(&label);
            let game = label.split(':').last().unwrap();
            let meta = catalog.stat_sync(fid)?;
            // // println!(
            // //     "At: {}:{:13} @ {}",
            // //     game,
            // //     meta.name(),
            // //     meta.path()
            // //         .map(|v| v.to_string_lossy())
            // //         .unwrap_or_else(|| "<none>".into())
            // // );
            let data = catalog.read_sync(fid)?;
            let pic = Pic::from_bytes(&data)?;
            println!(
                "At: {}:{:13} @ {:?} p {}",
                game,
                meta.name(),
                pic.format(),
                pic.palette().is_some(),
            );
            let img = Pic::decode(&palette, &data)?;
            printed_value += img.get_pixel(0, 0).0[0] as usize;
            // uploader.upload(&palette, &data, &gpu.read(), &mut tracker)?;
            printed_value += data[0] as usize;
        }
        println!("prepare time: {:?}", start.elapsed());

        let start = Instant::now();
        uploader.dispatch_singleton(&mut gpu.write(), tracker)?;
        println!("dispatch time: {:?}", start.elapsed());

        let start = Instant::now();
        gpu.read().device().poll(wgpu::Maintain::Wait);
        println!("execute time: {:?}", start.elapsed());

        println!("through line: {}", printed_value);

        Ok(())
    }
}
