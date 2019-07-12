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

float clamp_radius(
    float r,
    float bottom_radius,
    float top_radius
) {
  return clamp(r, bottom_radius, top_radius);
}

float get_layer_density(
    DensityProfileLayer layer,
    float altitude
) {
    float density = layer.exp_term * exp(layer.exp_scale * altitude) +
        layer.linear_term * altitude + layer.constant_term;
    return clamp(density, 0.0, 1.0);
}

float get_profile_density(DensityProfile profile, float altitude) {
    return altitude < profile.layer0.width ?
        get_layer_density(profile.layer0, altitude) :
        get_layer_density(profile.layer1, altitude);
}

float compute_optical_length_to_top_atmosphere_boundary(
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

vec3 compute_transmittance_to_top_atmosphere_boundary(
    vec2 rmu,
    AtmosphereParameters atmosphere
) {
    // assert(r >= atmosphere.bottom_radius && r <= atmosphere.top_radius);
    // assert(mu >= -1.0 && mu <= 1.0);
    vec3 rayleigh_depth = atmosphere.rayleigh_scattering_coefficient *
        compute_optical_length_to_top_atmosphere_boundary(
            rmu,
            atmosphere.rayleigh_density,
            atmosphere.bottom_radius,
            atmosphere.top_radius);

    vec3 mie_depth = atmosphere.mie_extinction_coefficient *
        compute_optical_length_to_top_atmosphere_boundary(
            rmu,
            atmosphere.mie_density,
            atmosphere.bottom_radius,
            atmosphere.top_radius);

    vec3 ozone_depth = atmosphere.absorption_extinction_coefficient *
        compute_optical_length_to_top_atmosphere_boundary(
            rmu,
            atmosphere.absorption_density,
            atmosphere.bottom_radius,
            atmosphere.top_radius);

    return exp(-(rayleigh_depth + mie_depth + ozone_depth));
}

void compute_transmittance_program(
    vec2 coord,
    AtmosphereParameters atmosphere,
    writeonly image2D transmittance_lambda
) {
    const vec2 TEXTURE_SIZE = vec2(TRANSMITTANCE_TEXTURE_WIDTH, TRANSMITTANCE_TEXTURE_HEIGHT);
    vec2 uv = coord / TEXTURE_SIZE;
    vec2 rmu = transmittance_uv_to_rmu(
        uv,
        atmosphere.bottom_radius,
        atmosphere.top_radius
    );
    vec3 transmittance = compute_transmittance_to_top_atmosphere_boundary(rmu, atmosphere);
    imageStore(
        transmittance_lambda,
        ivec2(coord),
        vec4(transmittance, 1.0)
    );
}


vec3 compute_direct_irradiance(
    AtmosphereParameters atmosphere,
    sampler2D transmittance_texture,
    vec2 rmus
) {
    float r = rmus.x;
    float mu_s = rmus.y;

    // assert(r >= atmosphere.bottom_radius && r <= atmosphere.top_radius);
    // assert(mu_s >= -1.0 && mu_s <= 1.0);

    float alpha_s = atmosphere.sun_angular_radius;
    // Approximate average of the cosine factor mu_s over the visible fraction of
    // the Sun disc.
    float average_cosine_factor =
        mu_s < -alpha_s
        ? 0.0
        : (mu_s > alpha_s
          ? mu_s
          : (mu_s + alpha_s) * (mu_s + alpha_s) / (4.0 * alpha_s));

    vec3 transmittance = get_transmittance_to_top_atmosphere_boundary(
        rmus,
        transmittance_texture,
        atmosphere.bottom_radius,
        atmosphere.top_radius
    );
    return atmosphere.sun_irradiance * transmittance * average_cosine_factor;
}

void compute_direct_irradiance_program(
    vec2 coord,
    AtmosphereParameters atmosphere,
    sampler2D transmittance_lambda,
    writeonly image2D irradiance_lambda
) {
    const vec2 TEXTURE_SIZE = vec2(IRRADIANCE_TEXTURE_WIDTH, IRRADIANCE_TEXTURE_HEIGHT);
    vec2 uv = coord / TEXTURE_SIZE;
    vec2 rmus = irradiance_uv_to_rmus(
        uv,
        atmosphere.bottom_radius,
        atmosphere.top_radius
    );
    vec3 direct_irradiance = compute_direct_irradiance(
        atmosphere,
        transmittance_lambda,
        rmus
    );
    imageStore(
        irradiance_lambda,
        ivec2(coord),
        vec4(direct_irradiance, 1.0)
    );
}


//vec4 GetScatteringTextureUvwzFromRMuMuSNu(IN(AtmosphereParameters) atmosphere,
//Length r, Number mu, Number mu_s, Number nu,
//bool ray_r_mu_intersects_ground) {
//assert(r >= atmosphere.bottom_radius && r <= atmosphere.top_radius);
//assert(mu >= -1.0 && mu <= 1.0);
//assert(mu_s >= -1.0 && mu_s <= 1.0);
//assert(nu >= -1.0 && nu <= 1.0);
//
//// Distance to top atmosphere boundary for a horizontal ray at ground level.
//Length H = sqrt(atmosphere.top_radius * atmosphere.top_radius -
//atmosphere.bottom_radius * atmosphere.bottom_radius);
//// Distance to the horizon.
//Length rho =
//SafeSqrt(r * r - atmosphere.bottom_radius * atmosphere.bottom_radius);
//Number u_r = GetTextureCoordFromUnitRange(rho / H, SCATTERING_TEXTURE_R_SIZE);
//
//// Discriminant of the quadratic equation for the intersections of the ray
//// (r,mu) with the ground (see RayIntersectsGround).
//Length r_mu = r * mu;
//Area discriminant =
//r_mu * r_mu - r * r + atmosphere.bottom_radius * atmosphere.bottom_radius;
//Number u_mu;
//if (ray_r_mu_intersects_ground) {
//    // Distance to the ground for the ray (r,mu), and its minimum and maximum
//    // values over all mu - obtained for (r,-1) and (r,mu_horizon).
//    Length d = -r_mu - SafeSqrt(discriminant);
//    Length d_min = r - atmosphere.bottom_radius;
//    Length d_max = rho;
//    u_mu = 0.5 - 0.5 * GetTextureCoordFromUnitRange(d_max == d_min ? 0.0 :
//    (d - d_min) / (d_max - d_min), SCATTERING_TEXTURE_MU_SIZE / 2);
//} else {
//    // Distance to the top atmosphere boundary for the ray (r,mu), and its
//    // minimum and maximum values over all mu - obtained for (r,1) and
//    // (r,mu_horizon).
//    Length d = -r_mu + SafeSqrt(discriminant + H * H);
//    Length d_min = atmosphere.top_radius - r;
//    Length d_max = rho + H;
//    u_mu = 0.5 + 0.5 * GetTextureCoordFromUnitRange(
//    (d - d_min) / (d_max - d_min), SCATTERING_TEXTURE_MU_SIZE / 2);
//}
//
//Length d = DistanceToTopAtmosphereBoundary(
//atmosphere, atmosphere.bottom_radius, mu_s);
//Length d_min = atmosphere.top_radius - atmosphere.bottom_radius;
//Length d_max = H;
//Number a = (d - d_min) / (d_max - d_min);
//Number A =
//-2.0 * atmosphere.mu_s_min * atmosphere.bottom_radius / (d_max - d_min);
//Number u_mu_s = GetTextureCoordFromUnitRange(
//max(1.0 - a / A, 0.0) / (1.0 + a), SCATTERING_TEXTURE_MU_S_SIZE);
//
//Number u_nu = (nu + 1.0) / 2.0;
//return vec4(u_nu, u_mu_s, u_mu, u_r);
//}

/*
<p>The inverse mapping follows immediately:
*/

struct ScatterCoord {
    float r;
    float mu;
    float mu_s;
    float nu;
};

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

vec3 get_transmittance(
    sampler2D transmittance_texture,
    float r,
    float mu,
    float d,
    bool ray_r_mu_intersects_ground,
    float bottom_radius,
    float top_radius
) {
    // assert(r >= bottom_radius && r <= top_radius);
    // assert(mu >= -1.0 && mu <= 1.0);
    // assert(d >= 0.0 * m);

    float r_d = clamp_radius(
        sqrt(d * d + 2.0 * r * mu * d + r * r),
        bottom_radius,
        top_radius
    );
    float mu_d = clamp_cosine((r * mu + d) / r_d);

  if (ray_r_mu_intersects_ground) {
    return min(
        get_transmittance_to_top_atmosphere_boundary(
            vec2(r_d, -mu_d), transmittance_texture, bottom_radius, top_radius) /
        get_transmittance_to_top_atmosphere_boundary(
            vec2(r, -mu), transmittance_texture, bottom_radius, top_radius),
        vec3(1.0));
  } else {
    return min(
        get_transmittance_to_top_atmosphere_boundary(
            vec2(r, mu), transmittance_texture, bottom_radius, top_radius) /
        get_transmittance_to_top_atmosphere_boundary(
            vec2(r_d, mu_d), transmittance_texture, bottom_radius, top_radius),
        vec3(1.0));
  }
}

vec3 get_transmittance_to_sun(
    AtmosphereParameters atmosphere,
    sampler2D transmittance_texture,
    float r,
    float mu_s
) {
    float sin_theta_h = atmosphere.bottom_radius / r;
    float cos_theta_h = -sqrt(max(1.0 - sin_theta_h * sin_theta_h, 0.0));
    vec3 base = get_transmittance_to_top_atmosphere_boundary(
        vec2(r, mu_s),
        transmittance_texture,
        atmosphere.bottom_radius,
        atmosphere.top_radius
    );
    return  base * smoothstep(-sin_theta_h * atmosphere.sun_angular_radius,
                 sin_theta_h * atmosphere.sun_angular_radius,
                 mu_s - cos_theta_h);
}

void compute_single_scattering_integrand(
    AtmosphereParameters atmosphere,
    sampler2D transmittance_texture,
    ScatterCoord coord,
    float d,
    bool ray_r_mu_intersects_ground,
    out vec3 rayleigh,
    out vec3 mie
) {
    float altitude = sqrt(d * d + 2.0 * coord.r * coord.mu * d + coord.r * coord.r);
    float r_d = clamp_radius(altitude, atmosphere.bottom_radius, atmosphere.top_radius);
    float mu_s_d = clamp_cosine((coord.r * coord.mu_s + d * coord.nu) / r_d);
    vec3 transmittance_to_sun = get_transmittance_to_sun(atmosphere, transmittance_texture, r_d, mu_s_d);
    vec3 base_transmittance = get_transmittance(transmittance_texture, coord.r, coord.mu, d,
        ray_r_mu_intersects_ground, atmosphere.bottom_radius, atmosphere.top_radius);
    vec3 transmittance = base_transmittance * transmittance_to_sun;
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
    out vec3 rayleigh,
    out vec3 mie
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
    vec3 rayleigh_sum = vec3(0.0);
    vec3 mie_sum = vec3(0.0);
    for (int i = 0; i <= SAMPLE_COUNT; ++i) {
        float d_i = float(i) * dx;
        // The Rayleigh and Mie single scattering at the current sample point.
        vec3 rayleigh_i;
        vec3 mie_i;
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
    //rayleigh = rayleigh_sum * path_length / float(SAMPLE_COUNT) * atmosphere.rayleigh_scattering_coefficient;
    mie = mie_sum * dx * atmosphere.sun_irradiance * atmosphere.mie_scattering_coefficient;
}

void compute_single_scattering_program(
    vec3 sample_coord,
    mat3 rad_to_lum,
    AtmosphereParameters atmosphere,
    sampler2D transmittance_lambda,
    restrict writeonly image3D rayleigh_lambda,
    restrict writeonly image3D mie_lambda,
    out ivec3 frag_coord,
    out vec4 scattering,
    out vec4 single_mie_scattering
) {
    bool ray_r_mu_intersects_ground;
    ScatterCoord coord = scattering_frag_coord_to_rmumusnu(
        sample_coord,
        atmosphere,
        ray_r_mu_intersects_ground
    );

    vec3 rayleigh;
    vec3 mie;
    compute_single_scattering(
        atmosphere,
        transmittance_lambda,
        coord,
        ray_r_mu_intersects_ground,
        rayleigh,
        mie
    );

    frag_coord = ivec3(sample_coord);
    imageStore(
        rayleigh_lambda,
        frag_coord,
        vec4(rayleigh, 1.0)
    );
    imageStore(
        mie_lambda,
        frag_coord,
        vec4(mie, 1.0)
    );

    scattering = vec4(rad_to_lum * rayleigh, rad_to_lum * mie.r);
    single_mie_scattering = vec4(rad_to_lum * mie, 1);

    /*
    */
}
