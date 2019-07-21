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
use crate::earth_consts::RGB_LAMBDAS;
use crate::{
    colorspace::{wavelength_to_srgb, MAX_LAMBDA, MIN_LAMBDA},
    earth_consts::EarthParameters,
    fs,
};
use failure::{bail, Fallible};
use image::{ImageBuffer, Rgb};
use nalgebra::Matrix3;
use std::sync::Arc;
use vulkano::{
    buffer::{BufferUsage, CpuAccessibleBuffer},
    command_buffer::{AutoCommandBufferBuilder, CommandBuffer},
    descriptor::descriptor_set::{DescriptorSet, PersistentDescriptorSet},
    device::Device,
    format::Format,
    image::{Dimensions, ImageLayout, ImageUsage, ImmutableImage, MipmapsCount, StorageImage},
    impl_vertex,
    pipeline::{
        ComputePipeline, ComputePipelineAbstract, GraphicsPipeline, GraphicsPipelineAbstract,
    },
    sampler::{Filter, MipmapMode, Sampler, SamplerAddressMode},
    sync::GpuFuture,
};
use window::GraphicsWindow;

// Checklist:
// Final
//     final transmittance:   correct
//     final irradiance:      wrong
//     final scattering:      correct
//     final mie scatter:     correct
// Single Round
//     delta transmittance:   correct
//     direct irradiance:     correct
//     single delta rayleigh: correct
//     single delta mie:      correct
//     single rayleigh acc:   correct
//     single mie acc:        correct
// Multi Round
//     density
//     delta irradiance:
//     irradiance acc:        wrong

const DUMP_TRANSMITTANCE: bool = false;
const DUMP_DIRECT_IRRADIANCE: bool = false;
const DUMP_SINGLE_RAYLEIGH: bool = false;
const DUMP_SINGLE_MIE: bool = false;
const DUMP_SINGLE_ACC: bool = false;
const DUMP_SINGLE_MIE_ACC: bool = false;
const DUMP_SCATTERING_DENSITY: bool = false;
const DUMP_INDIRECT_IRRADIANCE_DELTA: bool = true;
const DUMP_INDIRECT_IRRADIANCE_ACC: bool = true;
const DUMP_MULTIPLE_SCATTERING: bool = false;

const TRANSMITTANCE_TEXTURE_WIDTH: u32 = 256;
const TRANSMITTANCE_TEXTURE_HEIGHT: u32 = 64;

const SCATTERING_TEXTURE_R_SIZE: u32 = 32;
const SCATTERING_TEXTURE_MU_SIZE: u32 = 128;
const SCATTERING_TEXTURE_MU_S_SIZE: u32 = 32;
const SCATTERING_TEXTURE_NU_SIZE: u32 = 8;

const SCATTERING_TEXTURE_WIDTH: u32 = SCATTERING_TEXTURE_NU_SIZE * SCATTERING_TEXTURE_MU_S_SIZE;
const SCATTERING_TEXTURE_HEIGHT: u32 = SCATTERING_TEXTURE_MU_SIZE;
const SCATTERING_TEXTURE_DEPTH: u32 = SCATTERING_TEXTURE_R_SIZE;

const IRRADIANCE_TEXTURE_WIDTH: u32 = 64;
const IRRADIANCE_TEXTURE_HEIGHT: u32 = 16;

// Temp storage for stuff as we pre-compute the textures we need for fast rendering.
pub struct Precompute {
    transmittance_dimensions: Dimensions,
    irradiance_dimensions: Dimensions,
    scattering_dimensions: Dimensions,

    // Shaders.
    compute_transmittance: Arc<dyn ComputePipelineAbstract + Send + Sync>,
    compute_direct_irradiance: Arc<dyn ComputePipelineAbstract + Send + Sync>,
    compute_single_scattering: Arc<dyn ComputePipelineAbstract + Send + Sync>,
    compute_scattering_density: Arc<dyn ComputePipelineAbstract + Send + Sync>,
    compute_indirect_irradiance: Arc<dyn ComputePipelineAbstract + Send + Sync>,
    compute_multiple_scattering: Arc<dyn ComputePipelineAbstract + Send + Sync>,

    // Temporary textures.
    delta_irradiance_texture: Arc<StorageImage<Format>>,
    delta_rayleigh_scattering_texture: Arc<StorageImage<Format>>,
    delta_mie_scattering_texture: Arc<StorageImage<Format>>,
    delta_multiple_scattering_texture: Arc<StorageImage<Format>>,
    delta_scattering_density_texture: Arc<StorageImage<Format>>,

    // Permanent/accumulator textures.
    transmittance_texture: Arc<StorageImage<Format>>,
    scattering_texture: Arc<StorageImage<Format>>,
    single_mie_scattering_texture: Arc<StorageImage<Format>>,
    irradiance_texture: Arc<StorageImage<Format>>,

    params: EarthParameters,
}

mod compute_transmittance_shader {
    vulkano_shaders::shader! {
    ty: "compute",
    include: ["./libs/renderer/sky/src"],
    src: "
        #version 450
        #include \"lut_builder.glsl\"

        layout(local_size_x = 8, local_size_y = 8, local_size_z = 1) in;
        layout(binding = 0) uniform Data { AtmosphereParameters atmosphere; } data;
        layout(binding = 1, rgba32f) uniform writeonly image2D transmittance_texture;

        void main() {
            compute_transmittance_program(
                gl_GlobalInvocationID.xy + vec2(0.5, 0.5),
                data.atmosphere,
                transmittance_texture
            );
        }"
    }
}

