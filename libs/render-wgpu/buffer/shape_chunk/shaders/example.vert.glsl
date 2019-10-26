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

// Per Vertex input
layout(location = 0) in vec3 position;
layout(location = 1) in vec4 color;
layout(location = 2) in vec2 tex_coord;
layout(location = 3) in uint flags0;
layout(location = 4) in uint flags1;
layout(location = 5) in uint xform_id;

#include <buffer/camera_parameters/include/library.glsl>

// Per shape input
layout(set = 3, binding = 0) buffer ChunkFlags {
    uint flag_data[];
};
layout(set = 3, binding = 1) buffer ChunkXforms {
    float xform_data[];
};

#include <buffer/shape_chunk/include/include_shape.glsl>

layout(location = 0) smooth out vec4 v_color;
layout(location = 1) smooth out vec2 v_tex_coord;
layout(location = 2) flat out uint f_flags0;
layout(location = 3) flat out uint f_flags1;

void main() {
    uint shape_base_flag = 0;
    uint shape_base_xform = 0;
    if (gl_InstanceIndex >= 10) {
        shape_base_flag = 2;
        shape_base_xform = 24;
    }

    float xform[6] = {
        xform_data[shape_base_xform + 6 * xform_id + 0],
        xform_data[shape_base_xform + 6 * xform_id + 1],
        xform_data[shape_base_xform + 6 * xform_id + 2],
        xform_data[shape_base_xform + 6 * xform_id + 3],
        xform_data[shape_base_xform + 6 * xform_id + 4],
        xform_data[shape_base_xform + 6 * xform_id + 5],
    };

    gl_Position = camera_projection() * camera_view() * matrix_for_xform(xform) * vec4(position, 1.0);
    gl_Position.x += float(gl_InstanceIndex) * 10.0;
    v_color = color;
    v_tex_coord = tex_coord;

    f_flags0 = flags0 & flag_data[shape_base_flag + 0];
    f_flags1 = flags1 & flag_data[shape_base_flag + 1];
}
