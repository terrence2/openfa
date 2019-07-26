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

ScatterCoord scattering_uvwz_to_rmumusnu(
    vec4 uvwz,
    float mu_s_min,
    float bottom_radius,
    float top_radius,
    out bool ray_r_mu_intersects_ground
) {
    //assert(uvwz.x >= 0.0 && uvwz.x <= 1.0);
    //assert(uvwz.y >= 0.0 && uvwz.y <= 1.0);
    //assert(uvwz.z >= 0.0 && uvwz.z <= 1.0);
    //assert(uvwz.w >= 0.0 && uvwz.w <= 1.0);

    // Distance to top atmosphere boundary for a horizontal ray at ground level.
    float H = sqrt(top_radius * top_radius - bottom_radius * bottom_radius);
    // Distance to the horizon.
    float rho = H * get_unit_range_from_texture_coord(uvwz.w, SCATTERING_TEXTURE_R_SIZE);
    float r = sqrt(rho * rho + bottom_radius * bottom_radius);

    float mu;
    if (uvwz.z < 0.5) {
        // Distance to the ground for the ray (r,mu), and its minimum and maximum
        // values over all mu - obtained for (r,-1) and (r,mu_horizon) - from which
        // we can recover mu:
        float d_min = r - bottom_radius;
        float d_max = rho;
        float d = d_min + (d_max - d_min) * get_unit_range_from_texture_coord(
            1.0 - 2.0 * uvwz.z, SCATTERING_TEXTURE_MU_SIZE / 2);
        mu = d == 0.0 ? -1.0 : clamp_cosine(-(rho * rho + d * d) / (2.0 * r * d));
        ray_r_mu_intersects_ground = true;
    } else {
        // Distance to the top atmosphere boundary for the ray (r,mu), and its
        // minimum and maximum values over all mu - obtained for (r,1) and
        // (r,mu_horizon) - from which we can recover mu:
        float d_min = top_radius - r;
        float d_max = rho + H;
        float d = d_min + (d_max - d_min) * get_unit_range_from_texture_coord(
            2.0 * uvwz.z - 1.0, SCATTERING_TEXTURE_MU_SIZE / 2);
        mu = d == 0.0 ? 1.0 : clamp_cosine((H * H - rho * rho - d * d) / (2.0 * r * d));
        ray_r_mu_intersects_ground = false;
    }

    float x_mu_s = get_unit_range_from_texture_coord(uvwz.y, SCATTERING_TEXTURE_MU_S_SIZE);
    float d_min = top_radius - bottom_radius;
    float d_max = H;
    float A = -2.0 * mu_s_min * bottom_radius / (d_max - d_min);
    float a = (A - x_mu_s * A) / (1.0 + x_mu_s * A);
    float d = d_min + min(a, A) * (d_max - d_min);
    float mu_s = d == 0.0 ? 1.0 : clamp_cosine((H * H - d * d) / (2.0 * bottom_radius * d));

    float nu = clamp_cosine(uvwz.x * 2.0 - 1.0);

    return ScatterCoord(r, mu, mu_s, nu);
}

vec4 scattering_frag_coord_to_uvwz(vec3 frag_coord) {
    const vec4 SCATTERING_TEXTURE_SIZE = vec4(
        SCATTERING_TEXTURE_NU_SIZE - 1,
        SCATTERING_TEXTURE_MU_S_SIZE,
        SCATTERING_TEXTURE_MU_SIZE,
        SCATTERING_TEXTURE_R_SIZE
    );
    float frag_coord_nu = floor(frag_coord.x / float(SCATTERING_TEXTURE_MU_S_SIZE));
    float frag_coord_mu_s = mod(frag_coord.x, float(SCATTERING_TEXTURE_MU_S_SIZE));
    vec4 frag4 = vec4(frag_coord_nu, frag_coord_mu_s, frag_coord.y, frag_coord.z);
    return frag4 / SCATTERING_TEXTURE_SIZE;
}

ScatterCoord scattering_frag_coord_to_rmumusnu(
    vec3 frag_coord,
    AtmosphereParameters atmosphere,
    out bool ray_r_mu_intersects_ground
) {
    vec4 uvwz = scattering_frag_coord_to_uvwz(frag_coord);
    ScatterCoord coord = scattering_uvwz_to_rmumusnu(
        uvwz,
        atmosphere.mu_s_min,
        atmosphere.bottom_radius,
        atmosphere.top_radius,
        ray_r_mu_intersects_ground
    );

    // Clamp nu to its valid range of values, given mu and mu_s.
    float mu = coord.mu;
    float mu_s = coord.mu_s;
    float min_nu = mu * mu_s - sqrt((1.0 - mu * mu) * (1.0 - mu_s * mu_s));
    float max_nu = mu * mu_s + sqrt((1.0 - mu * mu) * (1.0 - mu_s * mu_s));
    coord.nu = clamp(coord.nu, min_nu, max_nu);

    return coord;
}