impl Precompute {
    fn compute_transmittance_at(
        &self,
        lambdas: [f64; 3],
        window: &GraphicsWindow,
        atmosphere_params_buffer: Arc<CpuAccessibleBuffer<fs::ty::AtmosphereParameters>>,
    ) -> Fallible<()> {
        let pds = Arc::new(
            PersistentDescriptorSet::start(self.compute_transmittance.clone(), 0)
                .add_buffer(atmosphere_params_buffer)?
                .add_image(self.transmittance_texture.clone())?
                .build()?,
        );

        let command_buffer =
            AutoCommandBufferBuilder::new(window.device(), window.queue().family())?
                .dispatch(
                    [
                        self.transmittance_dimensions.width() / 8,
                        self.transmittance_dimensions.height() / 8,
                        1,
                    ],
                    self.compute_transmittance.clone(),
                    pds.clone(),
                    (),
                )?
                .build()?;

        let finished = command_buffer.execute(window.queue())?;
        finished.then_signal_fence_and_flush()?.wait(None)?;

        if DUMP_TRANSMITTANCE {
            let path = format!(
                "dump/sky/transmittance-{}-{}-{}.png",
                lambdas[0] as usize, lambdas[1] as usize, lambdas[2] as usize
            );
            Self::dump_2d(&path, self.transmittance_texture.clone(), window)?;
        }

        Ok(())
    }
}

mod compute_direct_irradiance_shader {
    vulkano_shaders::shader! {
    ty: "compute",
    include: ["./libs/renderer/sky/src"],
    src: "
        #version 450
        #include \"lut_builder.glsl\"

        layout(local_size_x = 8, local_size_y = 8, local_size_z = 1) in;
        layout(binding = 0) uniform Data { AtmosphereParameters atmosphere; } data;
        layout(binding = 1) uniform sampler2D transmittance_texture;
        layout(binding = 2, rgba32f) uniform writeonly image2D delta_irradiance_texture;

        void main() {
            compute_direct_irradiance_program(
                gl_GlobalInvocationID.xy + vec2(0.5, 0.5),
                data.atmosphere,
                transmittance_texture,
                delta_irradiance_texture
            );
        }"
    }
}

impl Precompute {
    fn compute_direct_irradiance_at(
        &self,
        lambdas: [f64; 3],
        window: &GraphicsWindow,
        atmosphere_params_buffer: Arc<CpuAccessibleBuffer<fs::ty::AtmosphereParameters>>,
    ) -> Fallible<()> {
        let pds = Arc::new(
            PersistentDescriptorSet::start(self.compute_direct_irradiance.clone(), 0)
                .add_buffer(atmosphere_params_buffer)?
                .add_sampled_image(
                    self.transmittance_texture.clone(),
                    Self::make_sampler(window.device())?,
                )?
                .add_image(self.delta_irradiance_texture.clone())?
                .build()?,
        );

        let command_buffer =
            AutoCommandBufferBuilder::new(window.device(), window.queue().family())?
                .dispatch(
                    [
                        self.irradiance_dimensions.width() / 8,
                        self.irradiance_dimensions.height() / 8,
                        1,
                    ],
                    self.compute_direct_irradiance.clone(),
                    pds.clone(),
                    (),
                )?
                .build()?;

        let finished = command_buffer.execute(window.queue())?;
        finished.then_signal_fence_and_flush()?.wait(None)?;

        if DUMP_DIRECT_IRRADIANCE {
            let path = format!(
                "dump/sky/direct-irradiance-{}-{}-{}.png",
                lambdas[0] as usize, lambdas[1] as usize, lambdas[2] as usize
            );
            Self::dump_2d(&path, self.delta_irradiance_texture.clone(), window)?;
        }

        Ok(())
    }
}

mod compute_single_scattering_shader {
    vulkano_shaders::shader! {
    ty: "compute",
    include: ["./libs/renderer/sky/src"],
    src: "
        #version 450
        #include \"lut_builder.glsl\"

        layout(local_size_x = 8, local_size_y = 8, local_size_z = 8) in;
        layout(binding = 0) uniform Data1 { AtmosphereParameters atmosphere; } data1;
        layout(binding = 1) uniform Data2 { mat4 rad_to_lum; } data2;
        layout(binding = 2) uniform sampler2D transmittance_texture;
        layout(binding = 3, rgba8) uniform restrict writeonly image3D delta_rayleigh_scattering_texture;
        layout(binding = 4, rgba8) uniform restrict writeonly image3D delta_mie_scattering_texture;
        layout(binding = 5, rgba8) uniform coherent image3D scattering_texture;
        layout(binding = 6, rgba8) uniform coherent image3D single_mie_scattering_texture;

        void main() {
            vec3 scattering = vec3(0);
            vec3 single_mie_scattering = vec3(0);
            compute_single_scattering_program(
                gl_GlobalInvocationID.xyz + vec3(0.5),
                mat3(data2.rad_to_lum),
                data1.atmosphere,
                transmittance_texture,
                delta_rayleigh_scattering_texture,
                delta_mie_scattering_texture,
                scattering,
                single_mie_scattering
            );

            ivec3 coord = ivec3(gl_GlobalInvocationID.xyz);

            vec3 prior_scattering = imageLoad(scattering_texture, coord).rgb;
            imageStore(
                scattering_texture,
                coord,
                vec4(prior_scattering + scattering, 1.0)
            );

            vec3 prior_single_mie_scattering = imageLoad(single_mie_scattering_texture, coord).rgb;
            imageStore(
                single_mie_scattering_texture,
                coord,
                vec4(prior_single_mie_scattering + single_mie_scattering, 1.0)
            );
        }
        "
    }
}

