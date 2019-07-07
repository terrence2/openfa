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

#define PI 3.1415926538
#define PI_2 (PI / 2.0)
#define TAU (PI * 2.0)
#define RADIUS 6378.0

const int TRANSMITTANCE_TEXTURE_WIDTH = 256;
const int TRANSMITTANCE_TEXTURE_HEIGHT = 64;

const int SCATTERING_TEXTURE_R_SIZE = 32;
const int SCATTERING_TEXTURE_MU_SIZE = 128;
const int SCATTERING_TEXTURE_MU_S_SIZE = 32;
const int SCATTERING_TEXTURE_NU_SIZE = 8;

const int SCATTERING_TEXTURE_WIDTH = SCATTERING_TEXTURE_NU_SIZE * SCATTERING_TEXTURE_MU_S_SIZE;
const int SCATTERING_TEXTURE_HEIGHT = SCATTERING_TEXTURE_MU_SIZE;
const int SCATTERING_TEXTURE_DEPTH = SCATTERING_TEXTURE_R_SIZE;

const int IRRADIANCE_TEXTURE_WIDTH = 64;
const int IRRADIANCE_TEXTURE_HEIGHT = 16;

struct DensityProfileLayer {
    // Height of this layer, except for the last layer which always
    // extends to the top of the atmosphere region.
    float width; // meters

    // Density in this layer in [0,1) as defined by the following function:
    //   'exp_term' * exp('exp_scale' * h) + 'linear_term' * h + 'constant_term',
    float exp_term;
    float exp_scale; // 1 / meters
    float linear_term; // 1 / meters
    float constant_term;
};

// From low to high.
struct DensityProfile {
    // Note: Arrays are busted in shaderc right now.
    DensityProfileLayer layer0;
    DensityProfileLayer layer1;
};

struct AtmosphereParameters {
    // Energy received into the system from the nearby star.
    vec3 sun_irradiance;

    // The size of the nearby star in radians.
    float sun_angular_radius; // radians

    // From center to subocean.
    float bottom_radius; // meters

    // from center to top of simulated atmosphere.
    float top_radius; // meters

    // The density profile of tiny air molecules.
    DensityProfile rayleigh_density;

    // Per component, at max density.
    vec3 rayleigh_scattering_coefficient;

    // The density profile of aerosols.
    DensityProfile mie_density;

    // Per component, at max density.
    vec3 mie_scattering_coefficient;

    // Per component, at max density.
    vec3 mie_extinction_coefficient;

    // The asymmetry parameter for the Cornette-Shanks phase function for the
    // aerosols.
    float mie_phase_function_g;

    // The density profile of O3.
    DensityProfile absorption_density;

    // Per component, at max density.
    vec3 absorption_extinction_coefficient;

    // The average albedo of the ground, per component.
    vec3 ground_albedo;

    // The cosine of the maximum Sun zenith angle for which atmospheric scattering
    // must be precomputed (for maximum precision, use the smallest Sun zenith
    // angle yielding negligible sky light radiance values. For instance, for the
    // Earth case, 102 degrees is a good choice - yielding mu_s_min = -0.2).
    float mu_s_min;
};

float clamp_cosine(float mu) {
    return clamp(mu, -1.0, 1.0);
}

float clamp_distance(float d) {
    return max(d, 0.0);
}

float safe_sqrt(float a) {
    return sqrt(max(a, 0.0));
}

float get_texture_coord_from_unit_range(float x, int texture_size) {
    return 0.5 / float(texture_size) + x * (1.0 - 1.0 / float(texture_size));
}

float get_unit_range_from_texture_coord(float u, int texture_size) {
    return (u - 0.5 / float(texture_size)) / (1.0 - 1.0 / float(texture_size));
}

bool ray_intersects_ground(
    vec2 rmu,
    float bottom_radius
) {
    float r = rmu.x;
    float mu = rmu.y;

    float f = r * r * (mu * mu - 1.0) + bottom_radius * bottom_radius;
    return mu < 0.0 && f >= 0.0;
}

float distance_to_bottom_atmosphere_boundary(
    vec2 rmu,
    float bottom_radius
) {
    float r = rmu.x;
    float mu = rmu.y;

    // assert(r >= atmosphere.bottom_radius);
    // assert(mu >= -1.0 && mu <= 1.0);
    float discriminant = r * r * (mu * mu - 1.0) + bottom_radius * bottom_radius;
    return clamp_distance(-r * mu - safe_sqrt(discriminant));
}

