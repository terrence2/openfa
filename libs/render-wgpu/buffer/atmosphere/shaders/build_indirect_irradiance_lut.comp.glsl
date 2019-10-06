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
#include <buffer/atmosphere/include/global.glsl>
#include <buffer/atmosphere/include/lut_builder_common.glsl>

layout(local_size_x = 8, local_size_y = 8, local_size_z = 1) in;
layout(binding = 0) uniform AtmosphereParams { AtmosphereParameters atmosphere; };
layout(binding = 1) uniform RadToLum { mat4 rad_to_lum; };
layout(binding = 2) uniform ScatteringOrder { uint scattering_order; };
layout(binding = 3) uniform texture3D delta_rayleigh_scattering_texture;
layout(binding = 4) uniform sampler delta_rayleigh_scattering_sampler;
layout(binding = 5) uniform texture3D delta_mie_scattering_texture;
layout(binding = 6) uniform sampler delta_mie_scattering_sampler;
layout(binding = 7) uniform texture3D delta_multiple_scattering_texture;
layout(binding = 8) uniform sampler delta_multiple_scattering_sampler;
layout(binding = 9, rgba32f) uniform writeonly image2D delta_irradiance_texture;
layout(binding = 10, rgba32f) uniform image2D irradiance_texture;

// For the indirect ground irradiance the integral over the hemisphere must be
// computed numerically. More precisely we need to compute the integral over all
// the directions $\bw$ of the hemisphere, of the product of:
//   * the radiance arriving from direction $\bw$ after $n$ bounces,
//   * the cosine factor, i.e. $\omega_z$
//     This leads to the following implementation (where
//     `multiple_scattering_texture` is supposed to contain the $n$-th
//     order of scattering, if $n>1$, and `scattering_order` is equal to
//     $n$):
vec4 compute_indirect_irradiance(
    vec2 rmus,
    uint scattering_order,
    AtmosphereParameters atmosphere
) {
    const int SAMPLE_COUNT = 32;
    const float dphi = PI / float(SAMPLE_COUNT);
    const float dtheta = PI / float(SAMPLE_COUNT);

    vec4 result = vec4(0.0);
    vec3 omega_s = vec3(sqrt(1.0 - rmus.y * rmus.y), 0.0, rmus.y);

    for (int j = 0; j < SAMPLE_COUNT / 2; ++j) {
        float theta = (float(j) + 0.5) * dtheta;
        for (int i = 0; i < 2 * SAMPLE_COUNT; ++i) {
            float phi = (float(i) + 0.5) * dphi;
            vec3 omega = vec3(cos(phi) * sin(theta), sin(phi) * sin(theta), cos(theta));
            float domega = dtheta * dphi * sin(theta);

            float nu = dot(omega, omega_s);
            result += get_best_scattering(
                delta_rayleigh_scattering_texture,
                delta_rayleigh_scattering_sampler,
                delta_mie_scattering_texture,
                delta_mie_scattering_sampler,
                delta_multiple_scattering_texture,
                delta_multiple_scattering_sampler,
                ScatterCoord(rmus.x, omega.z, rmus.y, nu),
                atmosphere.bottom_radius,
                atmosphere.top_radius,
                atmosphere.mu_s_min,
                atmosphere.mie_phase_function_g,
                false, // ray_r_theta_intersects_ground,
                scattering_order) * omega.z * domega;
        }
    }
    return result;
}

void compute_indirect_irradiance_program(
    vec2 frag_coord,
    uint scattering_order,
    AtmosphereParameters atmosphere,
    out vec4 indirect_irradiance
) {
    const vec2 TEXTURE_SIZE = vec2(IRRADIANCE_TEXTURE_WIDTH, IRRADIANCE_TEXTURE_HEIGHT);
    vec2 uv = frag_coord / TEXTURE_SIZE;
    vec2 rmus = irradiance_uv_to_rmus(
        uv,
        atmosphere.bottom_radius,
        atmosphere.top_radius
    );
    indirect_irradiance = compute_indirect_irradiance(
        rmus,
        scattering_order,
        atmosphere
    );
    imageStore(
        delta_irradiance_texture,
        ivec2(frag_coord),
        indirect_irradiance
    );
}

void main() {
    vec4 indirect_irradiance;
    compute_indirect_irradiance_program(
        gl_GlobalInvocationID.xy + vec2(0.5, 0.5),
        scattering_order,
        atmosphere,
        indirect_irradiance
    );

    vec3 prior_irradiance = imageLoad(
        irradiance_texture,
        ivec2(gl_GlobalInvocationID.xy)
    ).rgb;
    // FIXME: this should all be vec4... why are we subbing in a 1 here?
    imageStore(
        irradiance_texture,
        ivec2(gl_GlobalInvocationID.xy),
        vec4(prior_irradiance + vec3(rad_to_lum * indirect_irradiance), 1.0)
    );
}
