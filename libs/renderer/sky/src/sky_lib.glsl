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

// Derived in large part from the excellent work at:
//     https://ebruneton.github.io/precomputed_atmospheric_scattering/
// Which is:
//     Copyright (c) 2017 Eric Bruneton
// All errors and omissions below were introduced in transcription
// to Rust and are not reflective of the high quality fo the
// original work in any way.

#define PI 3.1415926538
#define PI_2 (PI / 2.0)
#define TAU (PI * 2.0)
#define RADIUS 6378.0

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
   DensityProfileLayer layers[2];
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
    vec3 rayleigh_scattering_coefficent;

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

struct FrameParameters {
    // A unit vector pointing towards the sun.
    vec3 sun_direction;
};

void compute_ground_radiance(vec3 camera, vec3 view, float radius, out vec3 radiance, out float alpha) {
    // The planet center is always at 0.
    vec3 p = camera - vec3(0, 0, 0);
    float p_dot_v = dot(p, view);
    float p_dot_p = dot(p, p);
    float dist2 = p_dot_p - p_dot_v * p_dot_v;
    float t0 = -p_dot_v - sqrt(radius * radius - dist2);

    alpha = 0.0;
    radiance = vec3(0);
    if (t0 > 0.0) {
        vec3 intersect = camera + view * t0;
        vec3 normal = normalize(intersect);

        // Get sun and sky irradiance at the ground point and modulate
        // by the ground albedo.
        vec3 sky_irradiance;
        vec3 sun_irradiance;
        // GetSunAndSkyIrradiance(
        //        intersect, normal, sun_direction, sky_irradiance, sun_irradiance);
        vec3 ground_radiance = vec3(0.5); //pc.ground_albedo;

        // Fade the radiance on the ground by the amount of atmosphere
        // between us and that point and brighten by ambient in-scatter
        // to the camera on that path.
        vec3 transmittance = vec3(1);
        vec3 in_scatter = vec3(0.1);
        //    GetSkyRadianceToPoint(camera,
        //        intersect, shadow_length, sun_direction, transmittance);

        radiance = ground_radiance * transmittance + in_scatter;
        alpha = 1.0;
    }
}


//
//
//  // Compute the radiance reflected by the ground, if the ray intersects it.
//  float ground_alpha = 0.0;
//  vec3 ground_radiance = vec3(0.0);
//  if (distance_to_intersection > 0.0) {
//    vec3 point = camera + view_direction * distance_to_intersection;
//    vec3 normal = normalize(point - earth_center);
//
//    // Compute the radiance reflected by the ground.
//    vec3 sky_irradiance;
//    vec3 sun_irradiance = GetSunAndSkyIrradiance(
//        point - earth_center, normal, sun_direction, sky_irradiance);
//    ground_radiance = kGroundAlbedo * (1.0 / PI) * (
//        sun_irradiance * GetSunVisibility(point, sun_direction) +
//        sky_irradiance * GetSkyVisibility(point));
//
//    float shadow_length =
//        max(0.0, min(shadow_out, distance_to_intersection) - shadow_in) *
//        lightshaft_fadein_hack;
//    vec3 transmittance;
//    vec3 in_scatter = GetSkyRadianceToPoint(camera - earth_center,
//        point - earth_center, shadow_length, sun_direction, transmittance);
//    ground_radiance = ground_radiance * transmittance + in_scatter;
//    ground_alpha = 1.0;
//  }

vec3 get_sun_radiance(vec3 sun_irradiance, float sun_angular_radius) {
    return sun_irradiance / (PI * sun_angular_radius * sun_angular_radius);
}

void compute_sky_radiance(vec3 camera, vec3 view, vec3 sun_direction, vec3 sun_irradiance, float sun_angular_radius, out vec3 radiance) {
//    vec3 transmittance;
//    GetSkyRadiance(camera, view, /*shadow_length,*/ sun_direction, transmittance, radiance);

    radiance = vec3(0);
    if (dot(view, sun_direction) > cos(sun_angular_radius)) {
        vec3 transmittance = vec3(1);
        radiance = transmittance * get_sun_radiance(sun_irradiance, sun_angular_radius);
    }
}
