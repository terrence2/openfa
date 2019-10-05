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
#version 450
#include <common/include/include_global.glsl>
#include <buffer/atmosphere/include/common.glsl>
#include <buffer/atmosphere/include/lut_builder_common.glsl>

layout(local_size_x = 8, local_size_y = 8, local_size_z = 1) in;
layout(binding = 0) uniform Data { AtmosphereParameters atmosphere; };
layout(binding = 1, rgba32f) uniform writeonly image2D transmittance_texture;

float
compute_optical_length_to_top_atmosphere_boundary(
    vec2 rmu,
    DensityProfile profile,
    float bottom_radius,
    float top_radius
) {
    float r = rmu.x;
    float mu = rmu.y;

    // assert(r >= bottom_radius && r <= top_radius);
    // assert(mu >= -1.0 && mu <= 1.0);
    // Number of intervals for the numerical integration.
    const int SAMPLE_COUNT = 500;
    // The integration step, i.e. the length of each integration interval.
    float dx = distance_to_top_atmosphere_boundary(rmu, top_radius) / float(SAMPLE_COUNT);
    // Integration loop.
    float result = 0.0;
    for (int i = 0; i <= SAMPLE_COUNT; ++i) {
        float d_i = float(i) * dx;
        // Distance between the current sample point and the planet center.
        float r_i = sqrt(d_i * d_i + 2.0 * r * mu * d_i + r * r);
        // Number density at the current sample point (divided by the number density
        // at the bottom of the atmosphere, yielding a dimensionless number).
        float y_i = get_profile_density(profile, r_i - bottom_radius);
        // Sample weight (from the trapezoidal rule).
        float weight_i = i == 0 || i == SAMPLE_COUNT ? 0.5 : 1.0;
        result += y_i * weight_i * dx;
    }
    return result;
}

vec4
compute_transmittance_to_top_atmosphere_boundary(
    vec2 rmu,
    AtmosphereParameters atmosphere
) {
    // assert(r >= atmosphere.bottom_radius && r <= atmosphere.top_radius);
    // assert(mu >= -1.0 && mu <= 1.0);
    vec4 rayleigh_depth = atmosphere.rayleigh_scattering_coefficient *
        compute_optical_length_to_top_atmosphere_boundary(
            rmu,
            atmosphere.rayleigh_density,
            atmosphere.bottom_radius,
            atmosphere.top_radius
        );

    vec4 mie_depth = atmosphere.mie_extinction_coefficient *
        compute_optical_length_to_top_atmosphere_boundary(
            rmu,
            atmosphere.mie_density,
            atmosphere.bottom_radius,
            atmosphere.top_radius
        );

    vec4 ozone_depth = atmosphere.absorption_extinction_coefficient *
        compute_optical_length_to_top_atmosphere_boundary(
            rmu,
            atmosphere.absorption_density,
            atmosphere.bottom_radius,
            atmosphere.top_radius
        );

    return exp(-(rayleigh_depth + mie_depth + ozone_depth));
}

void
compute_transmittance_program(
    vec2 coord,
    AtmosphereParameters atmosphere
) {
    const vec2 TEXTURE_SIZE = vec2(TRANSMITTANCE_TEXTURE_WIDTH, TRANSMITTANCE_TEXTURE_HEIGHT);
    vec2 uv = coord / TEXTURE_SIZE;
    vec2 rmu = transmittance_uv_to_rmu(
        uv,
        atmosphere.bottom_radius,
        atmosphere.top_radius
    );
    vec4 transmittance = compute_transmittance_to_top_atmosphere_boundary(rmu, atmosphere);
    imageStore(
        transmittance_texture,
        ivec2(coord),
        transmittance
    );
}

void
main()
{
    compute_transmittance_program(
        gl_GlobalInvocationID.xy + vec2(0.5, 0.5),
        atmosphere
    );
}
