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

vec3 get_solar_luminance(
    vec3 sun_irradiance,
    float sun_angular_radius,
    vec3 sun_spectral_radiance_to_luminance
) {
  return sun_irradiance /
      (PI * sun_angular_radius * sun_angular_radius) * sun_spectral_radiance_to_luminance;
}

//vec3 get_sun_radiance(vec3 sun_irradiance, float sun_angular_radius) {
//    return sun_irradiance / (PI * sun_angular_radius * sun_angular_radius);
//}


void get_sun_and_sky_irradiance(
    AtmosphereParameters atmosphere,
    sampler2D transmittance_texture,
    sampler2D irradiance_texture,
    vec3 point,
    vec3 normal,
    vec3 sun_direction,
    out vec3 sun_irradiance,
    out vec3 sky_irradiance
) {
    float r = length(point);
    float mu_s = dot(point, sun_direction) / r;

    // Indirect irradiance (approximated if the surface is not horizontal).
    vec4 irradiance_at_point = get_irradiance(
        irradiance_texture,
        vec2(r, mu_s),
        atmosphere.bottom_radius,
        atmosphere.top_radius
    );
    sky_irradiance = vec3(irradiance_at_point * (1.0 + dot(normal, point) / r) * 0.5);

    // Direct irradiance.
    vec4 transmittance_to_point = get_transmittance_to_sun(
        transmittance_texture,
        r,
        mu_s,
        atmosphere.bottom_radius,
        atmosphere.top_radius,
        atmosphere.sun_angular_radius
    );
    sun_irradiance = vec3(atmosphere.sun_irradiance * transmittance_to_point *
        max(dot(normal, sun_direction), 0.0));
}

float get_sun_visibility(vec3 point, vec3 sun_direction) {
    /*
    vec3 p = point - kSphereCenter;
    float p_dot_v = dot(p, sun_direction);
    float p_dot_p = dot(p, p);
    float ray_sphere_center_squared_distance = p_dot_p - p_dot_v * p_dot_v;
    float distance_to_intersection = -p_dot_v - sqrt(
      kSphereRadius * kSphereRadius - ray_sphere_center_squared_distance);
    if (distance_to_intersection > 0.0) {
        // Compute the distance between the view ray and the sphere, and the
        // corresponding (tangent of the) subtended angle. Finally, use this to
        // compute an approximate sun visibility.
        float ray_sphere_distance =
            kSphereRadius - sqrt(ray_sphere_center_squared_distance);
        float ray_sphere_angular_distance = -ray_sphere_distance / p_dot_v;
        return smoothstep(1.0, 0.0, ray_sphere_angular_distance / sun_size.x);
    }
    return 1.0;
    */
    return 1.0;
}

float get_sky_visibility(vec3 point) {
    //vec3 p = point - kSphereCenter;
    //float p_dot_p = dot(p, p);
    //return 1.0 + p.z / sqrt(p_dot_p) * kSphereRadius * kSphereRadius / p_dot_p;
    return 1.0;
}

void get_combined_scattering(
    AtmosphereParameters atmosphere,
    sampler3D scattering_texture,
    sampler3D single_mie_scattering_texture,
    ScatterCoord sc,
    bool ray_r_mu_intersects_ground,
    out vec3 scattering,
    out vec3 single_mie_scattering
) {
    vec4 uvwz = scattering_rmumusnu_to_uvwz(
        sc,
        atmosphere.bottom_radius,
        atmosphere.top_radius,
        atmosphere.mu_s_min,
        ray_r_mu_intersects_ground
    );
    float tex_coord_x = uvwz.x * float(SCATTERING_TEXTURE_NU_SIZE - 1);
    float tex_x = floor(tex_coord_x);
    float lerp = tex_coord_x - tex_x;
    vec3 uvw0 = vec3((tex_x + uvwz.y) / float(SCATTERING_TEXTURE_NU_SIZE), uvwz.z, uvwz.w);
    vec3 uvw1 = vec3((tex_x + 1.0 + uvwz.y) / float(SCATTERING_TEXTURE_NU_SIZE), uvwz.z, uvwz.w);
    scattering = vec3(
        texture(scattering_texture, uvw0) * (1.0 - lerp) +
        texture(scattering_texture, uvw1) * lerp);
    single_mie_scattering = vec3(
        texture(single_mie_scattering_texture, uvw0) * (1.0 - lerp) +
        texture(single_mie_scattering_texture, uvw1) * lerp);
}

