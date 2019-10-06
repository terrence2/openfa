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
#include <buffer/raymarching/include/raymarching_library.glsl>
#include <buffer/atmosphere/include/common.glsl>

#include <buffer/raymarching/include/descriptorset.glsl>
#include <buffer/atmosphere/include/descriptorset.glsl>

layout(location = 0) in vec2 position;
layout(location = 0) out vec3 v_ray;
layout(location = 1) out flat vec3 v_camera;
layout(location = 2) out flat vec3 v_sun_direction;

void main() {
    v_ray = raymarching_view_ray(position, inv_view_proj[0], inv_view_proj[1]);
    v_camera = camera_and_sun[0].xyz;
    v_sun_direction = camera_and_sun[1].xyz;
    gl_Position = vec4(position, 0.0, 1.0);
}
