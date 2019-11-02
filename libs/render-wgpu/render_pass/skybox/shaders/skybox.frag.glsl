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
#version 450
#include <common/include/include_global.glsl>
#include <buffer/atmosphere/include/global.glsl>
#include <buffer/stars/include/global.glsl>
#include <buffer/global_data/include/library.glsl>

layout(location = 0) in vec3 v_ray;
layout(location = 0) out vec4 f_color;

const float EXPOSURE = MAX_LUMINOUS_EFFICACY * 0.0001;

#include <buffer/atmosphere/include/descriptorset.glsl>
#include <buffer/stars/include/descriptorset.glsl>

#include <buffer/atmosphere/include/library.glsl>
#include <buffer/stars/include/library.glsl>

// Stars are in J2000 coordinates, so vec3(0, 1, 0) points to polaris, rather than elliptical up. This is nice as
// it means that we don't have to do any work to align the ground / planet we draw to the stars. That said,
// whatever passes in the sun direction *does* need to account for that relative tilt.
void main() {
    vec3 view = normalize(v_ray);
    vec3 camera = camera_position().xyz;
    vec3 sun_direction = camera_and_sun[1].xyz;

    vec3 ground_radiance;
    float ground_alpha;
    compute_ground_radiance(
        atmosphere,
        transmittance_texture,
        transmittance_sampler,
        scattering_texture,
        scattering_sampler,
        single_mie_scattering_texture,
        single_mie_scattering_sampler,
        irradiance_texture,
        irradiance_sampler,
        camera,
        view,
        sun_direction,
        ground_radiance,
        ground_alpha);

    vec3 sky_radiance = vec3(0);
    compute_sky_radiance(
        atmosphere,
        transmittance_texture,
        transmittance_sampler,
        scattering_texture,
        scattering_sampler,
        single_mie_scattering_texture,
        single_mie_scattering_sampler,
        irradiance_texture,
        irradiance_sampler,
        camera,
        view,
        sun_direction,
        sky_radiance
    );

    vec3 star_radiance;
    float star_alpha = 0.5;
    show_stars(view, star_radiance, star_alpha);

    vec3 radiance = sky_radiance + star_radiance * star_alpha;
    radiance = mix(radiance, ground_radiance, ground_alpha);

    vec3 color = pow(
        vec3(1.0) - exp(-radiance / vec3(atmosphere.whitepoint) * EXPOSURE),
        vec3(1.0 / 2.2)
    );
    f_color = vec4(color, 1.0);
}