vec4 get_scattering(
    sampler3D scattering_texture,
    ScatterCoord sc,
    float atmosphere_bottom_radius,
    float atmosphere_top_radius,
    float atmosphere_mu_s_min,
    bool ray_r_mu_intersects_ground
) {
    vec4 uvwz = scattering_rmumusnu_to_uvwz(
        sc,
        atmosphere_bottom_radius,
        atmosphere_top_radius,
        atmosphere_mu_s_min,
        ray_r_mu_intersects_ground);
    float tex_coord_x = uvwz.x * float(SCATTERING_TEXTURE_NU_SIZE - 1);
    float tex_x = floor(tex_coord_x);
    float lerp = tex_coord_x - tex_x;
    vec3 uvw0 = vec3((tex_x + uvwz.y) / float(SCATTERING_TEXTURE_NU_SIZE), uvwz.z, uvwz.w);
    vec3 uvw1 = vec3((tex_x + 1.0 + uvwz.y) / float(SCATTERING_TEXTURE_NU_SIZE), uvwz.z, uvwz.w);
    return texture(scattering_texture, uvw0) * (1.0 - lerp) +
        texture(scattering_texture, uvw1) * lerp;
}

vec4 get_best_scattering(
    sampler3D delta_rayleigh_scattering_texture,
    sampler3D delta_mie_scattering_texture,
    sampler3D delta_multiple_scattering_texture,
    ScatterCoord sc,
    float atmosphere_bottom_radius,
    float atmosphere_top_radius,
    float atmosphere_mu_s_min,
    float atmosphere_mie_phase_function_g,
    bool ray_r_mu_intersects_ground,
    uint scattering_order
) {
    if (scattering_order == 1) {
        vec4 rayleigh = get_scattering(
            delta_rayleigh_scattering_texture,
            sc,
            atmosphere_bottom_radius,
            atmosphere_top_radius,
            atmosphere_mu_s_min,
            ray_r_mu_intersects_ground
        );
        vec4 mie = get_scattering(
            delta_mie_scattering_texture,
            sc,
            atmosphere_bottom_radius,
            atmosphere_top_radius,
            atmosphere_mu_s_min,
            ray_r_mu_intersects_ground
        );
        return rayleigh * rayleigh_phase_function(sc.nu) +
            mie * mie_phase_function(atmosphere_mie_phase_function_g, sc.nu);
    } else {
        return get_scattering(
            delta_multiple_scattering_texture,
            sc,
            atmosphere_bottom_radius,
            atmosphere_top_radius,
            atmosphere_mu_s_min,
            ray_r_mu_intersects_ground
        );
    }
}

void compute_single_scattering_integrand(
    AtmosphereParameters atmosphere,
    sampler2D transmittance_texture,
    ScatterCoord coord,
    float d,
    bool ray_r_mu_intersects_ground,
    out vec4 rayleigh,
    out vec4 mie
) {
    float altitude = sqrt(d * d + 2.0 * coord.r * coord.mu * d + coord.r * coord.r);
    float r_d = clamp_radius(altitude, atmosphere.bottom_radius, atmosphere.top_radius);
    float mu_s_d = clamp_cosine((coord.r * coord.mu_s + d * coord.nu) / r_d);
    vec4 base_transmittance = get_transmittance(transmittance_texture, coord.r, coord.mu, d,
        ray_r_mu_intersects_ground, atmosphere.bottom_radius, atmosphere.top_radius);
    vec4 transmittance_to_sun = get_transmittance_to_sun(
        transmittance_texture,
        r_d,
        mu_s_d,
        atmosphere.bottom_radius,
        atmosphere.top_radius,
        atmosphere.sun_angular_radius);
    vec4 transmittance = base_transmittance * transmittance_to_sun;
    rayleigh = transmittance * get_profile_density(
        atmosphere.rayleigh_density, r_d - atmosphere.bottom_radius);
    mie = transmittance * get_profile_density(
        atmosphere.mie_density, r_d - atmosphere.bottom_radius);
}

