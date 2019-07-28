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
#include "header.glsl"
#include "lut_shared_builder.glsl"

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
    AtmosphereParameters atmosphere,
    sampler3D delta_rayleigh_scattering_texture,
    sampler3D delta_mie_scattering_texture,
    sampler3D delta_multiple_scattering_texture
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
                delta_mie_scattering_texture,
                delta_multiple_scattering_texture,
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
    sampler3D delta_rayleigh_scattering_texture,
    sampler3D delta_mie_scattering_texture,
    sampler3D delta_multiple_scattering_texture,
    writeonly image2D delta_indirect_irradiance,
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
        atmosphere,
        delta_rayleigh_scattering_texture,
        delta_mie_scattering_texture,
        delta_multiple_scattering_texture
    );
    imageStore(
        delta_indirect_irradiance,
        ivec2(frag_coord),
        indirect_irradiance
    );
}
