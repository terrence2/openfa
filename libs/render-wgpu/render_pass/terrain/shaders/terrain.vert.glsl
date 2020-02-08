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
#include <common/shader_globals/include/quaternion.glsl>
#include <buffer/global_data/include/library.glsl>

layout(location = 0) in vec4 position;

//layout(location = 0) out smooth vec4 v_position;
//layout(location = 1) out smooth vec4 v_normal;
//layout(location = 2) out smooth vec4 v_color;
//layout(location = 3) out smooth vec2 v_tex_coord;

struct TileData {
    vec4 rotation_and_scale;
};

layout(set = 2, binding = 0) uniform readonly TileUpload {
    TileData tiles[1024];
};

void main() {
    vec3 rot = tiles[gl_InstanceIndex].rotation_and_scale.xyz;
    float s = tiles[gl_InstanceIndex].rotation_and_scale[3];

    vec4 q_lon = quat_from_axis_angle(vec3(0, 1, 0), rot[1]);
    vec4 q_lat = quat_from_axis_angle(quat_rotate(q_lon, vec4(1, 0, 0, 0)).xyz, rot[0]);
    vec4 q_facing = quat_from_axis_angle(vec3(0, 0, 1), PI + rot[2]);
    vec4 pos_geocenter_km = quat_rotate(q_lat, quat_rotate(q_lon, quat_rotate(q_facing, position)));

    mat4 scale = mat4(
        s, 0, 0, 0,
        0, s, 0, 0,
        0, 0, s, 0,
        0, 0, 0, 1
    );

    gl_Position = geocenter_km_projection() * geocenter_km_view() * scale * pos_geocenter_km;
}