void compute_single_scattering(
    AtmosphereParameters atmosphere,
    sampler2D transmittance_texture,
    ScatterCoord coord,
    bool ray_r_mu_intersects_ground,
    out vec4 rayleigh,
    out vec4 mie
) {
    // assert(coord.r >= atmosphere.bottom_radius && coord.r <= atmosphere.top_radius);
    // assert(coord.mu >= -1.0 && coord.mu <= 1.0);
    // assert(coord.mu_s >= -1.0 && coord.mu_s <= 1.0);
    // assert(coord.nu >= -1.0 && coord.nu <= 1.0);

    // Number of intervals for the numerical integration.
    const int SAMPLE_COUNT = 50;
    // The integration step, i.e. the length of each integration interval.
    float path_length = distance_to_nearest_atmosphere_boundary(
        vec2(coord.r, coord.mu),
        atmosphere.bottom_radius,
        atmosphere.top_radius,
        ray_r_mu_intersects_ground
    );
    float dx =  path_length / float(SAMPLE_COUNT);
    // Integration loop.
    vec4 rayleigh_sum = vec4(0.0);
    vec4 mie_sum = vec4(0.0);
    for (int i = 0; i <= SAMPLE_COUNT; ++i) {
        float d_i = float(i) * dx;
        // The Rayleigh and Mie single scattering at the current sample point.
        vec4 rayleigh_i;
        vec4 mie_i;
        compute_single_scattering_integrand(
            atmosphere,
            transmittance_texture,
            coord,
            d_i,
            ray_r_mu_intersects_ground,
            rayleigh_i,
            mie_i
        );
        // Sample weight (from the trapezoidal rule).
        float weight_i = (i == 0 || i == SAMPLE_COUNT) ? 0.5 : 1.0;
        rayleigh_sum += rayleigh_i * weight_i;
        mie_sum += mie_i * weight_i;
    }
    rayleigh = rayleigh_sum * dx * atmosphere.sun_irradiance * atmosphere.rayleigh_scattering_coefficient;
    mie = mie_sum * dx * atmosphere.sun_irradiance * atmosphere.mie_scattering_coefficient;
}

void compute_single_scattering_program(
    vec3 sample_coord,
    mat4 rad_to_lum,
    AtmosphereParameters atmosphere,
    sampler2D transmittance_texture,
    restrict writeonly image3D delta_rayleigh_scattering_texture,
    restrict writeonly image3D delta_mie_scattering_texture,
    out vec3 scattering,
    out vec3 single_mie_scattering
) {
    bool ray_r_mu_intersects_ground;
    ScatterCoord coord = scattering_frag_coord_to_rmumusnu(
        sample_coord,
        atmosphere,
        ray_r_mu_intersects_ground
    );

    vec4 delta_rayleigh;
    vec4 delta_mie;
    compute_single_scattering(
        atmosphere,
        transmittance_texture,
        coord,
        ray_r_mu_intersects_ground,
        delta_rayleigh,
        delta_mie
    );

    ivec3 frag_coord = ivec3(sample_coord);
    imageStore(
        delta_rayleigh_scattering_texture,
        frag_coord,
        delta_rayleigh
    );
    imageStore(
        delta_mie_scattering_texture,
        frag_coord,
        delta_mie
    );

    scattering = vec3(rad_to_lum * delta_rayleigh);
    single_mie_scattering = vec3(rad_to_lum * delta_mie);
}

vec4 compute_scattering_density(
    ScatterCoord sc,
    AtmosphereParameters atmosphere,
    mat3 rad_to_lum,
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
            vec4 incident_radiance = get_best_scattering(
                delta_rayleigh_scattering_texture,
                delta_mie_scattering_texture,
                delta_multiple_scattering_texture,
                ScatterCoord(sc.r, omega_i.z, sc.mu_s, nu1),
                atmosphere.bottom_radius,
                atmosphere.top_radius,
                atmosphere.mu_s_min,
                atmosphere.mie_phase_function_g,
                ray_r_theta_intersects_ground,
                scattering_order - 1
            );

            // and of the contribution from the light paths with n-1 bounces and whose
            // last bounce is on the ground. This contribution is the product of the
            // transmittance to the ground, the ground albedo, the ground BRDF, and
            // the irradiance received on the ground after n-2 bounces.
            vec3 ground_normal = normalize(zenith_direction * sc.r + omega_i * distance_to_ground);
            vec4 ground_irradiance = get_irradiance(
                delta_irradiance_texture,
                vec2(
                    atmosphere.bottom_radius,
                    dot(ground_normal, omega_s)
                ),
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

void compute_scattering_density_program(
    vec3 frag_coord,
    AtmosphereParameters atmosphere,
    mat3 rad_to_lum,
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
        sc, atmosphere, rad_to_lum, scattering_order, transmittance_texture,
        delta_rayleigh_scattering_texture, delta_mie_scattering_texture,
        delta_multiple_scattering_texture, delta_irradiance_texture
    );

    imageStore(
        delta_scattering_density_texture,
        ivec3(frag_coord),
        rayleigh_mie
    );
}

vec4 compute_multiple_scattering(
    ScatterCoord sc,
    AtmosphereParameters atmosphere,
    mat3 rad_to_lum,
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

void compute_multiple_scattering_program(
    vec3 frag_coord,
    AtmosphereParameters atmosphere,
    mat3 rad_to_lum,
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
        rad_to_lum,
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