void get_sky_radiance_to_point(
    AtmosphereParameters atmosphere,
    sampler2D transmittance_texture,
    sampler3D scattering_texture,
    sampler3D single_mie_scattering_texture,
    vec3 camera,
    vec3 point,
    vec3 sun_direction,
    out vec3 transmittance,
    out vec3 radiance
) {
    // Compute the distance to the top atmosphere boundary along the view ray,
    // assuming the viewer is in space (or NaN if the view ray does not intersect
    // the atmosphere).
    vec3 view_ray = normalize(point - camera);
    float r = length(camera);
    float rmu = dot(camera, view_ray);
    float distance_to_top_atmosphere_boundary = -rmu -
        sqrt(rmu * rmu - r * r + atmosphere.top_radius * atmosphere.top_radius);

    // If the viewer is in space and the view ray intersects the atmosphere, move
    // the viewer to the top atmosphere boundary (along the view ray):
    if (distance_to_top_atmosphere_boundary > 0.0) {
        camera = camera + view_ray * distance_to_top_atmosphere_boundary;
        r = atmosphere.top_radius;
        rmu += distance_to_top_atmosphere_boundary;
    }

    // Compute the r, mu, mu_s and nu parameters for the first texture lookup.
    float mu = rmu / r;
    float mu_s = dot(camera, sun_direction) / r;
    float nu = dot(view_ray, sun_direction);
    float d = length(point - camera);
    bool ray_r_mu_intersects_ground = ray_intersects_ground(vec2(r, mu), atmosphere.bottom_radius);

    transmittance = vec3(get_transmittance(
        transmittance_texture,
        r, mu, d,
        ray_r_mu_intersects_ground,
        atmosphere.bottom_radius,
        atmosphere.top_radius));

    vec3 single_mie_scattering;
    vec3 scattering;
    get_combined_scattering(
        atmosphere,
        scattering_texture,
        single_mie_scattering_texture,
        ScatterCoord(r, mu, mu_s, nu),
        ray_r_mu_intersects_ground,
        scattering,
        single_mie_scattering
    );

    // TODO: adjust scattering down by amount of atmosphere occluded by shadowing objects.

    // Compute the r, mu, mu_s and nu parameters for the second texture lookup.
    // If shadow_length is not 0 (case of light shafts), we want to ignore the
    // scattering along the last shadow_length meters of the view ray, which we
    // do by subtracting shadow_length from d (this way scattering_p is equal to
    // the S|x_s=x_0-lv term in Eq. (17) of our paper).
    d = max(d - 0.0, 0.0);
    float r_p = clamp_radius(sqrt(d * d + 2.0 * r * mu * d + r * r), atmosphere.bottom_radius, atmosphere.top_radius);
    float mu_p = (r * mu + d) / r_p;
    float mu_s_p = (r * mu_s + d * nu) / r_p;

    vec3 single_mie_scattering_p;
    vec3 scattering_p;
    get_combined_scattering(
        atmosphere,
        scattering_texture,
        single_mie_scattering_texture,
        ScatterCoord(r_p, mu_p, mu_s_p, nu),
        ray_r_mu_intersects_ground,
        scattering_p,
        single_mie_scattering_p
    );

    // Combine the lookup results to get the scattering between camera and point.
    scattering = scattering - transmittance * scattering_p;
    single_mie_scattering = single_mie_scattering - transmittance * single_mie_scattering_p;

    // Hack to avoid rendering artifacts when the sun is below the horizon.
    single_mie_scattering = single_mie_scattering * smoothstep(0.0, 0.01, mu_s);

    radiance = scattering * rayleigh_phase_function(nu) +
        single_mie_scattering * mie_phase_function(atmosphere.mie_phase_function_g, nu);
}

