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
#include <wgpu-render/shader_shared/include/consts.glsl>
#include <wgpu-buffer/global_data/include/global_data.glsl>
// #include <buffer/terrain/include/global.glsl>

//#include <wgpu-buffer/atmosphere/include/global.glsl>
//#include <wgpu-buffer/atmosphere/include/descriptorset.glsl>
//#include <wgpu-buffer/atmosphere/include/library.glsl>

//layout(location = 0) in vec4 v_position; // hm tile xyz
//layout(location = 1) in vec4 v_normal; // hm tile xyz
//layout(location = 2) in vec4 v_color;
//layout(location = 3) in vec2 v_tex_coord;

layout(location = 0) out vec4 f_color;

layout(location = 0) in vec4 v_color;

// TODO: move to globals?
//const float EXPOSURE = MAX_LUMINOUS_EFFICACY * 0.0001;

void
main()
{
    f_color = v_color;
    //f_color = vec4(1, 0, 1, 1);
/*
    vec3 intersect = v_position.xyz;
    vec3 normal = v_normal.xyz;
    vec3 sun_direction = camera_and_sun[0].xyz;

    vec4 ground_albedo = t2_atlas_color_uv(v_tex_coord);
    if (v_tex_coord.x == 0.0) {
        ground_albedo = v_color;
    }

    // Get sun and sky irradiance at the ground point and modulate
    // by the ground albedo.
    vec3 sky_irradiance;
    vec3 sun_irradiance;
    get_sun_and_sky_irradiance(
        atmosphere,
        transmittance_texture,
        transmittance_sampler,
        irradiance_texture,
        irradiance_sampler,
        intersect,
        normal,
        sun_direction,
        sun_irradiance,
        sky_irradiance
    );
    vec3 ground_radiance = ground_albedo.xyz * (1.0 / PI) * (sun_irradiance + sky_irradiance);

    // Fade the radiance on the ground by the amount of atmosphere
    // between us and that point and brighten by ambient in-scatter
    // to the camera on that path.
    vec3 transmittance;
    vec3 in_scatter = vec3(0);
    get_sky_radiance_to_point(
        atmosphere,
        transmittance_texture,
        transmittance_sampler,
        scattering_texture,
        scattering_sampler,
        single_mie_scattering_texture,
        single_mie_scattering_sampler,
        camera_position_earth_km().xyz,
        intersect,
        sun_direction,
        transmittance,
        in_scatter
    );
    ground_radiance = ground_radiance * transmittance + in_scatter;

    const float EXPOSURE = MAX_LUMINOUS_EFFICACY * 0.0001;
    vec3 color = pow(
            vec3(1.0) - exp(-ground_radiance / vec3(atmosphere.whitepoint) * EXPOSURE),
            vec3(1.0 / 2.2)
        );

    f_color = vec4(color, 1);
*/
}
