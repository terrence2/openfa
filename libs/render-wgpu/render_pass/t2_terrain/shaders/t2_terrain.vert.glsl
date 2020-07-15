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
#include <common/shader_globals/include/global.glsl>
#include <wgpu-buffer/global_data/include/global_data.glsl>

layout(location = 0) in vec3 position;
layout(location = 1) in vec3 normal;
layout(location = 2) in vec4 color;
layout(location = 3) in vec2 tex_coord;

layout(location = 0) out smooth vec4 v_position;
layout(location = 1) out smooth vec4 v_normal;
layout(location = 2) out smooth vec4 v_color;
layout(location = 3) out smooth vec2 v_tex_coord;

void main() {
    gl_Position = camera_projection() * camera_view() * vec4(position, 1.0);
    v_position = tile_to_earth_translation() + (tile_to_earth_scale() * tile_to_earth_rotation() * (vec4(position, 1.0) - tile_center_offset()));
    v_normal = tile_to_earth_rotation() * vec4(normal, 1.0);
    v_color = color;
    v_tex_coord = tex_coord;
}