void compute_ground_radiance(
    AtmosphereParameters atmosphere,
    sampler2D transmittance_texture,
    sampler3D scattering_texture,
    sampler3D single_mie_scattering_texture,
    sampler2D irradiance_texture,
    vec3 camera,
    vec3 view,
    vec3 sun_direction,
    out vec3 ground_radiance,
    out float ground_alpha
) {
    // The planet center is always at 0.
    vec3 p = camera - vec3(0, 0, 0);
    float p_dot_v = dot(p, view);
    float p_dot_p = dot(p, p);
    float dist2 = p_dot_p - p_dot_v * p_dot_v;
    float t0 = -p_dot_v - sqrt(atmosphere.bottom_radius * atmosphere.bottom_radius - dist2);

    ground_alpha = 0.0;
    ground_radiance = vec3(0);
    if (t0 > 0.0) {
        ground_alpha = 1.0;

        vec3 intersect = camera + view * t0;
        vec3 normal = normalize(intersect);

        // Get sun and sky irradiance at the ground point and modulate
        // by the ground albedo.
        vec3 sky_irradiance;
        vec3 sun_irradiance;
        get_sun_and_sky_irradiance(
            atmosphere,
            transmittance_texture,
            irradiance_texture,
            intersect,
            normal,
            sun_direction,
            sun_irradiance,
            sky_irradiance
        );
        ground_radiance = vec3(atmosphere.ground_albedo) * (1.0 / PI) * (
            sun_irradiance * get_sun_visibility(intersect, sun_direction) +
            sky_irradiance * get_sky_visibility(intersect)
        );

        // Fade the radiance on the ground by the amount of atmosphere
        // between us and that point and brighten by ambient in-scatter
        // to the camera on that path.
        vec3 transmittance;
        vec3 in_scatter;
        get_sky_radiance_to_point(
            atmosphere,
            transmittance_texture,
            scattering_texture,
            single_mie_scattering_texture,
            camera,
            intersect,
            sun_direction,
            transmittance,
            in_scatter
        );

        ground_radiance = ground_radiance * transmittance + in_scatter;
    }
}

void get_sky_radiance(
    AtmosphereParameters atmosphere,
    sampler2D transmittance_texture,
    sampler3D scattering_texture,
    sampler3D single_mie_scattering_texture,
    vec3 camera,
    vec3 view,
    vec3 sun_direction,
    out vec3 transmittance,
    out vec3 radiance
) {
//    transmittance = vec3(1);
//    radiance = vec3(0);

    // Compute the distance to the top atmosphere boundary along the view ray,
    // assuming the viewer is in space (or NaN if the view ray does not intersect
    // the atmosphere).
    float r = length(camera);
    float rmu = dot(camera, view);
    float t0 = -rmu - sqrt(rmu * rmu - r * r + atmosphere.top_radius * atmosphere.top_radius);
    if (t0 > 0.0) {
        // If the viewer is in space and the view ray intersects the atmosphere, move
        // the viewer to the top atmosphere boundary (along the view ray):
        camera = camera + view * t0;
        r = atmosphere.top_radius;
        rmu += t0;
    } else if (r > atmosphere.top_radius) {
        // Spaaaaace! I'm in space.
        // If the view ray does not intersect the atmosphere, simply return 0.
        transmittance = vec3(1);
        radiance = vec3(0);
        return;
    }

    // Compute the r, mu, mu_s and nu parameters needed for the texture lookups.
    float mu = rmu / r;
    float mu_s = dot(camera, sun_direction) / r;
    float nu = dot(view, sun_direction);
    bool ray_r_mu_intersects_ground = ray_intersects_ground(vec2(r, mu), atmosphere.bottom_radius);

    transmittance = ray_r_mu_intersects_ground
        ? vec3(0.0)
        : vec3(get_transmittance_to_top_atmosphere_boundary(
            vec2(r, mu), transmittance_texture, atmosphere.bottom_radius, atmosphere.top_radius));

    vec3 scattering;
    vec3 single_mie_scattering;
    get_combined_scattering(
        atmosphere,
        scattering_texture,
        single_mie_scattering_texture,
        ScatterCoord(r, mu, mu_s, nu),
        ray_r_mu_intersects_ground,
        scattering,
        single_mie_scattering);

    radiance = scattering = scattering * rayleigh_phase_function(nu) +
               single_mie_scattering * mie_phase_function(atmosphere.mie_phase_function_g, nu);
}

void compute_sky_radiance(
    AtmosphereParameters atmosphere,
    sampler2D transmittance_texture,
    sampler3D scattering_texture,
    sampler3D single_mie_scattering_texture,
    sampler2D irradiance_texture,
    vec3 camera,
    vec3 view,
    vec3 sun_direction,
    out vec3 radiance
) {
    vec3 transmittance;
    get_sky_radiance(
        atmosphere,
        transmittance_texture,
        scattering_texture,
        single_mie_scattering_texture,
        camera,
        view,
        sun_direction,
        transmittance,
        radiance);

    if (dot(view, sun_direction) > cos(atmosphere.sun_angular_radius)) {
        vec3 sun_lums = get_solar_luminance(
            vec3(atmosphere.sun_irradiance),
            atmosphere.sun_angular_radius,
            atmosphere.sun_spectral_radiance_to_luminance
        );
        radiance = transmittance * sun_lums;
    }
}