impl Precompute {
    fn compute_single_scattering_at(
        &self,
        lambdas: [f64; 3],
        window: &GraphicsWindow,
        atmosphere_params_buffer: Arc<CpuAccessibleBuffer<fs::ty::AtmosphereParameters>>,
        rad_to_lum_buffer: Arc<CpuAccessibleBuffer<[f32; 16]>>,
    ) -> Fallible<()> {
        let pds = Arc::new(
            PersistentDescriptorSet::start(self.compute_single_scattering.clone(), 0)
                .add_buffer(atmosphere_params_buffer)?
                .add_buffer(rad_to_lum_buffer)?
                .add_sampled_image(
                    self.transmittance_texture.clone(),
                    Self::make_sampler(window.device())?,
                )?
                .add_image(self.delta_rayleigh_scattering_texture.clone())?
                .add_image(self.delta_mie_scattering_texture.clone())?
                .add_image(self.scattering_texture.clone())?
                .add_image(self.single_mie_scattering_texture.clone())?
                .build()?,
        );

        let command_buffer =
            AutoCommandBufferBuilder::new(window.device(), window.queue().family())?
                .dispatch(
                    [
                        self.scattering_dimensions.width() / 8,
                        self.scattering_dimensions.height() / 8,
                        self.scattering_dimensions.depth() / 8,
                    ],
                    self.compute_single_scattering.clone(),
                    pds,
                    (),
                )?
                .build()?;

        let finished = command_buffer.execute(window.queue())?;
        finished.then_signal_fence_and_flush()?.wait(None)?;

        if DUMP_SINGLE_RAYLEIGH {
            let path = format!(
                "dump/sky/single-scattering-delta-rayleigh-{}-{}-{}",
                lambdas[0] as usize, lambdas[1] as usize, lambdas[2] as usize
            );
            Self::dump_3d(
                &path,
                self.delta_rayleigh_scattering_texture.clone(),
                window,
            )?;
        }
        if DUMP_SINGLE_ACC {
            let path = format!(
                "dump/sky/single-scattering-acc-{}-{}-{}",
                lambdas[0] as usize, lambdas[1] as usize, lambdas[2] as usize
            );
            Self::dump_3d(&path, self.scattering_texture.clone(), window)?;
        }
        if DUMP_SINGLE_MIE {
            let path = format!(
                "dump/sky/single-scattering-delta-mie-{}-{}-{}",
                lambdas[0] as usize, lambdas[1] as usize, lambdas[2] as usize
            );
            Self::dump_3d(&path, self.delta_mie_scattering_texture.clone(), window)?;
        }
        if DUMP_SINGLE_MIE_ACC {
            let path = format!(
                "dump/sky/single-scattering-mie-acc-{}-{}-{}",
                lambdas[0] as usize, lambdas[1] as usize, lambdas[2] as usize
            );
            Self::dump_3d(&path, self.single_mie_scattering_texture.clone(), window)?;
        }

        Ok(())
    }
}

mod compute_scattering_density_shader {
    vulkano_shaders::shader! {
    ty: "compute",
    include: ["./libs/renderer/sky/src"],
    src: "
        #version 450
        #include \"lut_builder.glsl\"

        layout(local_size_x = 8, local_size_y = 8, local_size_z = 8) in;
        layout(binding = 0) uniform Data1 { AtmosphereParameters atmosphere; } data1;
        layout(binding = 1) uniform Data2 { mat4 rad_to_lum; } data2;
        layout(binding = 2) uniform Data3 { uint scattering_order; } data3;
        layout(binding = 3) uniform sampler2D transmittance_texture;
        layout(binding = 4) uniform sampler3D delta_rayleigh_scattering_texture;
        layout(binding = 5) uniform sampler3D delta_mie_scattering_texture;
        layout(binding = 6) uniform sampler3D delta_multiple_scattering_texture;
        layout(binding = 7) uniform sampler2D delta_irradiance_texture;
        layout(binding = 8, rgba8) uniform writeonly image3D delta_scattering_density_texture;

        void main() {
            compute_scattering_density_program(
                gl_GlobalInvocationID.xyz + vec3(0.5, 0.5, 0.5),
                data1.atmosphere,
                mat3(data2.rad_to_lum),
                data3.scattering_order,
                transmittance_texture,
                delta_rayleigh_scattering_texture,
                delta_mie_scattering_texture,
                delta_multiple_scattering_texture,
                delta_irradiance_texture,
                delta_scattering_density_texture
            );
        }
        "
    }
}

impl Precompute {
    fn compute_scattering_density_at(
        &self,
        lambdas: [f64; 3],
        scattering_order: usize,
        window: &GraphicsWindow,
        atmosphere_params_buffer: Arc<CpuAccessibleBuffer<fs::ty::AtmosphereParameters>>,
        rad_to_lum_buffer: Arc<CpuAccessibleBuffer<[f32; 16]>>,
        scattering_order_buffer: Arc<CpuAccessibleBuffer<u32>>,
    ) -> Fallible<()> {
        let pds = Arc::new(
            PersistentDescriptorSet::start(self.compute_scattering_density.clone(), 0)
                .add_buffer(atmosphere_params_buffer)?
                .add_buffer(rad_to_lum_buffer)?
                .add_buffer(scattering_order_buffer)?
                .add_sampled_image(
                    self.transmittance_texture.clone(),
                    Self::make_sampler(window.device())?,
                )?
                .add_sampled_image(
                    self.delta_rayleigh_scattering_texture.clone(),
                    Self::make_sampler(window.device())?,
                )?
                .add_sampled_image(
                    self.delta_mie_scattering_texture.clone(),
                    Self::make_sampler(window.device())?,
                )?
                .add_sampled_image(
                    self.delta_multiple_scattering_texture.clone(),
                    Self::make_sampler(window.device())?,
                )?
                .add_sampled_image(
                    self.delta_irradiance_texture.clone(),
                    Self::make_sampler(window.device())?,
                )?
                .add_image(self.delta_scattering_density_texture.clone())?
                .build()?,
        );

        let command_buffer =
            AutoCommandBufferBuilder::new(window.device(), window.queue().family())?
                .dispatch(
                    [
                        self.scattering_dimensions.width() / 8,
                        self.scattering_dimensions.height() / 8,
                        self.scattering_dimensions.depth() / 8,
                    ],
                    self.compute_scattering_density.clone(),
                    pds,
                    (),
                )?
                .build()?;

        let finished = command_buffer.execute(window.queue())?;
        finished.then_signal_fence_and_flush()?.wait(None)?;

        if DUMP_SCATTERING_DENSITY {
            let path = format!(
                "dump/sky/delta-scattering-density-@{}-{}-{}-{}",
                scattering_order, lambdas[0] as usize, lambdas[1] as usize, lambdas[2] as usize
            );
            Self::dump_3d(&path, self.delta_scattering_density_texture.clone(), window)?;
        }

        Ok(())
    }
}

