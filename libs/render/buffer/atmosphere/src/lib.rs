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
use base::{GlobalSets, RayMarchingRenderer};
use failure::Fallible;
use log::trace;
use std::{sync::Arc, time::Instant};
use vulkano::{
    descriptor::descriptor_set::{DescriptorSet, PersistentDescriptorSet},
    pipeline::GraphicsPipelineAbstract,
    sampler::{Filter, MipmapMode, Sampler, SamplerAddressMode},
};
use window::GraphicsWindow;

const NUM_PRECOMPUTED_WAVELENGTHS: usize = 15;
const NUM_SCATTERING_ORDER: usize = 4;

mod fs {
    vulkano_shaders::shader! {
    ty: "fragment",
    include: ["./libs/render/buffer/atmosphere/src"],
    src: "
        #version 450

        #include \"include_atmosphere.glsl\"
        #include \"descriptorset_atmosphere.glsl\"

        void main() {}
        "
    }
}

pub struct AtmosphereRenderer {
    descriptorset: Arc<dyn DescriptorSet + Send + Sync>,
}

impl AtmosphereRenderer {
    pub fn new(
        _raymarching_renderer: &RayMarchingRenderer,
        pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
        window: &GraphicsWindow,
    ) -> Fallible<Self> {
        trace!("AtmosphereRenderer::new");

        let precompute_start = Instant::now();
        let (
            atmosphere_params_buffer,
            transmittance_texture,
            scattering_texture,
            single_mie_scattering_texture,
            irradiance_texture,
        ) = Precompute::new(window)?.run(
            NUM_PRECOMPUTED_WAVELENGTHS,
            NUM_SCATTERING_ORDER,
            window,
        )?;
        let precompute_time = precompute_start.elapsed();
        trace!(
            "AtmosphereRenderer::precompute timing: {}.{}ms",
            precompute_time.as_secs() * 1000 + u64::from(precompute_time.subsec_millis()),
            precompute_time.subsec_micros()
        );

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

        Ok(Self { descriptorset })
    }

    pub fn descriptor_set(&self) -> Arc<dyn DescriptorSet + Send + Sync> {
        self.descriptorset.clone()
    }
}
