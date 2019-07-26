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

#define PI 3.14159265358979323846
#define PI_2 (PI / 2.0)
#define TAU (PI * 2.0)
#define RADIUS 6378.0

// The conversion factor between watts and lumens.
const float MAX_LUMINOUS_EFFICACY = 683.0;

const vec3 SKY_SPECTRAL_RADIANCE_TO_LUMINANCE = vec3(
    MAX_LUMINOUS_EFFICACY,
    MAX_LUMINOUS_EFFICACY,
    MAX_LUMINOUS_EFFICACY
);

const vec3 SUN_SPECTRAL_RADIANCE_TO_LUMINANCE = vec3(
    MAX_LUMINOUS_EFFICACY,
    MAX_LUMINOUS_EFFICACY,
    MAX_LUMINOUS_EFFICACY
);

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
    vec4 sun_irradiance;

    // The size of the nearby star in radians.
    float sun_angular_radius; // radians

    // Conversion between the solar irradiance above and our desired sRGB luminance output.
    vec3 sun_spectral_radiance_to_luminance;

    // Conversion between the irradiance stored in our LUT and sRGB luminance outputs.
    // Note that this is where we re-add the luminous efficacy constant that we factored
    // out of the precomputations to keep the numbers closer to 0 for precision.
    vec3 sky_spectral_radiance_to_luminance;

    // From center to subocean.
    float bottom_radius; // meters

    // from center to top of simulated atmosphere.
    float top_radius; // meters

    // The density profile of tiny air molecules.
    DensityProfile rayleigh_density;

    // Per component, at max density.
    vec4 rayleigh_scattering_coefficient;

    // The density profile of aerosols.
    DensityProfile mie_density;

    // Per component, at max density.
    vec4 mie_scattering_coefficient;

    // Per component, at max density.
    vec4 mie_extinction_coefficient;

    // The asymmetry parameter for the Cornette-Shanks phase function for the
    // aerosols.
    float mie_phase_function_g;

    // The density profile of O3.
    DensityProfile absorption_density;

    // Per component, at max density.
    vec4 absorption_extinction_coefficient;

    // The average albedo of the ground, per component.
    vec4 ground_albedo;

    // The cosine of the maximum Sun zenith angle for which atmospheric scattering
    // must be precomputed (for maximum precision, use the smallest Sun zenith
    // angle yielding negligible sky light radiance values. For instance, for the
    // Earth case, 102 degrees is a good choice - yielding mu_s_min = -0.2).
    float mu_s_min;
};

float clamp_radius(
    float r,
    float bottom_radius,
    float top_radius
) {
    return clamp(r, bottom_radius, top_radius);
}

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

vec4
get_transmittance_to_top_atmosphere_boundary(
    vec2 rmu,
    sampler2D transmittance_texture,
    float bottom_radius,
    float top_radius
) {
    // assert(r >= bottom_radius && r <= top_radius);
    vec2 uv = transmittance_rmu_to_uv(rmu, bottom_radius, top_radius);
    return texture(transmittance_texture, uv);
}

vec4 get_transmittance_to_sun(
    sampler2D transmittance_texture,
    float r,
    float mu_s,
    float bottom_radius,
    float top_radius,
    float sun_angular_radius
) {
    float sin_theta_h = bottom_radius / r;
    float cos_theta_h = -sqrt(max(1.0 - sin_theta_h * sin_theta_h, 0.0));
    vec4 base = get_transmittance_to_top_atmosphere_boundary(
        vec2(r, mu_s),
        transmittance_texture,
        bottom_radius,
        top_radius
    );
    return  base * smoothstep(
        -sin_theta_h * sun_angular_radius,
        sin_theta_h * sun_angular_radius,
        mu_s - cos_theta_h);
}

vec4 get_transmittance(
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
            vec4(1.0));
    } else {
        return min(
            get_transmittance_to_top_atmosphere_boundary(
                vec2(r, mu), transmittance_texture, bottom_radius, top_radius) /
            get_transmittance_to_top_atmosphere_boundary(
                vec2(r_d, mu_d), transmittance_texture, bottom_radius, top_radius),
            vec4(1.0));
    }
}