mod compute_indirect_irradiance_shader {
    vulkano_shaders::shader! {
    ty: "compute",
    include: ["./libs/renderer/sky/src"],
    src: "
        #version 450
        #include \"lut_builder.glsl\"

        layout(local_size_x = 8, local_size_y = 8, local_size_z = 1) in;
        layout(binding = 0) uniform Data1 { AtmosphereParameters atmosphere; } data1;
        layout(binding = 1) uniform Data2 { mat4 rad_to_lum; } data2;
        layout(binding = 2) uniform Data3 { uint scattering_order; } data3;
        layout(binding = 3) uniform sampler3D delta_rayleigh_scattering_texture;
        layout(binding = 4) uniform sampler3D delta_mie_scattering_texture;
        layout(binding = 5) uniform sampler3D delta_multiple_scattering_texture;
        layout(binding = 6, rgba32f) uniform writeonly image2D delta_irradiance_texture;
        layout(binding = 7, rgba32f) uniform image2D irradiance_texture;

        void main() {
            vec3 indirect_irradiance;
            compute_indirect_irradiance_program(
                gl_GlobalInvocationID.xy + vec2(0.5, 0.5),
                data3.scattering_order,
                data1.atmosphere,
                delta_rayleigh_scattering_texture,
                delta_mie_scattering_texture,
                delta_multiple_scattering_texture,
                delta_irradiance_texture,
                indirect_irradiance
            );

            vec3 prior_irradiance = imageLoad(
                irradiance_texture,
                ivec2(gl_GlobalInvocationID.xy)
            ).rgb;
            imageStore(
                irradiance_texture,
                ivec2(gl_GlobalInvocationID.xy),
                vec4(prior_irradiance + (mat3(data2.rad_to_lum) * indirect_irradiance), 1.0)
            );
        }
        "
    }
}

impl Precompute {
    fn compute_indirect_irradiance_at(
        &self,
        lambdas: [f64; 3],
        scattering_order: usize,
        window: &GraphicsWindow,
        atmosphere_params_buffer: Arc<CpuAccessibleBuffer<fs::ty::AtmosphereParameters>>,
        rad_to_lum_buffer: Arc<CpuAccessibleBuffer<[f32; 16]>>,
    ) -> Fallible<()> {
        // This actually needs to be one lower in get_best_scattering so that
        // it will not use the pre-computed on the first pass.
        let scattering_order_less_one_buffer = CpuAccessibleBuffer::from_data(
            window.device(),
            BufferUsage::all(),
            (scattering_order - 1) as u32,
        )?;
        let pds = Arc::new(
            PersistentDescriptorSet::start(self.compute_indirect_irradiance.clone(), 0)
                .add_buffer(atmosphere_params_buffer)?
                .add_buffer(rad_to_lum_buffer)?
                .add_buffer(scattering_order_less_one_buffer)?
                .add_sampled_image(
                    self.delta_rayleigh_scattering_texture.clone(),
                    Self::make_sampler(window.device())?,
                )?
                .add_sampled_image(
                    self.delta_mie_scattering_texture.clone(),
                    Self::make_sampler(window.device())?,
                )?
                .add_sampled_image(
                    self.delta_multiple_scattering_texture.clone(),
                    Self::make_sampler(window.device())?,
                )?
                .add_image(self.delta_irradiance_texture.clone())?
                .add_image(self.irradiance_texture.clone())?
                .build()?,
        );

        let command_buffer =
            AutoCommandBufferBuilder::new(window.device(), window.queue().family())?
                .dispatch(
                    [
                        self.irradiance_dimensions.width() / 8,
                        self.irradiance_dimensions.height() / 8,
                        1,
                    ],
                    self.compute_indirect_irradiance.clone(),
                    pds,
                    (),
                )?
                .build()?;

        let finished = command_buffer.execute(window.queue())?;
        finished.then_signal_fence_and_flush()?.wait(None)?;

        if DUMP_INDIRECT_IRRADIANCE_DELTA {
            let path = format!(
                "dump/sky/indirect-delta-irradiance-@{}-{}-{}-{}.png",
                scattering_order, lambdas[0] as usize, lambdas[1] as usize, lambdas[2] as usize
            );
            Self::dump_2d(&path, self.delta_irradiance_texture.clone(), window)?;
        }
        if DUMP_INDIRECT_IRRADIANCE_ACC {
            let path = format!(
                "dump/sky/indirect-irradiance-acc-@{}-{}-{}-{}.png",
                scattering_order, lambdas[0] as usize, lambdas[1] as usize, lambdas[2] as usize
            );
            Self::dump_2d(&path, self.irradiance_texture.clone(), window)?;
        }

        Ok(())
    }
}

