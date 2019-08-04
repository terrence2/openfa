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
#include "include_atmosphere.glsl"
#include "lut_shared_builder.glsl"

vec4
compute_multiple_scattering(
    ScatterCoord sc,
    AtmosphereParameters atmosphere,
    uint scattering_order,
    sampler2D transmittance_texture,
    sampler3D delta_scattering_density_texture,
    bool ray_r_mu_intersects_ground
) {
    // Number of intervals for the numerical integration.
    const int SAMPLE_COUNT = 50;
    // The integration step, i.e. the length of each integration interval.
    float dx = distance_to_nearest_atmosphere_boundary(
        vec2(sc.r, sc.mu),
        atmosphere.bottom_radius,
        atmosphere.top_radius,
        ray_r_mu_intersects_ground) / float(SAMPLE_COUNT);
    // Integration loop.
    vec4 rayleigh_mie_sum = vec4(0.0);
    for (int i = 0; i <= SAMPLE_COUNT; ++i) {
        float d_i = float(i) * dx;

        // The r, mu and mu_s parameters at the current integration point (see the
        // single scattering section for a detailed explanation).
        float r_i = clamp_radius(
            sqrt(d_i * d_i + 2.0 * sc.r * sc.mu * d_i + sc.r * sc.r),
            atmosphere.bottom_radius, atmosphere.top_radius
        );
        float mu_i = clamp_cosine((sc.r * sc.mu + d_i) / r_i);
        float mu_s_i = clamp_cosine((sc.r * sc.mu_s + d_i * sc.nu) / r_i);

        // The Rayleigh and Mie multiple scattering at the current sample point.
        vec4 rayleigh_mie_i = get_scattering(
                delta_scattering_density_texture,
                ScatterCoord(r_i, mu_i, mu_s_i, sc.nu),
                atmosphere.bottom_radius,
                atmosphere.top_radius,
                atmosphere.mu_s_min,
                ray_r_mu_intersects_ground
            ) * get_transmittance(
                transmittance_texture,
                sc.r,
                sc.mu,
                d_i,
                ray_r_mu_intersects_ground,
                atmosphere.bottom_radius,
                atmosphere.top_radius
            ) * dx;

        // Sample weight (from the trapezoidal rule).
        float weight_i = (i == 0 || i == SAMPLE_COUNT) ? 0.5 : 1.0;
        rayleigh_mie_sum += rayleigh_mie_i * weight_i;
    }
    return rayleigh_mie_sum;
}

void
compute_multiple_scattering_program(
    vec3 frag_coord,
    AtmosphereParameters atmosphere,
    uint scattering_order,
    sampler2D transmittance_texture,
    sampler3D delta_scattering_density_texture,
    writeonly image3D delta_multiple_scattering_texture,
    out ScatterCoord sc,
    out vec4 delta_multiple_scattering
) {
    bool ray_r_mu_intersects_ground;
    sc = scattering_frag_coord_to_rmumusnu(frag_coord, atmosphere, ray_r_mu_intersects_ground);

    delta_multiple_scattering = compute_multiple_scattering(
        sc,
        atmosphere,
        scattering_order,
        transmittance_texture,
        delta_scattering_density_texture,
        ray_r_mu_intersects_ground
    );
    imageStore(
        delta_multiple_scattering_texture,
        ivec3(frag_coord),
        delta_multiple_scattering
    );
}