// In order to precompute the ground irradiance in a texture we need a mapping
// from the ground irradiance parameters to texture coordinates. Since we
// precompute the ground irradiance only for horizontal surfaces, this irradiance
// depends only on $r$ and $\mu_s$, so we need a mapping from $(r,\mu_s)$ to
// $(u,v)$ texture coordinates. The simplest, affine mapping is sufficient here,
// because the ground irradiance function is very smooth:
vec2 irradiance_rmus_to_uv(
    vec2 rmus,
    float bottom_radius,
    float top_radius
) {
    float r = rmus.x;
    float mu_s = rmus.y;
    float x_r = (r - bottom_radius) / (top_radius - bottom_radius);
    float x_mu_s = mu_s * 0.5 + 0.5;
    return vec2(
        get_texture_coord_from_unit_range(x_mu_s, IRRADIANCE_TEXTURE_WIDTH),
        get_texture_coord_from_unit_range(x_r, IRRADIANCE_TEXTURE_HEIGHT)
    );
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

vec4 get_irradiance(
    sampler2D irradiance_texture,
    vec2 rmus,
    float bottom_radius,
    float top_radius
) {
    vec2 uv = irradiance_rmus_to_uv(rmus, bottom_radius, top_radius);
    return texture(irradiance_texture, uv);
}

struct ScatterCoord {
    float r;
    float mu;
    float mu_s;
    float nu;
};

vec4 scattering_rmumusnu_to_uvwz(
    ScatterCoord sc,
    float bottom_radius,
    float top_radius,
    float mu_s_min,
    bool ray_r_mu_intersects_ground
) {
    // Distance to top atmosphere boundary for a horizontal ray at ground level.
    float H = sqrt(top_radius * top_radius - bottom_radius * bottom_radius);

    // Distance to the horizon.
    float rho = safe_sqrt(sc.r * sc.r - bottom_radius * bottom_radius);
    float u_r = get_texture_coord_from_unit_range(rho / H, SCATTERING_TEXTURE_R_SIZE);

    // Discriminant of the quadratic equation for the intersections of the ray
    // (r,mu) with the ground (see RayIntersectsGround).
    float r_mu = sc.r * sc.mu;
    float discriminant = r_mu * r_mu - sc.r * sc.r + bottom_radius * bottom_radius;
    float u_mu;
    if (ray_r_mu_intersects_ground) {
        // Distance to the ground for the ray (r,mu), and its minimum and maximum
        // values over all mu - obtained for (r,-1) and (r,mu_horizon).
        float d = -r_mu - safe_sqrt(discriminant);
        float d_min = sc.r - bottom_radius;
        float d_max = rho;
        u_mu = 0.5 - 0.5 * get_texture_coord_from_unit_range(d_max == d_min ? 0.0 :
        (d - d_min) / (d_max - d_min), SCATTERING_TEXTURE_MU_SIZE / 2);
    } else {
        // Distance to the top atmosphere boundary for the ray (r,mu), and its
        // minimum and maximum values over all mu - obtained for (r,1) and
        // (r,mu_horizon).
        float d = -r_mu + safe_sqrt(discriminant + H * H);
        float d_min = top_radius - sc.r;
        float d_max = rho + H;
        u_mu = 0.5 + 0.5 * get_texture_coord_from_unit_range(
        (d - d_min) / (d_max - d_min), SCATTERING_TEXTURE_MU_SIZE / 2);
    }

    float d = distance_to_top_atmosphere_boundary(vec2(bottom_radius, sc.mu_s), top_radius);
    float d_min = top_radius - bottom_radius;
    float d_max = H;
    float a = (d - d_min) / (d_max - d_min);
    float A = -2.0 * mu_s_min * bottom_radius / (d_max - d_min);
    float u_mu_s = get_texture_coord_from_unit_range(
    max(1.0 - a / A, 0.0) / (1.0 + a), SCATTERING_TEXTURE_MU_S_SIZE);

    float u_nu = (sc.nu + 1.0) / 2.0;
    return vec4(u_nu, u_mu_s, u_mu, u_r);
}

// Note that we added the solar irradiance and the scattering coefficient terms
// that we omitted in <code>ComputeSingleScatteringIntegrand</code>, but not the
// phase function terms - they are added at <a href="#rendering">render time</a>
// for better angular precision. We provide them here for completeness:
float rayleigh_phase_function(float nu) {
    return 0.1;
    float k = 3.0 / (16.0 * PI);
    return k * (1.0 + nu * nu);
}

float mie_phase_function(float g, float nu) {
    return 0.1;
    float k = 3.0 / (8.0 * PI) * (1.0 - g * g) / (2.0 + g * g);
    return k * (1.0 + nu * nu) / pow(1.0 + g * g - 2.0 * g * nu, 1.5);
}