mod compute_multiple_scattering_shader {
    vulkano_shaders::shader! {
    ty: "compute",
    include: ["./libs/renderer/sky/src"],
    src: "
        #version 450
        #include \"lut_builder.glsl\"

        layout(local_size_x = 8, local_size_y = 8, local_size_z = 8) in;
        layout(binding = 0) uniform Data1 { AtmosphereParameters atmosphere; } data1;
        layout(binding = 1) uniform Data2 { mat4 rad_to_lum; } data2;
        layout(binding = 2) uniform Data3 { uint scattering_order; } data3;
        layout(binding = 3) uniform sampler2D transmittance_texture;
        layout(binding = 4) uniform sampler3D delta_scattering_density_texture; // density_lambda;
        layout(binding = 5, rgba8) uniform writeonly image3D delta_multiple_scattering_texture; // scattering_lambda;
        layout(binding = 6, rgba8) uniform image3D scattering_texture;

        void main() {
            ScatterCoord sc;
            vec3 delta_multiple_scattering;
            compute_multiple_scattering_program(
                gl_GlobalInvocationID.xyz + vec3(0.5, 0.5, 0.5),
                data1.atmosphere,
                mat3(data2.rad_to_lum),
                data3.scattering_order,
                transmittance_texture,
                delta_scattering_density_texture,
                delta_multiple_scattering_texture,
                sc,
                delta_multiple_scattering
            );

            vec4 scattering = vec4(
                  mat3(data2.rad_to_lum) * delta_multiple_scattering.rgb / rayleigh_phase_function(sc.nu),
                  0.0);
            vec4 prior_scattering = imageLoad(
                scattering_texture,
                ivec3(gl_GlobalInvocationID.xyz)
            );
            imageStore(
                scattering_texture,
                ivec3(gl_GlobalInvocationID.xyz),
                prior_scattering + scattering
            );
        }
        "
    }
}

impl Precompute {
    fn compute_multiple_scattering_at(
        &self,
        lambdas: [f64; 3],
        scattering_order: usize,
        window: &GraphicsWindow,
        atmosphere_params_buffer: Arc<CpuAccessibleBuffer<fs::ty::AtmosphereParameters>>,
        rad_to_lum_buffer: Arc<CpuAccessibleBuffer<[f32; 16]>>,
        scattering_order_buffer: Arc<CpuAccessibleBuffer<u32>>,
    ) -> Fallible<()> {
        let pds = Arc::new(
            PersistentDescriptorSet::start(self.compute_multiple_scattering.clone(), 0)
                .add_buffer(atmosphere_params_buffer)?
                .add_buffer(rad_to_lum_buffer)?
                .add_buffer(scattering_order_buffer)?
                .add_sampled_image(
                    self.transmittance_texture.clone(),
                    Self::make_sampler(window.device())?,
                )?
                .add_sampled_image(
                    self.delta_scattering_density_texture.clone(),
                    Self::make_sampler(window.device())?,
                )?
                .add_image(self.delta_multiple_scattering_texture.clone())?
                .add_image(self.scattering_texture.clone())?
                .build()?,
        );

        let command_buffer =
            AutoCommandBufferBuilder::new(window.device(), window.queue().family())?
                .dispatch(
                    [
                        self.scattering_dimensions.width() / 8,
                        self.scattering_dimensions.height() / 8,
                        self.scattering_dimensions.depth() / 8,
                    ],
                    self.compute_multiple_scattering.clone(),
                    pds,
                    (),
                )?
                .build()?;

        let finished = command_buffer.execute(window.queue())?;
        finished.then_signal_fence_and_flush()?.wait(None)?;

        if DUMP_MULTIPLE_SCATTERING {
            let path = format!(
                "dump/sky/delta-multiple-scattering-@{}-{}-{}-{}",
                scattering_order, lambdas[0] as usize, lambdas[1] as usize, lambdas[2] as usize
            );
            Self::dump_3d(
                &path,
                self.delta_multiple_scattering_texture.clone(),
                window,
            )?;
            let path = format!(
                "dump/sky/multiple-scattering-@{}-{}-{}-{}",
                scattering_order, lambdas[0] as usize, lambdas[1] as usize, lambdas[2] as usize
            );
            Self::dump_3d(&path, self.scattering_texture.clone(), window)?;
        }

        Ok(())
    }
}

