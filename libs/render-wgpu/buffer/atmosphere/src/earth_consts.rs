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
use crate::colorspace::{cie_color_coefficient_at_wavelength, convert_xyz_to_srgb};
use num_traits::pow::Pow;
use static_assertions::{assert_eq_align, assert_eq_size};
use std::{f64::consts::PI as PI64, mem, ops::Range};

pub const RGB_LAMBDAS: [f64; 4] = [680.0, 550.0, 440.0, 0.0];

#[derive(Copy, Clone)]
pub struct DensityProfileLayer {
    // Height of this layer, except for the last layer which always
    // extends to the top of the atmosphere region.
    width: f32, // meters

    // Density in this layer in [0,1) as defined by the following function:
    //   'exp_term' * exp('exp_scale' * h) + 'linear_term' * h + 'constant_term',
    exp_term: f32,
    exp_scale: f32,   // 1 / meters
    linear_term: f32, // 1 / meters
    constant_term: f32,
    _pad: [f32; 3],
}
assert_eq_size!(DensityProfileLayer, [f32; 8]);
assert_eq_align!(DensityProfileLayer, [f32; 4]);

// From low to high.
#[derive(Copy, Clone)]
pub struct DensityProfile {
    // Note: Arrays are busted in shaderc right now.
    layer0: DensityProfileLayer,
    layer1: DensityProfileLayer,
}
assert_eq_size!(DensityProfile, [f32; 16]);
assert_eq_align!(DensityProfile, [f32; 4]);

#[derive(Copy, Clone)]
pub struct AtmosphereParameters {
    // The density profile of tiny air molecules.
    rayleigh_density: DensityProfile,

    // The density profile of aerosols.
    mie_density: DensityProfile,

    // The density profile of O3.
    absorption_density: DensityProfile,

    // Per component, at max density.
    rayleigh_scattering_coefficient: [f32; 4],

    // Per component, at max density.
    mie_scattering_coefficient: [f32; 4],

    // Per component, at max density.
    mie_extinction_coefficient: [f32; 4],

    // Per component, at max density.
    absorption_extinction_coefficient: [f32; 4],

    // Energy received into the system from the nearby star.
    sun_irradiance: [f32; 4],

    // The average albedo of the ground, per component.
    pub ground_albedo: [f32; 4],

    // The whitepoint, given the relative contributions of all possible wavelengths.
    whitepoint: [f32; 4],

    // Conversion between the solar irradiance above and our desired sRGB luminance output.
    sun_spectral_radiance_to_luminance: [f32; 3],
    _pad0: f32,

    // Conversion between the irradiance stored in our LUT and sRGB luminance outputs.
    // Note that this is where we re-add the luminous efficacy constant that we factored
    // out of the precomputations to keep the numbers closer to 0 for precision.
    sky_spectral_radiance_to_luminance: [f32; 3],

    // From center to subocean.
    bottom_radius: f32, // meters

    // from center to top of simulated atmosphere.
    top_radius: f32, // meters

    // The size of the nearby star in radians.
    sun_angular_radius: f32, // radians

    // The asymmetry parameter for the Cornette-Shanks phase function for the
    // aerosols.
    mie_phase_function_g: f32,

    // The cosine of the maximum Sun zenith angle for which atmospheric scattering
    // must be precomputed (for maximum precision, use the smallest Sun zenith
    // angle yielding negligible sky light radiance values. For instance, for the
    // Earth case, 102 degrees is a good choice - yielding mu_s_min = -0.2).
    mu_s_min: f32,
}
assert_eq_size!(AtmosphereParameters, [f32; 40 + 16 * 3]);
assert_eq_align!(AtmosphereParameters, [f32; 4]);
pub const ATMOSPHERE_PARAMETERS_BUFFER_SIZE: u64 = mem::size_of::<AtmosphereParameters>() as u64;

// Evaluate the wavelength-based table at the given wavelength,
// interpolating between adjacent table values.
fn interpolate_at_lambda(wavelengths: &[f64], properties: &[f64], wavelength: f64) -> f64 {
    assert_eq!(properties.len(), wavelengths.len());
    if wavelength < wavelengths[0] {
        return properties[0];
    }
    for (wl, props) in wavelengths.windows(2).zip(properties.windows(2)) {
        if wavelength < wl[1] {
            let f = (wavelength - wl[0]) / (wl[1] - wl[0]);
            return props[0] * (1.0 - f) + props[1] * f;
        }
    }
    *properties.last().expect("non empty list")
}

fn interpolate(wavelengths: &[f64], properties: &[f64], lambdas: [f64; 4], scale: f64) -> [f32; 4] {
    [
        (interpolate_at_lambda(wavelengths, properties, lambdas[0]) * scale) as f32,
        (interpolate_at_lambda(wavelengths, properties, lambdas[1]) * scale) as f32,
        (interpolate_at_lambda(wavelengths, properties, lambdas[2]) * scale) as f32,
        (interpolate_at_lambda(wavelengths, properties, lambdas[3]) * scale) as f32,
    ]
}

