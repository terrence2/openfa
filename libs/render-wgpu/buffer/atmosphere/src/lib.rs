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
// to Rust/Vulkan and are not reflective of the high quality of the
// original work in any way.

mod colorspace;
mod earth_consts;
mod precompute;

use crate::precompute::Precompute;
use failure::Fallible;
use global_layout::GlobalSets;
use log::trace;
use std::{sync::Arc, time::Instant};

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
            //            transmittance_texture,
            //            scattering_texture,
            //            single_mie_scattering_texture,
            //            irradiance_texture,
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
                            ty: wgpu::BindingType::StorageBuffer {
                                dynamic: false,
                                readonly: true,
                            },
                        },
                        // transmittance texture
                        wgpu::BindGroupLayoutBinding {
                            binding: 1,
                            visibility: wgpu::ShaderStage::FRAGMENT,
                            ty: wgpu::BindingType::StorageBuffer {
                                dynamic: false,
                                readonly: true,
                            },
                        },
                        wgpu::BindGroupLayoutBinding {
                            binding: 2,
                            visibility: wgpu::ShaderStage::FRAGMENT,
                            ty: wgpu::BindingType::StorageBuffer {
                                dynamic: false,
                                readonly: true,
                            },
                        },
                        wgpu::BindGroupLayoutBinding {
                            binding: 3,
                            visibility: wgpu::ShaderStage::FRAGMENT,
                            ty: wgpu::BindingType::StorageBuffer {
                                dynamic: false,
                                readonly: true,
                            },
                        },
                    ],
                });

        let bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
            bindings: &[
//                wgpu::Binding {
//                    binding: 0,
//                    resource: wgpu::BindingResource::Buffer {
//                        buffer: &band_buffer,
//                        range: 0..band_buffer_size,
//                    },
//                },
//                wgpu::Binding {
//                    binding: 1,
//                    resource: wgpu::BindingResource::Buffer {
//                        buffer: &bin_positions_buffer,
//                        range: 0..bin_positions_buffer_size,
//                    },
//                },
//                wgpu::Binding {
//                    binding: 2,
//                    resource: wgpu::BindingResource::Buffer {
//                        buffer: &star_indices_buffer,
//                        range: 0..star_indices_buffer_size,
//                    },
//                },
//                wgpu::Binding {
//                    binding: 3,
//                    resource: wgpu::BindingResource::Buffer {
//                        buffer: &star_buffer,
//                        range: 0..star_buffer_size,
//                    },
//                },
            ],
        });

        /*
        let sampler = Sampler::new(
            window.device(),
            Filter::Linear,
            Filter::Linear,
            MipmapMode::Nearest,
            SamplerAddressMode::ClampToEdge,
            SamplerAddressMode::ClampToEdge,
            SamplerAddressMode::ClampToEdge,
            0.0,
            1.0,
            0.0,
            0.0,
        )?;
        let descriptorset: Arc<dyn DescriptorSet + Send + Sync> = Arc::new(
            PersistentDescriptorSet::start(pipeline.clone(), GlobalSets::Atmosphere.into())
                .add_buffer(atmosphere_params_buffer.clone())?
                .add_sampled_image(transmittance_texture.clone(), sampler.clone())?
                .add_sampled_image(scattering_texture.clone(), sampler.clone())?
                .add_sampled_image(single_mie_scattering_texture.clone(), sampler.clone())?
                .add_sampled_image(irradiance_texture.clone(), sampler.clone())?
                .build()?,
        );
        */

        Ok(Self {
            _bind_group_layout: bind_group_layout,
            _bind_group: bind_group,
        })
    }
}