impl Precompute {
    pub fn new(window: &GraphicsWindow) -> Fallible<Self> {
        let params = EarthParameters::new();

        let transmittance_dimensions = Dimensions::Dim2d {
            width: TRANSMITTANCE_TEXTURE_WIDTH,
            height: TRANSMITTANCE_TEXTURE_HEIGHT,
        };
        let irradiance_dimensions = Dimensions::Dim2d {
            width: IRRADIANCE_TEXTURE_WIDTH,
            height: IRRADIANCE_TEXTURE_HEIGHT,
        };
        let scattering_dimensions = Dimensions::Dim3d {
            width: SCATTERING_TEXTURE_WIDTH,
            height: SCATTERING_TEXTURE_HEIGHT,
            depth: SCATTERING_TEXTURE_DEPTH,
        };

        // Load all shaders.
        let compute_transmittance_shader =
            compute_transmittance_shader::Shader::load(window.device())?;
        let compute_direct_irradiance_shader =
            compute_direct_irradiance_shader::Shader::load(window.device())?;
        let compute_single_scattering_shader =
            compute_single_scattering_shader::Shader::load(window.device())?;
        let compute_scattering_density_shader =
            compute_scattering_density_shader::Shader::load(window.device())?;
        let compute_indirect_irradiance_shader =
            compute_indirect_irradiance_shader::Shader::load(window.device())?;
        let compute_multiple_scattering_shader =
            compute_multiple_scattering_shader::Shader::load(window.device())?;

        // Build compute pipelines for all of our shaders.
        let compute_transmittance = Arc::new(ComputePipeline::new(
            window.device(),
            &compute_transmittance_shader.main_entry_point(),
            &(),
        )?);
        let compute_direct_irradiance = Arc::new(ComputePipeline::new(
            window.device(),
            &compute_direct_irradiance_shader.main_entry_point(),
            &(),
        )?);
        let compute_single_scattering = Arc::new(ComputePipeline::new(
            window.device(),
            &compute_single_scattering_shader.main_entry_point(),
            &(),
        )?);
        let compute_scattering_density = Arc::new(ComputePipeline::new(
            window.device(),
            &compute_scattering_density_shader.main_entry_point(),
            &(),
        )?);
        let compute_indirect_irradiance = Arc::new(ComputePipeline::new(
            window.device(),
            &compute_indirect_irradiance_shader.main_entry_point(),
            &(),
        )?);
        let compute_multiple_scattering = Arc::new(ComputePipeline::new(
            window.device(),
            &compute_multiple_scattering_shader.main_entry_point(),
            &(),
        )?);

        // Allocate all of our memory up front.
        let delta_irradiance_texture = StorageImage::new(
            window.device(),
            irradiance_dimensions,
            Format::R32G32B32A32Sfloat,
            Some(window.queue().family()),
        )?;
        let delta_rayleigh_scattering_texture = StorageImage::new(
            window.device(),
            scattering_dimensions,
            Format::R32G32B32A32Sfloat,
            Some(window.queue().family()),
        )?;
        let delta_mie_scattering_texture = StorageImage::new(
            window.device(),
            scattering_dimensions,
            Format::R32G32B32A32Sfloat,
            Some(window.queue().family()),
        )?;
        let delta_multiple_scattering_texture = StorageImage::new(
            window.device(),
            scattering_dimensions,
            Format::R32G32B32A32Sfloat,
            Some(window.queue().family()),
        )?;
        let delta_scattering_density_texture = StorageImage::new(
            window.device(),
            scattering_dimensions,
            Format::R32G32B32A32Sfloat,
            Some(window.queue().family()),
        )?;

        let transmittance_texture = StorageImage::new(
            window.device(),
            transmittance_dimensions,
            Format::R32G32B32A32Sfloat,
            Some(window.queue().family()),
        )?;
        let scattering_texture = StorageImage::new(
            window.device(),
            scattering_dimensions,
            Format::R32G32B32A32Sfloat,
            Some(window.queue().family()),
        )?;
        let single_mie_scattering_texture = StorageImage::new(
            window.device(),
            scattering_dimensions,
            Format::R32G32B32A32Sfloat,
            Some(window.queue().family()),
        )?;
        let irradiance_texture = StorageImage::new(
            window.device(),
            irradiance_dimensions,
            Format::R32G32B32A32Sfloat,
            Some(window.queue().family()),
        )?;

        // Initialize all accumulator textures.
        Self::clear_image(scattering_texture.clone(), window)?
            .join(Self::clear_image(
                single_mie_scattering_texture.clone(),
                window,
            )?)
            .join(Self::clear_image(irradiance_texture.clone(), window)?)
            .then_signal_fence_and_flush()?
            .wait(None)?;

        Ok(Self {
            transmittance_dimensions,
            irradiance_dimensions,
            scattering_dimensions,

            compute_transmittance,
            compute_direct_irradiance,
            compute_single_scattering,
            compute_scattering_density,
            compute_indirect_irradiance,
            compute_multiple_scattering,

            delta_irradiance_texture,
            delta_rayleigh_scattering_texture,
            delta_mie_scattering_texture,
            delta_multiple_scattering_texture,
            delta_scattering_density_texture,

            transmittance_texture,
            scattering_texture,
            single_mie_scattering_texture,
            irradiance_texture,

            params,
        })
    }

    fn make_sampler(device: Arc<Device>) -> Fallible<Arc<Sampler>> {
        let sampler = Sampler::new(
            device.clone(),
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

        Ok(sampler)
    }

    pub fn build_textures(
        &self,
        num_precomputed_wavelengths: usize,
        num_scattering_passes: usize,
        window: &GraphicsWindow,
    ) -> Fallible<()> {
        let num_iterations = (num_precomputed_wavelengths + 2) / 3;
        let delta_lambda = (MAX_LAMBDA - MIN_LAMBDA) / (3.0 * num_iterations as f64);
        for i in 0..num_iterations {
            let lambdas = [
                MIN_LAMBDA + (3.0 * i as f64 + 0.5) * delta_lambda,
                MIN_LAMBDA + (3.0 * i as f64 + 1.5) * delta_lambda,
                MIN_LAMBDA + (3.0 * i as f64 + 2.5) * delta_lambda,
            ];
            // Do not include MAX_LUMINOUS_EFFICACY here to keep values
            // as close to 0 as possible to preserve maximal precision.
            // It is included in SKY_SPECTRA_RADIANCE_TO_LUMINANCE.
            // Note: Why do we scale by delta_lambda here?
            let l0 = wavelength_to_srgb(lambdas[0], delta_lambda);
            let l1 = wavelength_to_srgb(lambdas[1], delta_lambda);
            let l2 = wavelength_to_srgb(lambdas[2], delta_lambda);
            // Stuff these factors into a matrix by columns so that our GPU can do the
            // conversion for us quickly.
            let rad_to_lum = Matrix3::new(
                l0[0], l1[0], l2[0], l0[1], l1[1], l2[1], l0[2], l1[2], l2[2],
            );
            self.precompute_one_step(lambdas, num_scattering_passes, rad_to_lum, window)?;
        }

        // Rebuild transmittance at RGB instead of high UV.
        // Upload atmosphere parameters for this set of wavelengths.
        let atmosphere_params_buffer = CpuAccessibleBuffer::from_data(
            window.device(),
            BufferUsage::all(),
            self.params.sample(RGB_LAMBDAS),
        )?;
        self.compute_transmittance_at(RGB_LAMBDAS, window, atmosphere_params_buffer.clone())?;

        if true {
            Self::dump_2d(
                "dump/sky/final-transmittance-texture.png",
                self.transmittance_texture.clone(),
                window,
            )?;
            Self::dump_2d(
                "dump/sky/final-irradiance-texture.png",
                self.irradiance_texture.clone(),
                window,
            )?;
            Self::dump_3d(
                "dump/sky/final-scattering-texture",
                self.scattering_texture.clone(),
                window,
            )?;
            Self::dump_3d(
                "dump/sky/final-single-mie-scattering-texture",
                self.single_mie_scattering_texture.clone(),
                window,
            )?;
        }

        Ok(())
    }

    fn precompute_one_step(
        &self,
        lambdas: [f64; 3],
        num_scattering_passes: usize,
        rad_to_lum: Matrix3<f64>,
        window: &GraphicsWindow,
    ) -> Fallible<()> {
        // Upload atmosphere parameters for this set of wavelengths.
        let atmosphere_params_buffer = CpuAccessibleBuffer::from_data(
            window.device(),
            BufferUsage::all(),
            self.params.sample(lambdas),
        )?;
        let mut q: [f32; 9] = Default::default();
        let m: Matrix3<f32> = nalgebra::convert(rad_to_lum);
        q.copy_from_slice(m.as_slice());
        // Expand into a mat4 to handle alignment restrictions.
        let raw = [
            q[0], q[1], q[2], 0f32, q[3], q[4], q[5], 0f32, q[6], q[7], q[8], 0f32, 0f32, 0f32,
            0f32, 0f32,
        ];
        let rad_to_lum_buffer =
            CpuAccessibleBuffer::from_data(window.device(), BufferUsage::all(), raw)?;

        self.compute_transmittance_at(lambdas, window, atmosphere_params_buffer.clone())?;

        self.compute_direct_irradiance_at(lambdas, window, atmosphere_params_buffer.clone())?;

        self.compute_single_scattering_at(
            lambdas,
            window,
            atmosphere_params_buffer.clone(),
            rad_to_lum_buffer.clone(),
        )?;

        for scattering_order in 2..=num_scattering_passes {
            let scattering_order_buffer = CpuAccessibleBuffer::from_data(
                window.device(),
                BufferUsage::all(),
                scattering_order as u32,
            )?;

            self.compute_scattering_density_at(
                lambdas,
                scattering_order,
                window,
                atmosphere_params_buffer.clone(),
                rad_to_lum_buffer.clone(),
                scattering_order_buffer.clone(),
            )?;

            self.compute_indirect_irradiance_at(
                lambdas,
                scattering_order,
                window,
                atmosphere_params_buffer.clone(),
                rad_to_lum_buffer.clone(),
            )?;

            self.compute_multiple_scattering_at(
                lambdas,
                scattering_order,
                window,
                atmosphere_params_buffer.clone(),
                rad_to_lum_buffer.clone(),
                scattering_order_buffer.clone(),
            )?;
        }

        Ok(())
    }
}

impl Precompute {
    fn show_range(buf: &[f32], path: &str) {
        use num_traits::float::Float;
        let mut minf = f32::max_value();
        let mut maxf = f32::min_value();
        for v in buf {
            if *v > maxf {
                maxf = *v;
            }
            if *v < minf {
                minf = *v;
            }
        }
        println!("RANGE: {} -> {} in {}", minf, maxf, path);
    }

