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

use crate::fs;
use num_traits::pow::Pow;
use std::{f64::consts::PI as PI64, ops::Range};

pub const RGB_LAMBDAS: [f64; 3] = [680.0, 550.0, 440.0];

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

fn interpolate(wavelengths: &[f64], properties: &[f64], lambdas: [f64; 3], scale: f64) -> [f32; 3] {
    [
        (interpolate_at_lambda(wavelengths, properties, lambdas[0]) * scale) as f32,
        (interpolate_at_lambda(wavelengths, properties, lambdas[1]) * scale) as f32,
        (interpolate_at_lambda(wavelengths, properties, lambdas[2]) * scale) as f32,
    ]
}

impl Default for fs::ty::DensityProfileLayer {
    fn default() -> Self {
        Self {
            width: 0f32,
            exp_term: 0f32,
            exp_scale: 0f32,
            linear_term: 0f32,
            constant_term: 0f32,
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
const MAX_OZONE_NUMBER_DENSITY: f64 = 300.0 * DOBSON_UNIT / 15000.0;
const RAYLEIGH_SCATTER_COEFFICIENT: f64 = 1.24062e-6;
const RAYLEIGH_SCALE_HEIGHT: f64 = 8000.0;
const MIE_SCALE_HEIGHT: f64 = 1200.0;
const MIE_ANGSTROM_ALPHA: f64 = 0.0;
const MIE_ANGSTROM_BETA: f64 = 5.328e-3;
const MIE_SINGLE_SCATTERING_ALBEDO: f64 = 0.9;
const MIE_PHASE_FUNCTION_G: f64 = 0.8;
const GROUND_ALBEDO: f64 = 0.1;
const MAX_SUN_ZENITH_ANGLE: f64 = 120.0 / 180.0 * PI64;

pub struct EarthParameters {
    wavelengths: Vec<f64>,
    sun_irradiance: Vec<f64>,
    rayleigh_scattering: Vec<f64>,
    mie_scattering: Vec<f64>,
    mie_extinction: Vec<f64>,
    absorption_extinction: Vec<f64>,
    ground_albedo: Vec<f64>,
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
            wavelengths.push(l as f64);
            sun_irradiance.push(*sun_irr);
            let lambda = f64::from(l) / 1000.0; // um
            rayleigh_scattering.push(RAYLEIGH_SCATTER_COEFFICIENT * lambda.pow(-4.0));
            let mie = MIE_ANGSTROM_BETA / MIE_SCALE_HEIGHT * lambda.pow(-MIE_ANGSTROM_ALPHA);
            mie_scattering.push(mie * MIE_SINGLE_SCATTERING_ALBEDO);
            mie_extinction.push(mie);
            absorption_extinction.push(MAX_OZONE_NUMBER_DENSITY * ozone_cross_sec);
            ground_albedo.push(GROUND_ALBEDO);
        }

        Self {
            wavelengths,
            sun_irradiance,
            rayleigh_scattering,
            mie_scattering,
            mie_extinction,
            absorption_extinction,
            ground_albedo,
        }
    }

    pub fn sample(&self, lambdas: [f64; 3]) -> fs::ty::AtmosphereParameters {
        // Evaluate our physical model for use in a shader.
        const LENGTH_SCALE: f64 = 1000.0;
        fs::ty::AtmosphereParameters {
            sun_irradiance: interpolate(&self.wavelengths, &self.sun_irradiance, lambdas, 1.0),
            sun_angular_radius: 0.00935 / 2.0,
            bottom_radius: (6_360_000.0 / LENGTH_SCALE) as f32,
            top_radius: (6_420_000.0 / LENGTH_SCALE) as f32,
            rayleigh_density: fs::ty::DensityProfile {
                layer0: Default::default(),
                layer1: fs::ty::DensityProfileLayer {
                    width: 0f32,
                    exp_term: 1f32,
                    exp_scale: (-1.0 / RAYLEIGH_SCALE_HEIGHT * LENGTH_SCALE) as f32,
                    linear_term: 0f32,
                    constant_term: 0f32,
                },
                _dummy0: Default::default(),
            },
            rayleigh_scattering_coefficient: interpolate(
                &self.wavelengths,
                &self.rayleigh_scattering,
                lambdas,
                LENGTH_SCALE,
            ),
            mie_density: fs::ty::DensityProfile {
                layer0: Default::default(),
                layer1: fs::ty::DensityProfileLayer {
                    width: 0f32,
                    exp_term: 1f32,
                    exp_scale: (-1.0 / MIE_SCALE_HEIGHT * LENGTH_SCALE) as f32,
                    linear_term: 0f32,
                    constant_term: 0f32,
                },
                _dummy0: Default::default(),
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
            absorption_density: fs::ty::DensityProfile {
                layer0: fs::ty::DensityProfileLayer {
                    width: (25_000.0 / LENGTH_SCALE) as f32,
                    exp_term: 0f32,
                    exp_scale: 0f32,
                    linear_term: (1.0 / 15_000.0 * LENGTH_SCALE) as f32,
                    constant_term: -2f32 / 3f32,
                },
                layer1: fs::ty::DensityProfileLayer {
                    width: 0f32,
                    exp_term: 0f32,
                    exp_scale: 0f32,
                    linear_term: (-1.0 / 15_000.0 * LENGTH_SCALE) as f32,
                    constant_term: 8f32 / 3f32,
                },
                _dummy0: Default::default(),
            },
            absorption_extinction_coefficient: interpolate(
                &self.wavelengths,
                &self.absorption_extinction,
                lambdas,
                LENGTH_SCALE,
            ),
            ground_albedo: interpolate(&self.wavelengths, &self.ground_albedo, lambdas, 1.0),
            mu_s_min: MAX_SUN_ZENITH_ANGLE.cos() as f32,
            _dummy0: Default::default(),
            _dummy1: Default::default(),
            _dummy2: Default::default(),
            _dummy3: Default::default(),
            _dummy4: Default::default(),
            _dummy5: Default::default(),
            _dummy6: Default::default(),
        }
    }
}