impl Default for DensityProfileLayer {
    fn default() -> Self {
        Self {
            width: 0f32,
            exp_term: 0f32,
            exp_scale: 0f32,
            linear_term: 0f32,
            constant_term: 0f32,
            _pad: [0f32; 3],
        }
    }
}

// Values from "Reference Solar Spectral Irradiance: ASTM G-173", ETR column
// (see http://rredc.nrel.gov/solar/spectra/am1.5/ASTMG173/ASTMG173.html),
// summed and averaged in each bin (e.g. the value for 360nm is the average
// of the ASTM G-173 values for all wavelengths between 360 and 370nm).
// Values in W.m^-2.
const LAMBDA_RANGE: Range<i32> = 360..830; // by 10
const SOLAR_IRRADIANCE: [f64; 48] = [
    1.11776, 1.14259, 1.01249, 1.14716, 1.72765, 1.73054, 1.6887, 1.61253, 1.91198, 2.03474,
    2.02042, 2.02212, 1.93377, 1.95809, 1.91686, 1.8298, 1.8685, 1.8931, 1.85149, 1.8504, 1.8341,
    1.8345, 1.8147, 1.78158, 1.7533, 1.6965, 1.68194, 1.64654, 1.6048, 1.52143, 1.55622, 1.5113,
    1.474, 1.4482, 1.41018, 1.36775, 1.34188, 1.31429, 1.28303, 1.26758, 1.2367, 1.2082, 1.18737,
    1.14683, 1.12362, 1.1058, 1.07124, 1.04992,
];
// Values from http://www.iup.uni-bremen.de/gruppen/molspec/databases/
// referencespectra/o3spectra2011/index.html for 233K, summed and averaged in
// each bin (e.g. the value for 360nm is the average of the original values
// for all wavelengths between 360 and 370nm). Values in m^2.
const OZONE_CROSS_SECTION: [f64; 48] = [
    1.18e-27, 2.182e-28, 2.818e-28, 6.636e-28, 1.527e-27, 2.763e-27, 5.52e-27, 8.451e-27,
    1.582e-26, 2.316e-26, 3.669e-26, 4.924e-26, 7.752e-26, 9.016e-26, 1.48e-25, 1.602e-25,
    2.139e-25, 2.755e-25, 3.091e-25, 3.5e-25, 4.266e-25, 4.672e-25, 4.398e-25, 4.701e-25,
    5.019e-25, 4.305e-25, 3.74e-25, 3.215e-25, 2.662e-25, 2.238e-25, 1.852e-25, 1.473e-25,
    1.209e-25, 9.423e-26, 7.455e-26, 6.566e-26, 5.105e-26, 4.15e-26, 4.228e-26, 3.237e-26,
    2.451e-26, 2.801e-26, 2.534e-26, 1.624e-26, 1.465e-26, 2.078e-26, 1.383e-26, 7.105e-27,
];
// From https://en.wikipedia.org/wiki/Dobson_unit, in molecules.m^-2.
const DOBSON_UNIT: f64 = 2.687e20;
// Maximum number density of ozone molecules, in m^-3 (computed so at to get
// 300 Dobson units of ozone - for this we divide 300 DU by the integral of
// the ozone density profile defined below, which is equal to 15km).
const MAX_OZONE_NUMBER_DENSITY: f64 = 300.0 * DOBSON_UNIT / 15_000.0;
const RAYLEIGH_SCATTER_COEFFICIENT: f64 = 1.24062e-6;
const RAYLEIGH_SCALE_HEIGHT: f64 = 8000.0;
const MIE_SCALE_HEIGHT: f64 = 1200.0;
const MIE_ANGSTROM_ALPHA: f64 = 0.0;
const MIE_ANGSTROM_BETA: f64 = 5.328e-3;
const MIE_SINGLE_SCATTERING_ALBEDO: f64 = 0.9;
const MIE_PHASE_FUNCTION_G: f64 = 0.8;
const GROUND_ALBEDO: f64 = 0.1;
const MAX_SUN_ZENITH_ANGLE: f64 = 120.0 / 180.0 * PI64;
const MAX_LUMINOUS_EFFICACY: f64 = 683.0;

pub struct EarthParameters {
    wavelengths: Vec<f64>,
    sun_irradiance: Vec<f64>,
    rayleigh_scattering: Vec<f64>,
    mie_scattering: Vec<f64>,
    mie_extinction: Vec<f64>,
    absorption_extinction: Vec<f64>,
    ground_albedo: Vec<f64>,
    sun_spectral_radiance_to_luminance: [f32; 3],
    whitepoint: [f32; 4],
}

