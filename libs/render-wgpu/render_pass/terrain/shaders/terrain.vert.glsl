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
#include <buffer/global_data/include/library.glsl>

layout(location = 0) in vec4 position;

//layout(location = 0) out smooth vec4 v_position;
//layout(location = 1) out smooth vec4 v_normal;
//layout(location = 2) out smooth vec4 v_color;
//layout(location = 3) out smooth vec2 v_tex_coord;

struct TileData {
    vec4 position_and_scale;
};

layout(set = 2, binding = 0) uniform readonly TileUpload {
    TileData tiles[72];
};

void main() {
    vec4 p = vec4(tiles[gl_InstanceIndex].position_and_scale.xyz, 0);
    float s = tiles[gl_InstanceIndex].position_and_scale[3];
    mat4 sm = mat4(
        s, 0, 0, 0,
        0, s, 0, 0,
        0, 0, s, 0,
        0, 0, 0, 1
    );
    gl_Position = geocenter_km_projection() * geocenter_km_view() * sm * (p + position);

    //    v_position = tile_to_earth_translation() + (tile_to_earth_scale() * tile_to_earth_rotation() * (vec4(position, 1.0) - tile_center_offset()));
    //    v_normal = tile_to_earth_rotation() * vec4(normal, 1.0);
    //    v_color = color;
    //    v_tex_coord = tex_coord;
}