float distance_to_top_atmosphere_boundary(
    vec2 rmu,
    float top_radius
) {
    float r = rmu.x;
    float mu = rmu.y;

    // assert(r <= top_radius);
    // assert(mu >= -1.0 && mu <= 1.0);
    float discriminant = r * r * (mu * mu - 1.0) + top_radius * top_radius;
    return clamp_distance(-r * mu + safe_sqrt(discriminant));
}

float distance_to_nearest_atmosphere_boundary(
    vec2 rmu,
    float bottom_radius,
    float top_radius,
    bool ray_r_mu_intersects_ground
) {
    if (ray_r_mu_intersects_ground) {
        return distance_to_bottom_atmosphere_boundary(rmu, bottom_radius);
    } else {
        return distance_to_top_atmosphere_boundary(rmu, top_radius);
    }
}

vec2 transmittance_rmu_to_uv(
    vec2 rmu,
    float bottom_radius,
    float top_radius
) {
    float r = rmu.x;
    float mu = rmu.y;

    // assert(r >= bottom_radius && r <= top_radius);
    // assert(mu >= -1.0 && mu <= 1.0);
    // Distance to top atmosphere boundary for a horizontal ray at ground level.
    float H = sqrt(top_radius * top_radius - bottom_radius * bottom_radius);
    // Distance to the horizon.
    float rho = safe_sqrt(r * r - bottom_radius * bottom_radius);
    // Distance to the top atmosphere boundary for the ray (r,mu), and its minimum
    // and maximum values over all mu - obtained for (r,1) and (r,mu_horizon).
    float d = distance_to_top_atmosphere_boundary(rmu, top_radius);
    float d_min = top_radius - r;
    float d_max = rho + H;
    float x_mu = (d - d_min) / (d_max - d_min);
    float x_r = rho / H;
    return vec2(
        get_texture_coord_from_unit_range(x_mu, TRANSMITTANCE_TEXTURE_WIDTH),
        get_texture_coord_from_unit_range(x_r, TRANSMITTANCE_TEXTURE_HEIGHT)
    );
}

vec2 transmittance_uv_to_rmu(
    vec2 uv,
    float bottom_radius,
    float top_radius
) {
    // assert(uv.x >= 0.0 && uv.x <= 1.0);
    // assert(uv.y >= 0.0 && uv.y <= 1.0);
    float x_mu = get_unit_range_from_texture_coord(uv.x, TRANSMITTANCE_TEXTURE_WIDTH);
    float x_r = get_unit_range_from_texture_coord(uv.y, TRANSMITTANCE_TEXTURE_HEIGHT);
    // Distance to top atmosphere boundary for a horizontal ray at ground level.
    float H = sqrt(top_radius * top_radius - bottom_radius * bottom_radius);
    // Distance to the horizon, from which we can compute r:
    float rho = H * x_r;
    float r = sqrt(rho * rho + bottom_radius * bottom_radius);
    // Distance to the top atmosphere boundary for the ray (r,mu), and its minimum
    // and maximum values over all mu - obtained for (r,1) and (r,mu_horizon) -
    // from which we can recover mu:
    float d_min = top_radius - r;
    float d_max = rho + H;
    float d = d_min + x_mu * (d_max - d_min);
    float mu = d == 0.0 ? 1.0 : (H * H - rho * rho - d * d) / (2.0 * r * d);
    mu = clamp_cosine(mu);
    return vec2(r, mu);
}

vec2 irradiance_uv_to_rmus(
    vec2 uv,
    float bottom_radius,
    float top_radius
) {
    // assert(uv.x >= 0.0 && uv.x <= 1.0);
    // assert(uv.y >= 0.0 && uv.y <= 1.0);
    float x_mu_s = get_unit_range_from_texture_coord(uv.x, IRRADIANCE_TEXTURE_WIDTH);
    float x_r = get_unit_range_from_texture_coord(uv.y, IRRADIANCE_TEXTURE_HEIGHT);
    float r = bottom_radius + x_r * (top_radius - bottom_radius);
    float mu_s = clamp_cosine(2.0 * x_mu_s - 1.0);
    return vec2(r, mu_s);
}

vec3 get_transmittance_to_top_atmosphere_boundary(
    vec2 rmu,
    sampler2D transmittance_texture,
    float bottom_radius,
    float top_radius
) {
    // assert(r >= bottom_radius && r <= top_radius);
    vec2 uv = transmittance_rmu_to_uv(rmu, bottom_radius, top_radius);
    return vec3(texture(transmittance_texture, uv));
}