    fn compress_pixels(src: &[f32], dim: Dimensions) -> Vec<u8> {
        const WHITE_POINT_R: f32 = 1.082414f32;
        const WHITE_POINT_G: f32 = 0.967556f32;
        const WHITE_POINT_B: f32 = 0.950030f32;
        const EXPOSURE: f32 = 683f32 * 0.0001f32;
        let mut bytes = Vec::with_capacity(dim.width() as usize * dim.height() as usize * 3);
        for i in 0usize..(dim.width() * dim.height() * dim.depth()) as usize {
            let r0 = src[4 * i + 0];
            let g0 = src[4 * i + 1];
            let b0 = src[4 * i + 2];

            let mut r1 = (1.0 - (-r0 / WHITE_POINT_R * EXPOSURE).exp()).powf(1.0 / 2.2);
            let mut g1 = (1.0 - (-g0 / WHITE_POINT_G * EXPOSURE).exp()).powf(1.0 / 2.2);
            let mut b1 = (1.0 - (-b0 / WHITE_POINT_B * EXPOSURE).exp()).powf(1.0 / 2.2);

            if r1.is_nan() {
                r1 = 0f32;
            }
            if g1.is_nan() {
                g1 = 0f32;
            }
            if b1.is_nan() {
                b1 = 0f32;
            }

            assert!(r1 >= 0.0 && r1 <= 1.0);
            assert!(g1 >= 0.0 && g1 <= 1.0);
            assert!(b1 >= 0.0 && b1 <= 1.0);

            bytes.push((r1 * 255f32) as u8);
            bytes.push((g1 * 255f32) as u8);
            bytes.push((b1 * 255f32) as u8);
        }
        bytes
    }

    fn dump_2d(
        path: &str,
        image: Arc<StorageImage<Format>>,
        window: &GraphicsWindow,
    ) -> Fallible<()> {
        let dim = image.dimensions();
        let nelems = dim.width() * dim.height() * 4;
        let buf = CpuAccessibleBuffer::from_iter(
            window.device(),
            BufferUsage::all(),
            (0..nelems).map(|_| 0f32),
        )?;
        let command_buffer =
            AutoCommandBufferBuilder::new(window.device(), window.queue().family())?
                .copy_image_to_buffer(image.clone(), buf.clone())?
                .build()?;
        let finished = command_buffer.execute(window.queue())?;
        finished.then_signal_fence_and_flush()?.wait(None)?;
        Self::show_range(&buf.read()?, path);
        let bytes = Self::compress_pixels(&buf.read()?, dim);
        let image =
            ImageBuffer::<Rgb<u8>, _>::from_raw(dim.width(), dim.height(), bytes.as_slice())
                .unwrap();
        image.save(path)?;
        Ok(())
    }

