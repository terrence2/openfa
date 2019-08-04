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
compute_scattering_density(
    ScatterCoord sc,
    AtmosphereParameters atmosphere,
    uint scattering_order,
    sampler2D transmittance_texture,
    sampler3D delta_rayleigh_scattering_texture,
    sampler3D delta_mie_scattering_texture,
    sampler3D delta_multiple_scattering_texture,
    sampler2D delta_irradiance_texture
) {
    // Compute unit direction vectors for the zenith, the view direction omega and
    // and the sun direction omega_s, such that the cosine of the view-zenith
    // angle is mu, the cosine of the sun-zenith angle is mu_s, and the cosine of
    // the view-sun angle is nu. The goal is to simplify computations below.
    vec3 zenith_direction = vec3(0.0, 0.0, 1.0);
    vec3 omega = vec3(sqrt(1.0 - sc.mu * sc.mu), 0.0, sc.mu);
    float sun_dir_x = omega.x == 0.0 ? 0.0 : (sc.nu - sc.mu * sc.mu_s) / omega.x;
    float sun_dir_y = sqrt(max(1.0 - sun_dir_x * sun_dir_x - sc.mu_s * sc.mu_s, 0.0));
    vec3 omega_s = vec3(sun_dir_x, sun_dir_y, sc.mu_s);

    const int SAMPLE_COUNT = 16;
    const float dphi = PI / float(SAMPLE_COUNT);
    const float dtheta = PI / float(SAMPLE_COUNT);
    vec4 rayleigh_mie = vec4(0.0);

    // Nested loops for the integral over all the incident directions omega_i.
    for (int l = 0; l < SAMPLE_COUNT; ++l) {
        float theta = (float(l) + 0.5) * dtheta;
        float cos_theta = cos(theta);
        float sin_theta = sin(theta);
        bool ray_r_theta_intersects_ground = ray_intersects_ground(vec2(sc.r, cos_theta), atmosphere.bottom_radius);

        // The distance and transmittance to the ground only depend on theta, so we
        // can compute them in the outer loop for efficiency.
        float distance_to_ground = 0.0;
        vec4 transmittance_to_ground = vec4(0.0);
        vec4 ground_albedo = vec4(0.0);
        if (ray_r_theta_intersects_ground) {
            distance_to_ground = distance_to_bottom_atmosphere_boundary(vec2(sc.r, cos_theta), atmosphere.bottom_radius);
            transmittance_to_ground = get_transmittance(
                transmittance_texture,
                sc.r,
                cos_theta,
                distance_to_ground,
                true, // ray_intersects_ground
                atmosphere.bottom_radius,
                atmosphere.top_radius
            );
            ground_albedo = atmosphere.ground_albedo;
        }

        for (int m = 0; m < 2 * SAMPLE_COUNT; ++m) {
            float phi = (float(m) + 0.5) * dphi;
            vec3 omega_i = vec3(cos(phi) * sin_theta, sin(phi) * sin_theta, cos_theta);
            float domega_i = dtheta * dphi * sin(theta);

            // The radiance L_i arriving from direction omega_i after n-1 bounces is
            // the sum of a term given by the precomputed scattering texture for the
            // (n-1)-th order:
            float nu1 = dot(omega_s, omega_i);
            vec4 incident_radiance = get_scattering(
                delta_multiple_scattering_texture,
                ScatterCoord(sc.r, omega_i.z, sc.mu_s, nu1),
                atmosphere.bottom_radius,
                atmosphere.top_radius,
                atmosphere.mu_s_min,
                ray_r_theta_intersects_ground
            );

            // and of the contribution from the light paths with n-1 bounces and whose
            // last bounce is on the ground. This contribution is the product of the
            // transmittance to the ground, the ground albedo, the ground BRDF, and
            // the irradiance received on the ground after n-2 bounces.
            vec3 ground_normal = normalize(zenith_direction * sc.r + omega_i * distance_to_ground);
            vec4 ground_irradiance = get_irradiance(
                delta_irradiance_texture,
                atmosphere.bottom_radius,
                dot(ground_normal, omega_s),
                atmosphere.bottom_radius,
                atmosphere.top_radius
            );
            incident_radiance += transmittance_to_ground * ground_albedo *
                (1.0 / PI) * ground_irradiance;

            // The radiance finally scattered from direction omega_i towards direction
            // -omega is the product of the incident radiance, the scattering
            // coefficient, and the phase function for directions omega and omega_i
            // (all this summed over all particle types, i.e. Rayleigh and Mie).
            float nu2 = dot(omega, omega_i);
            float rayleigh_density = get_profile_density(
                atmosphere.rayleigh_density,
                sc.r - atmosphere.bottom_radius
            );
            float mie_density = get_profile_density(
                atmosphere.mie_density,
                sc.r - atmosphere.bottom_radius
            );
            rayleigh_mie += incident_radiance * (
                atmosphere.rayleigh_scattering_coefficient * rayleigh_density * rayleigh_phase_function(nu2) +
                atmosphere.mie_scattering_coefficient * mie_density * mie_phase_function(atmosphere.mie_phase_function_g, nu2)
            ) * domega_i;
        }
    }

    return rayleigh_mie;
}

void
compute_scattering_density_program(
    vec3 frag_coord,
    AtmosphereParameters atmosphere,
    uint scattering_order,
    sampler2D transmittance_texture,
    sampler3D delta_rayleigh_scattering_texture,
    sampler3D delta_mie_scattering_texture,
    sampler3D delta_multiple_scattering_texture,
    sampler2D delta_irradiance_texture,
    writeonly image3D delta_scattering_density_texture
) {
    bool ray_r_mu_intersects_ground;
    ScatterCoord sc = scattering_frag_coord_to_rmumusnu(frag_coord, atmosphere, ray_r_mu_intersects_ground);

    vec4 rayleigh_mie = compute_scattering_density(
        sc, atmosphere, scattering_order, transmittance_texture,
        delta_rayleigh_scattering_texture, delta_mie_scattering_texture,
        delta_multiple_scattering_texture, delta_irradiance_texture
    );

    imageStore(
        delta_scattering_density_texture,
        ivec3(frag_coord),
        rayleigh_mie
    );
}

