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

#include "header.glsl"

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

void get_sky_radiance(
    vec3 camera,
    vec3 view,
    vec3 sun_direction,
    float bottom_radius,
    float top_radius,
    sampler2D transmittance_texture,
    out vec3 transmittance,
    out vec3 radiance
) {
    transmittance = vec3(1);
    radiance = vec3(0, 0, 1);

    // Compute the distance to the top atmosphere boundary along the view ray,
    // assuming the viewer is in space (or NaN if the view ray does not intersect
    // the atmosphere).
    float r = length(camera);
    float rmu = dot(camera, view);
    float t0 = -rmu - sqrt(rmu * rmu - r * r + top_radius * top_radius);
    if (t0 > 0.0) {
        // If the viewer is in space and the view ray intersects the atmosphere, move
        // the viewer to the top atmosphere boundary (along the view ray):
        camera = camera + view * t0;
        r = top_radius;
        rmu += t0;
    } else if (r > top_radius) {
        // Spaaaaace! I'm in space.
        // If the view ray does not intersect the atmosphere, simply return 0.
        transmittance = vec3(1);
        radiance = vec3(0);
    }

    // Compute the r, mu, mu_s and nu parameters needed for the texture lookups.
    float mu = rmu / r;
    float mu_s = dot(camera, sun_direction) / r;
    float nu = dot(view, sun_direction);
    bool ray_r_mu_intersects_ground = ray_intersects_ground(vec2(r, mu), bottom_radius);

    transmittance = ray_r_mu_intersects_ground
        ? vec3(0.0)
        : get_transmittance_to_top_atmosphere_boundary(
            vec2(r, mu), transmittance_texture, bottom_radius, top_radius);

    vec3 scattering;
    vec3 single_mie_scattering;
//    get_combined_scattering(
//        atmosphere, scattering_texture, single_mie_scattering_texture,
//        r, mu, mu_s, nu, ray_r_mu_intersects_ground,
//        single_mie_scattering);

    return;
}

//RadianceSpectrum GetSkyRadiance(
//    IN(AtmosphereParameters) atmosphere,
//    IN(TransmittanceTexture) transmittance_texture,
//    IN(ReducedScatteringTexture) scattering_texture,
//    IN(ReducedScatteringTexture) single_mie_scattering_texture,
//    Position camera,
//    IN(Direction) view_ray,
//    Length shadow_length,
//    IN(Direction) sun_direction, OUT(DimensionlessSpectrum) transmittance) {
//
//  // Compute the distance to the top atmosphere boundary along the view ray,
//  // assuming the viewer is in space (or NaN if the view ray does not intersect
//  // the atmosphere).
//  Length r = length(camera);
//  Length rmu = dot(camera, view_ray);
//  Length distance_to_top_atmosphere_boundary = -rmu -
//      sqrt(rmu * rmu - r * r + atmosphere.top_radius * atmosphere.top_radius);
//  // If the viewer is in space and the view ray intersects the atmosphere, move
//  // the viewer to the top atmosphere boundary (along the view ray):
//  if (distance_to_top_atmosphere_boundary > 0.0 * m) {
//    camera = camera + view_ray * distance_to_top_atmosphere_boundary;
//    r = atmosphere.top_radius;
//    rmu += distance_to_top_atmosphere_boundary;
//  } else if (r > atmosphere.top_radius) {
//    // If the view ray does not intersect the atmosphere, simply return 0.
//    transmittance = DimensionlessSpectrum(1.0);
//    return RadianceSpectrum(0.0 * watt_per_square_meter_per_sr_per_nm);
//  }
//
//  // Compute the r, mu, mu_s and nu parameters needed for the texture lookups.
//  Number mu = rmu / r;
//  Number mu_s = dot(camera, sun_direction) / r;
//  Number nu = dot(view_ray, sun_direction);
//  bool ray_r_mu_intersects_ground = RayIntersectsGround(atmosphere, r, mu);
//
//  transmittance = ray_r_mu_intersects_ground ? DimensionlessSpectrum(0.0) :
//      GetTransmittanceToTopAtmosphereBoundary(
//          atmosphere, transmittance_texture, r, mu);
//  IrradianceSpectrum single_mie_scattering;
//  IrradianceSpectrum scattering;
//  if (shadow_length == 0.0 * m) {
//    scattering = GetCombinedScattering(
//        atmosphere, scattering_texture, single_mie_scattering_texture,
//        r, mu, mu_s, nu, ray_r_mu_intersects_ground,
//        single_mie_scattering);
//  } else {
//    // Case of light shafts (shadow_length is the total length noted l in our
//    // paper): we omit the scattering between the camera and the point at
//    // distance l, by implementing Eq. (18) of the paper (shadow_transmittance
//    // is the T(x,x_s) term, scattering is the S|x_s=x+lv term).
//    Length d = shadow_length;
//    Length r_p =
//        ClampRadius(atmosphere, sqrt(d * d + 2.0 * r * mu * d + r * r));
//    Number mu_p = (r * mu + d) / r_p;
//    Number mu_s_p = (r * mu_s + d * nu) / r_p;
//
//    scattering = GetCombinedScattering(
//        atmosphere, scattering_texture, single_mie_scattering_texture,
//        r_p, mu_p, mu_s_p, nu, ray_r_mu_intersects_ground,
//        single_mie_scattering);
//    DimensionlessSpectrum shadow_transmittance =
//        GetTransmittance(atmosphere, transmittance_texture,
//            r, mu, shadow_length, ray_r_mu_intersects_ground);
//    scattering = scattering * shadow_transmittance;
//    single_mie_scattering = single_mie_scattering * shadow_transmittance;
//  }
//  return scattering * RayleighPhaseFunction(nu) + single_mie_scattering *
//      MiePhaseFunction(atmosphere.mie_phase_function_g, nu);
//}

vec3 get_sun_radiance(vec3 sun_irradiance, float sun_angular_radius) {
    return sun_irradiance / (PI * sun_angular_radius * sun_angular_radius);
}

void compute_sky_radiance(
    vec3 camera,
    vec3 view,
    vec3 sun_direction,
    vec3 sun_irradiance,
    float sun_angular_radius,
    float bottom_radius,
    float top_radius,
    sampler2D transmittance_texture,
    out vec3 radiance
) {
    vec3 transmittance;
    get_sky_radiance(
        camera,
        view,
        sun_direction,
        bottom_radius,
        top_radius,
        transmittance_texture,
        transmittance,
        radiance);
 /*shadow_length,*/

    //transmittance = vec3(0.000001);
    if (dot(view, sun_direction) > cos(sun_angular_radius)) {
        radiance = transmittance * get_sun_radiance(sun_irradiance, sun_angular_radius);
    }
}
