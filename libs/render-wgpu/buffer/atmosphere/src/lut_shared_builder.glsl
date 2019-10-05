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

float
get_layer_density(DensityProfileLayer layer, float altitude) {
    float density = layer.exp_term * exp(layer.exp_scale * altitude) +
        layer.linear_term * altitude + layer.constant_term;
    return clamp(density, 0.0, 1.0);
}

float
get_profile_density(DensityProfile profile, float altitude) {
    return altitude < profile.layer0.width
        ? get_layer_density(profile.layer0, altitude)
        : get_layer_density(profile.layer1, altitude);
}

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

vec4
scattering_frag_coord_to_uvwz(vec3 frag_coord) {
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

ScatterCoord
scattering_frag_coord_to_rmumusnu(
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

vec4
get_scattering(
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

vec4
get_best_scattering(
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