    fn dump_3d(
        base_path: &str,
        image: Arc<StorageImage<Format>>,
        window: &GraphicsWindow,
    ) -> Fallible<()> {
        let dim = image.dimensions();
        let nelems = dim.width() * dim.height() * dim.depth() * 4;
        let buf = CpuAccessibleBuffer::from_iter(
            window.device(),
            BufferUsage::all(),
            (0..nelems).map(|_| 0f32),
        )?;
        let command_buffer =
            AutoCommandBufferBuilder::new(window.device(), window.queue().family())?
                .copy_image_to_buffer(image.clone(), buf.clone())?
                .build()?;
        let finished = command_buffer.execute(window.queue())?;
        finished.then_signal_fence_and_flush()?.wait(None)?;

        let raw_pix = &buf.read()?;
        let buffer_content = Self::compress_pixels(raw_pix, dim);
        let layer_size = (dim.width() * dim.height() * 3) as usize;
        for layer_num in 0..dim.depth() as usize {
            print!("layer: {}, ", layer_num);

            let raw_layer_size = (dim.width() * dim.height() * 4) as usize;
            let raw_layer = &raw_pix[raw_layer_size * layer_num..raw_layer_size * (layer_num + 1)];
            Self::show_range(raw_layer, base_path);
            let layer = &buffer_content[layer_size * layer_num..layer_size * (layer_num + 1)];
            let image =
                ImageBuffer::<Rgb<u8>, _>::from_raw(dim.width(), dim.height(), &layer[..]).unwrap();
            image
                .save(&format!("{}-{:02}.png", base_path, layer_num))
                .unwrap();
        }
        Ok(())
    }

    fn clear_image(
        image: Arc<StorageImage<Format>>,
        window: &GraphicsWindow,
    ) -> Fallible<Box<GpuFuture>> {
        let nelems = match image.dimensions() {
            Dimensions::Dim2d { width, height } => width * height,
            Dimensions::Dim3d {
                width,
                height,
                depth,
            } => width * height * depth,
            dim => bail!("don't know how to handle dimensions: {:?}", dim),
        } * 4;
        let buf = CpuAccessibleBuffer::from_iter(
            window.device(),
            BufferUsage::all(),
            (0..nelems).map(|_| 0f32),
        )?;
        let command_buffer =
            AutoCommandBufferBuilder::new(window.device(), window.queue().family())?
                .copy_buffer_to_image(buf.clone(), image.clone())?
                .build()?;
        let finished = command_buffer.execute(window.queue())?;
        Ok(Box::new(finished) as Box<GpuFuture>)
    }

    pub fn make_immutable(
        self,
        window: &GraphicsWindow,
    ) -> Fallible<(
        Arc<ImmutableImage<Format>>,
        Arc<ImmutableImage<Format>>,
        Arc<ImmutableImage<Format>>,
        Arc<ImmutableImage<Format>>,
    )> {
        let usage = ImageUsage {
            transfer_destination: true,
            sampled: true,
            ..ImageUsage::none()
        };

        let (read_transmittance_texture, upload_transmittance_texture) =
            ImmutableImage::uninitialized(
                window.device(),
                self.transmittance_dimensions,
                Format::R32G32B32A32Sfloat,
                MipmapsCount::One,
                usage,
                ImageLayout::TransferDstOptimal,
                Some(window.queue().family()),
            )?;
        let (read_scattering_texture, upload_scattering_texture) = ImmutableImage::uninitialized(
            window.device(),
            self.scattering_dimensions,
            Format::R32G32B32A32Sfloat,
            MipmapsCount::One,
            usage,
            ImageLayout::TransferDstOptimal,
            Some(window.queue().family()),
        )?;
        let (read_single_mie_scattering_texture, upload_single_mie_scattering_texture) =
            ImmutableImage::uninitialized(
                window.device(),
                self.scattering_dimensions,
                Format::R32G32B32A32Sfloat,
                MipmapsCount::One,
                usage,
                ImageLayout::TransferDstOptimal,
                Some(window.queue().family()),
            )?;
        let (read_irradiance_texture, upload_irradiance_texture) = ImmutableImage::uninitialized(
            window.device(),
            self.irradiance_dimensions,
            Format::R32G32B32A32Sfloat,
            MipmapsCount::One,
            usage,
            ImageLayout::TransferDstOptimal,
            Some(window.queue().family()),
        )?;

        let command_buffer =
            AutoCommandBufferBuilder::new(window.device(), window.queue().family())?
                .copy_image(
                    self.transmittance_texture.clone(),
                    [0, 0, 0],
                    0,
                    0,
                    upload_transmittance_texture,
                    [0, 0, 0],
                    0,
                    0,
                    [
                        self.transmittance_dimensions.width(),
                        self.transmittance_dimensions.height(),
                        1,
                    ],
                    1,
                )?
                .copy_image(
                    self.scattering_texture.clone(),
                    [0, 0, 0],
                    0,
                    0,
                    upload_scattering_texture,
                    [0, 0, 0],
                    0,
                    0,
                    [
                        self.scattering_dimensions.width(),
                        self.scattering_dimensions.height(),
                        self.scattering_dimensions.depth(),
                    ],
                    1,
                )?
                .copy_image(
                    self.single_mie_scattering_texture.clone(),
                    [0, 0, 0],
                    0,
                    0,
                    upload_single_mie_scattering_texture,
                    [0, 0, 0],
                    0,
                    0,
                    [
                        self.scattering_dimensions.width(),
                        self.scattering_dimensions.height(),
                        self.scattering_dimensions.depth(),
                    ],
                    1,
                )?
                .copy_image(
                    self.irradiance_texture.clone(),
                    [0, 0, 0],
                    0,
                    0,
                    upload_irradiance_texture,
                    [0, 0, 0],
                    0,
                    0,
                    [
                        self.irradiance_dimensions.width(),
                        self.irradiance_dimensions.height(),
                        1,
                    ],
                    1,
                )?
                .build()?;

        let finished = command_buffer.execute(window.queue())?;
        finished.then_signal_fence_and_flush()?.wait(None)?;

        Ok((
            read_transmittance_texture,
            read_scattering_texture,
            read_single_mie_scattering_texture,
            read_irradiance_texture,
        ))
    }
}