impl EarthParameters {
    pub fn new() -> Self {
        // Our atmosphere parameters are sampled at 47 wavelengths. Expand all of our other
        // parameters that are consistent across all wavelengths to the same dimensionality.
        let mut wavelengths = Vec::new();
        let mut sun_irradiance = Vec::new();
        let mut rayleigh_scattering = Vec::new();
        let mut mie_scattering = Vec::new();
        let mut mie_extinction = Vec::new();
        let mut absorption_extinction = Vec::new();
        let mut ground_albedo = Vec::new();
        for ((l, sun_irr), ozone_cross_sec) in LAMBDA_RANGE
            .step_by(10)
            .zip(SOLAR_IRRADIANCE.iter())
            .zip(OZONE_CROSS_SECTION.iter())
        {
            let lf = f64::from(l);
            wavelengths.push(lf);
            sun_irradiance.push(*sun_irr);
            let lambda = lf / 1000.0; // um
            rayleigh_scattering.push(RAYLEIGH_SCATTER_COEFFICIENT * lambda.pow(-4.0));
            let mie = MIE_ANGSTROM_BETA / MIE_SCALE_HEIGHT * lambda.pow(-MIE_ANGSTROM_ALPHA);
            mie_scattering.push(mie * MIE_SINGLE_SCATTERING_ALBEDO);
            mie_extinction.push(mie);
            absorption_extinction.push(MAX_OZONE_NUMBER_DENSITY * ozone_cross_sec);
            ground_albedo.push(GROUND_ALBEDO);
        }
        let sun_spectral_radiance_to_luminance =
            Self::compute_spectral_radiance_to_luminance_factors(
                &wavelengths,
                &sun_irradiance,
                0.0,
            );
        let srgb = Self::compute_solar_irradiance_to_linear_srgb(&wavelengths, &sun_irradiance);
        let avg = (srgb[0] + srgb[1] + srgb[2]) / 3.0;
        let whitepoint = [
            (srgb[0] / avg) as f32,
            (srgb[1] / avg) as f32,
            (srgb[2] / avg) as f32,
            1f32,
        ];

        Self {
            wavelengths,
            sun_irradiance,
            rayleigh_scattering,
            mie_scattering,
            mie_extinction,
            absorption_extinction,
            ground_albedo,
            sun_spectral_radiance_to_luminance,
            whitepoint,
        }
    }

    // The returned constants are in lumen.nm / watt.
    fn compute_spectral_radiance_to_luminance_factors(
        wavelengths: &[f64],
        sun_irradiance: &[f64],
        lambda_power: f64,
    ) -> [f32; 3] {
        let mut out = [0f32; 3];
        let solar = interpolate(&wavelengths, &sun_irradiance, RGB_LAMBDAS, 1.0);
        for lambda in LAMBDA_RANGE {
            let f_lambda = f64::from(lambda);
            let xyz_bar = cie_color_coefficient_at_wavelength(f_lambda);
            let rgb_bar = convert_xyz_to_srgb(xyz_bar, 1.0);
            let irradiance = interpolate_at_lambda(&wavelengths, &sun_irradiance, f_lambda);
            for i in 0..3 {
                out[i] += (rgb_bar[i] * irradiance / f64::from(solar[i])
                    * (f_lambda / RGB_LAMBDAS[i]).pow(lambda_power))
                    as f32;
            }
        }
        for o in &mut out {
            *o *= MAX_LUMINOUS_EFFICACY as f32;
        }
        out
    }

    pub fn compute_solar_irradiance_to_linear_srgb(
        wavelengths: &[f64],
        sun_irradiance: &[f64],
    ) -> [f64; 3] {
        let mut x = 0f64;
        let mut y = 0f64;
        let mut z = 0f64;
        for lambda in LAMBDA_RANGE {
            let f_lambda = f64::from(lambda);
            let value = interpolate_at_lambda(wavelengths, sun_irradiance, f_lambda);
            let xyz = cie_color_coefficient_at_wavelength(f_lambda);
            x += xyz[0] * value;
            y += xyz[1] * value;
            z += xyz[2] * value;
        }
        convert_xyz_to_srgb([x, y, z], MAX_LUMINOUS_EFFICACY)
    }

