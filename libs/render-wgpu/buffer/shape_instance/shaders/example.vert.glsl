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
#include <buffer/shape_chunk/include/include_shape.glsl>
#include <buffer/camera_parameters/include/library.glsl>

// Vertex data
layout(location = 0) in vec3 position;
layout(location = 1) in vec4 color;
layout(location = 2) in vec2 tex_coord;
layout(location = 3) in uint flags0;
layout(location = 4) in uint flags1;
layout(location = 5) in uint xform_id;


// Per shape input
const uint MAX_XFORM_ID = 32;
layout(set = 3, binding = 0) buffer ChunkBaseTransforms {
    float shape_transforms[];
};
layout(set = 3, binding = 1) buffer ChunkFlags {
    uint shape_flags[];
};

//layout(set = 3, binding = 2) buffer ChunkXformOffsets {
//    uint data[];
//} shape_xform_offsets;
//        layout(set = 4, binding = 2) buffer ChunkXforms {
//            float data[];
//        } shape_xforms;

// Per Vertex input
layout(location = 0) smooth out vec4 v_color;
layout(location = 1) smooth out vec2 v_tex_coord;
layout(location = 2) flat out uint f_flags0;
layout(location = 3) flat out uint f_flags1;
void main() {
    uint base_transform = gl_InstanceIndex * 6;
    uint base_flag = gl_InstanceIndex * 2;
    float transform[6] = {
        shape_transforms[base_transform + 0],
        shape_transforms[base_transform + 1],
        shape_transforms[base_transform + 2],
        shape_transforms[base_transform + 3],
        shape_transforms[base_transform + 4],
        shape_transforms[base_transform + 5]
    };
    float xform[6] = {0, 0, 0, 0, 0, 0};
//            uint base_xform = shape_xform_offsets.data[gl_InstanceIndex];
//            if (xform_id < MAX_XFORM_ID) {
//                xform[0] = shape_xforms.data[base_xform + 6 * xform_id + 0];
//                xform[1] = shape_xforms.data[base_xform + 6 * xform_id + 1];
//                xform[2] = shape_xforms.data[base_xform + 6 * xform_id + 2];
//                xform[3] = shape_xforms.data[base_xform + 6 * xform_id + 3];
//                xform[4] = shape_xforms.data[base_xform + 6 * xform_id + 4];
//                xform[5] = shape_xforms.data[base_xform + 6 * xform_id + 5];
//            }
    gl_Position = camera_projection() *
                  camera_view() *
                  matrix_for_xform(transform) *
                  matrix_for_xform(xform) *
                  vec4(position, 1.0);
    v_color = color;
    v_tex_coord = tex_coord;
    f_flags0 = flags0 & shape_flags[base_flag + 0];
    f_flags1 = flags1 & shape_flags[base_flag + 1];
}
