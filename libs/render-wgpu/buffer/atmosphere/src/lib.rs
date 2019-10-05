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
use global_layout::GlobalSets;
use log::trace;
use std::time::Instant;

const NUM_PRECOMPUTED_WAVELENGTHS: usize = 40;
const NUM_SCATTERING_ORDER: usize = 4;

pub struct AtmosphereBuffers {
    _bind_group_layout: wgpu::BindGroupLayout,
    _bind_group: wgpu::BindGroup,
}

impl AtmosphereBuffers {
    pub fn new(gpu: &mut gpu::GPU) -> Fallible<Self> {
        trace!("AtmosphereBuffers::new");

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
            "AtmosphereBuffers::precompute timing: {}.{}ms",
            precompute_time.as_secs() * 1000 + u64::from(precompute_time.subsec_millis()),
            precompute_time.subsec_micros()
        );

        let bind_group_layout =
            gpu.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    bindings: &[
                        // atmosphere params
                        wgpu::BindGroupLayoutBinding {
                            binding: 0,
                            visibility: wgpu::ShaderStage::FRAGMENT,
                            ty: wgpu::BindingType::UniformBuffer { dynamic: false },
                        },
                        // transmittance texture
                        wgpu::BindGroupLayoutBinding {
                            binding: 1,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::SampledTexture {
                                multisampled: true,
                                dimension: wgpu::TextureViewDimension::D2,
                            },
                        },
                        wgpu::BindGroupLayoutBinding {
                            binding: 2,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::Sampler,
                        },
                        // irradiance texture
                        wgpu::BindGroupLayoutBinding {
                            binding: 3,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::SampledTexture {
                                multisampled: true,
                                dimension: wgpu::TextureViewDimension::D2,
                            },
                        },
                        wgpu::BindGroupLayoutBinding {
                            binding: 4,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::Sampler,
                        },
                        // scattering texture
                        wgpu::BindGroupLayoutBinding {
                            binding: 5,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::SampledTexture {
                                multisampled: true,
                                dimension: wgpu::TextureViewDimension::D3,
                            },
                        },
                        wgpu::BindGroupLayoutBinding {
                            binding: 6,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::Sampler,
                        },
                        // single mie scattering texture
                        wgpu::BindGroupLayoutBinding {
                            binding: 7,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::SampledTexture {
                                multisampled: true,
                                dimension: wgpu::TextureViewDimension::D3,
                            },
                        },
                        wgpu::BindGroupLayoutBinding {
                            binding: 8,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::Sampler,
                        },
                    ],
                });

        let bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
            bindings: &[
                wgpu::Binding {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer {
                        buffer: &atmosphere_params_buffer,
                        range: 0..ATMOSPHERE_PARAMETERS_BUFFER_SIZE,
                    },
                },
                // transmittance texture
                wgpu::Binding {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(
                        &transmittance_texture.create_view(&wgpu::TextureViewDescriptor {
                            format: wgpu::TextureFormat::Rgba32Float,
                            dimension: wgpu::TextureViewDimension::D2,
                            aspect: wgpu::TextureAspect::All,
                            base_mip_level: 0,
                            level_count: 1, // mip level
                            base_array_layer: 0,
                            array_layer_count: 1,
                        }),
                    ),
                },
                wgpu::Binding {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&gpu.device().create_sampler(
                        &wgpu::SamplerDescriptor {
                            address_mode_u: wgpu::AddressMode::ClampToEdge,
                            address_mode_v: wgpu::AddressMode::ClampToEdge,
                            address_mode_w: wgpu::AddressMode::ClampToEdge,
                            mag_filter: wgpu::FilterMode::Linear,
                            min_filter: wgpu::FilterMode::Linear,
                            mipmap_filter: wgpu::FilterMode::Linear,
                            lod_min_clamp: 0f32,
                            lod_max_clamp: 9_999_999f32,
                            compare_function: wgpu::CompareFunction::Never,
                        },
                    )),
                },
                // irradiance texture
                wgpu::Binding {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&irradiance_texture.create_view(
                        &wgpu::TextureViewDescriptor {
                            format: wgpu::TextureFormat::Rgba32Float,
                            dimension: wgpu::TextureViewDimension::D2,
                            aspect: wgpu::TextureAspect::All,
                            base_mip_level: 0,
                            level_count: 1, // mip level
                            base_array_layer: 0,
                            array_layer_count: 1,
                        },
                    )),
                },
                wgpu::Binding {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&gpu.device().create_sampler(
                        &wgpu::SamplerDescriptor {
                            address_mode_u: wgpu::AddressMode::ClampToEdge,
                            address_mode_v: wgpu::AddressMode::ClampToEdge,
                            address_mode_w: wgpu::AddressMode::ClampToEdge,
                            mag_filter: wgpu::FilterMode::Linear,
                            min_filter: wgpu::FilterMode::Linear,
                            mipmap_filter: wgpu::FilterMode::Linear,
                            lod_min_clamp: 0f32,
                            lod_max_clamp: 9_999_999f32,
                            compare_function: wgpu::CompareFunction::Never,
                        },
                    )),
                },
                // scattering texture
                wgpu::Binding {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&scattering_texture.create_view(
                        &wgpu::TextureViewDescriptor {
                            format: wgpu::TextureFormat::Rgba32Float,
                            dimension: wgpu::TextureViewDimension::D3,
                            aspect: wgpu::TextureAspect::All,
                            base_mip_level: 0,
                            level_count: 1, // mip level
                            base_array_layer: 0,
                            array_layer_count: 1,
                        },
                    )),
                },
                wgpu::Binding {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&gpu.device().create_sampler(
                        &wgpu::SamplerDescriptor {
                            address_mode_u: wgpu::AddressMode::ClampToEdge,
                            address_mode_v: wgpu::AddressMode::ClampToEdge,
                            address_mode_w: wgpu::AddressMode::ClampToEdge,
                            mag_filter: wgpu::FilterMode::Linear,
                            min_filter: wgpu::FilterMode::Linear,
                            mipmap_filter: wgpu::FilterMode::Linear,
                            lod_min_clamp: 0f32,
                            lod_max_clamp: 9_999_999f32,
                            compare_function: wgpu::CompareFunction::Never,
                        },
                    )),
                },
                // single mie scattering texture
                wgpu::Binding {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(
                        &single_mie_scattering_texture.create_view(&wgpu::TextureViewDescriptor {
                            format: wgpu::TextureFormat::Rgba32Float,
                            dimension: wgpu::TextureViewDimension::D3,
                            aspect: wgpu::TextureAspect::All,
                            base_mip_level: 0,
                            level_count: 1, // mip level
                            base_array_layer: 0,
                            array_layer_count: 1,
                        }),
                    ),
                },
                wgpu::Binding {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&gpu.device().create_sampler(
                        &wgpu::SamplerDescriptor {
                            address_mode_u: wgpu::AddressMode::ClampToEdge,
                            address_mode_v: wgpu::AddressMode::ClampToEdge,
                            address_mode_w: wgpu::AddressMode::ClampToEdge,
                            mag_filter: wgpu::FilterMode::Linear,
                            min_filter: wgpu::FilterMode::Linear,
                            mipmap_filter: wgpu::FilterMode::Linear,
                            lod_min_clamp: 0f32,
                            lod_max_clamp: 9_999_999f32,
                            compare_function: wgpu::CompareFunction::Never,
                        },
                    )),
                },
            ],
        });

        Ok(Self {
            _bind_group_layout: bind_group_layout,
            _bind_group: bind_group,
        })
    }
}