    pub fn sample(&self, lambdas: [f64; 4]) -> AtmosphereParameters {
        // Evaluate our physical model for use in a shader.
        const LENGTH_SCALE: f64 = 1000.0;
        AtmosphereParameters {
            _pad0: 0f32,
            sun_irradiance: interpolate(&self.wavelengths, &self.sun_irradiance, lambdas, 1.0),
            sun_angular_radius: 0.00935 / 2.0,
            sun_spectral_radiance_to_luminance: self.sun_spectral_radiance_to_luminance,
            sky_spectral_radiance_to_luminance: [
                MAX_LUMINOUS_EFFICACY as f32,
                MAX_LUMINOUS_EFFICACY as f32,
                MAX_LUMINOUS_EFFICACY as f32,
            ],
            bottom_radius: (6_360_000.0 / LENGTH_SCALE) as f32,
            top_radius: (6_420_000.0 / LENGTH_SCALE) as f32,
            rayleigh_density: DensityProfile {
                layer0: Default::default(),
                layer1: DensityProfileLayer {
                    width: 0f32,
                    exp_term: 1f32,
                    exp_scale: (-1.0 / RAYLEIGH_SCALE_HEIGHT * LENGTH_SCALE) as f32,
                    linear_term: 0f32,
                    constant_term: 0f32,
                    _pad: [0f32; 3],
                },
            },
            rayleigh_scattering_coefficient: interpolate(
                &self.wavelengths,
                &self.rayleigh_scattering,
                lambdas,
                LENGTH_SCALE,
            ),
            mie_density: DensityProfile {
                layer0: Default::default(),
                layer1: DensityProfileLayer {
                    width: 0f32,
                    exp_term: 1f32,
                    exp_scale: (-1.0 / MIE_SCALE_HEIGHT * LENGTH_SCALE) as f32,
                    linear_term: 0f32,
                    constant_term: 0f32,
                    _pad: [0f32; 3],
                },
            },
            mie_scattering_coefficient: interpolate(
                &self.wavelengths,
                &self.mie_scattering,
                lambdas,
                LENGTH_SCALE,
            ),
            mie_extinction_coefficient: interpolate(
                &self.wavelengths,
                &self.mie_extinction,
                lambdas,
                LENGTH_SCALE,
            ),
            mie_phase_function_g: MIE_PHASE_FUNCTION_G as f32,
            absorption_density: DensityProfile {
                layer0: DensityProfileLayer {
                    width: (25_000.0 / LENGTH_SCALE) as f32,
                    exp_term: 0f32,
                    exp_scale: 0f32,
                    linear_term: (1.0 / 15_000.0 * LENGTH_SCALE) as f32,
                    constant_term: -2f32 / 3f32,
                    _pad: [0f32; 3],
                },
                layer1: DensityProfileLayer {
                    width: 0f32,
                    exp_term: 0f32,
                    exp_scale: 0f32,
                    linear_term: (-1.0 / 15_000.0 * LENGTH_SCALE) as f32,
                    constant_term: 8f32 / 3f32,
                    _pad: [0f32; 3],
                },
            },
            absorption_extinction_coefficient: interpolate(
                &self.wavelengths,
                &self.absorption_extinction,
                lambdas,
                LENGTH_SCALE,
            ),
            ground_albedo: interpolate(&self.wavelengths, &self.ground_albedo, lambdas, 1.0),
            whitepoint: self.whitepoint,
            mu_s_min: MAX_SUN_ZENITH_ANGLE.cos() as f32,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use failure::Fallible;
    use spirv_reflect::ShaderModule;

    macro_rules! offsetof {
        ($obj:expr, $field:ident) => {
            &$obj.$field as *const _ as usize - &$obj as *const _ as usize
        };
    }

    #[test]
    fn test_layout() -> Fallible<()> {
        let earth = EarthParameters::new();
        let a = earth.sample(RGB_LAMBDAS);

        let expect_offsets = [
            offsetof!(a, rayleigh_density),
            offsetof!(a, mie_density),
            offsetof!(a, absorption_density),
            offsetof!(a, rayleigh_scattering_coefficient),
            offsetof!(a, mie_scattering_coefficient),
            offsetof!(a, mie_extinction_coefficient),
            offsetof!(a, absorption_extinction_coefficient),
            offsetof!(a, sun_irradiance),
            offsetof!(a, ground_albedo),
            offsetof!(a, whitepoint),
            offsetof!(a, sun_spectral_radiance_to_luminance),
            offsetof!(a, sky_spectral_radiance_to_luminance),
            offsetof!(a, bottom_radius),
            offsetof!(a, top_radius),
            offsetof!(a, sun_angular_radius),
            offsetof!(a, mie_phase_function_g),
            offsetof!(a, mu_s_min),
        ];

        let module = ShaderModule::load_u8_data(include_bytes!(
            "../target/build_transmittance_lut.comp.spirv"
        ))
        .unwrap();
        let bindings = module.enumerate_descriptor_bindings(None).unwrap();
        for binding in &bindings {
            if binding.binding == 0 {
                let block = &binding.block.members.first().unwrap();
                for (i, member) in block.members.iter().enumerate() {
                    assert_eq!(expect_offsets[i], member.offset as usize);
                }
            }
        }

        Ok(())
    }
}
