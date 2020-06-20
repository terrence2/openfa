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

// All code in this module is heavily inspired by -- and all too
// frequently directly copied from -- the most excellent:
//     https://ebruneton.github.io/precomputed_atmospheric_scattering/
// Which is:
//     Copyright (c) 2017 Eric Bruneton
// All errors and omissions below were introduced in transcription
// to Rust/Vulkan/wgpu and are not reflective of the high quality of the
// original work in any way.
mod colorspace;
mod earth_consts;
mod precompute;

use crate::{earth_consts::ATMOSPHERE_PARAMETERS_BUFFER_SIZE, precompute::Precompute};
use failure::Fallible;
use frame_graph::FrameStateTracker;
use gpu::GPU;
use log::trace;
use nalgebra::Vector3;
use std::{cell::RefCell, mem, sync::Arc, time::Instant};

const NUM_PRECOMPUTED_WAVELENGTHS: usize = 40;
const NUM_SCATTERING_ORDER: usize = 4;

pub struct AtmosphereBuffer {
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,

    sun_direction_buffer: Arc<Box<wgpu::Buffer>>,
}

impl AtmosphereBuffer {
    pub fn new(gpu: &mut GPU) -> Fallible<Arc<RefCell<Self>>> {
        trace!("AtmosphereBuffer::new");

        let precompute_start = Instant::now();
        let (
            atmosphere_params_buffer,
            transmittance_texture,
            irradiance_texture,
            scattering_texture,
            single_mie_scattering_texture,
        ) = Precompute::precompute(NUM_PRECOMPUTED_WAVELENGTHS, NUM_SCATTERING_ORDER, gpu)?;
        let precompute_time = precompute_start.elapsed();
        println!(
            "AtmosphereBuffer::precompute timing: {}.{}ms",
            precompute_time.as_secs() * 1000 + u64::from(precompute_time.subsec_millis()),
            precompute_time.subsec_micros()
        );

        let camera_and_sun_buffer_size = mem::size_of::<[[f32; 4]; 1]>() as u64;
        let camera_and_sun_buffer = Arc::new(Box::new(gpu.device().create_buffer(
            &wgpu::BufferDescriptor {
                label: Some("atmosphere-sun-buffer"),
                size: camera_and_sun_buffer_size,
                usage: wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST,
            },
        )));

        let bind_group_layout =
            gpu.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("atmosphere-bind-group-layout"),
                    bindings: &[
                        // camera and sun
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStage::FRAGMENT,
                            ty: wgpu::BindingType::UniformBuffer { dynamic: false },
                        },
                        // atmosphere params
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStage::FRAGMENT,
                            ty: wgpu::BindingType::UniformBuffer { dynamic: false },
                        },
                        // transmittance texture
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStage::FRAGMENT,
                            ty: wgpu::BindingType::SampledTexture {
                                multisampled: true,
                                component_type: wgpu::TextureComponentType::Float,
                                dimension: wgpu::TextureViewDimension::D2,
                            },
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 3,
                            visibility: wgpu::ShaderStage::FRAGMENT,
                            ty: wgpu::BindingType::Sampler { comparison: false },
                        },
                        // irradiance texture
                        wgpu::BindGroupLayoutEntry {
                            binding: 4,
                            visibility: wgpu::ShaderStage::FRAGMENT,
                            ty: wgpu::BindingType::SampledTexture {
                                multisampled: true,
                                component_type: wgpu::TextureComponentType::Float,
                                dimension: wgpu::TextureViewDimension::D2,
                            },
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 5,
                            visibility: wgpu::ShaderStage::FRAGMENT,
                            ty: wgpu::BindingType::Sampler { comparison: false },
                        },
                        // scattering texture
                        wgpu::BindGroupLayoutEntry {
                            binding: 6,
                            visibility: wgpu::ShaderStage::FRAGMENT,
                            ty: wgpu::BindingType::SampledTexture {
                                multisampled: true,
                                component_type: wgpu::TextureComponentType::Float,
                                dimension: wgpu::TextureViewDimension::D3,
                            },
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 7,
                            visibility: wgpu::ShaderStage::FRAGMENT,
                            ty: wgpu::BindingType::Sampler { comparison: false },
                        },
                        // single mie scattering texture
                        wgpu::BindGroupLayoutEntry {
                            binding: 8,
                            visibility: wgpu::ShaderStage::FRAGMENT,
                            ty: wgpu::BindingType::SampledTexture {
                                multisampled: true,
                                component_type: wgpu::TextureComponentType::Float,
                                dimension: wgpu::TextureViewDimension::D3,
                            },
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 9,
                            visibility: wgpu::ShaderStage::FRAGMENT,
                            ty: wgpu::BindingType::Sampler { comparison: false },
                        },
                    ],
                });

        let sampler = gpu.device().create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            lod_min_clamp: 0f32,
            lod_max_clamp: 9_999_999f32,
            compare: wgpu::CompareFunction::Never,
        });

        let t2descriptor = wgpu::TextureViewDescriptor {
            format: wgpu::TextureFormat::Rgba32Float,
            dimension: wgpu::TextureViewDimension::D2,
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            level_count: 1, // mip level
            base_array_layer: 0,
            array_layer_count: 1,
        };

        let t3descriptor = wgpu::TextureViewDescriptor {
            format: wgpu::TextureFormat::Rgba32Float,
            dimension: wgpu::TextureViewDimension::D3,
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            level_count: 1, // mip level
            base_array_layer: 0,
            array_layer_count: 1,
        };

        let bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("atmosphere-bind-group"),
            layout: &bind_group_layout,
            bindings: &[
                // camera and sun
                wgpu::Binding {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer {
                        buffer: &camera_and_sun_buffer,
                        range: 0..camera_and_sun_buffer_size,
                    },
                },
                // atmosphere
                wgpu::Binding {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer {
                        buffer: &atmosphere_params_buffer,
                        range: 0..ATMOSPHERE_PARAMETERS_BUFFER_SIZE,
                    },
                },
                // transmittance texture
                wgpu::Binding {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(
                        &transmittance_texture.create_view(&t2descriptor),
                    ),
                },
                wgpu::Binding {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                // irradiance texture
                wgpu::Binding {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(
                        &irradiance_texture.create_view(&t2descriptor),
                    ),
                },
                wgpu::Binding {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                // scattering texture
                wgpu::Binding {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(
                        &scattering_texture.create_view(&t3descriptor),
                    ),
                },
                wgpu::Binding {
                    binding: 7,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                // single mie scattering texture
                wgpu::Binding {
                    binding: 8,
                    resource: wgpu::BindingResource::TextureView(
                        &single_mie_scattering_texture.create_view(&t3descriptor),
                    ),
                },
                wgpu::Binding {
                    binding: 9,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        Ok(Arc::new(RefCell::new(Self {
            bind_group_layout,
            bind_group,
            sun_direction_buffer: camera_and_sun_buffer,
        })))
    }

    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    pub fn make_upload_buffer(
        &self,
        sun_direction: Vector3<f32>,
        gpu: &GPU,
        tracker: &mut FrameStateTracker,
    ) -> Fallible<()> {
        let buffer = [[
            sun_direction.x as f32,
            sun_direction.y as f32,
            sun_direction.z as f32,
            0.0f32,
        ]];
        gpu.upload_slice_to(
            "atmosphere-sun-upload-buffer",
            &buffer,
            self.sun_direction_buffer.clone(),
            wgpu::BufferUsage::MAP_READ | wgpu::BufferUsage::COPY_SRC,
            tracker,
        );
        Ok(())
    }
}
