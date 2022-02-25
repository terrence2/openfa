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

#include <wgpu-buffer/shader_shared/include/consts.glsl>
#include <wgpu-buffer/shader_shared/include/quaternion.glsl>
#include <wgpu-buffer/global_data/include/global_data.glsl>
#include <wgpu-buffer/shape/include/shape.glsl>

// Vertex inputs
layout(location = 0) in vec3 position;
layout(location = 1) in vec3 normal;
layout(location = 2) in vec4 color;
layout(location = 3) in uvec2 tex_coord;
layout(location = 4) in uint flags0;
layout(location = 5) in uint flags1;
layout(location = 6) in uint xform_id;

// Outputs
layout(location = 0) smooth out vec4 v_position_w_m;
layout(location = 1) smooth out vec4 v_color;
layout(location = 2) smooth out vec3 v_normal_w;
layout(location = 3) smooth out vec2 v_tex_coord;
layout(location = 4) flat out uint f_flags0;
layout(location = 5) flat out uint f_flags1;

// Chunk group
//layout(set = 2, binding = 0) uniform texture2D chunk_mega_atlas_texture;
//layout(set = 2, binding = 1) uniform sampler chunk_mega_atlas_sampler;
layout(set = 2, binding = 2) uniform ChunkMegaAtlasProperties {
    uvec4 chunk_atlas_size;
};

// Per shape input
const uint MAX_XFORM_ID = 32;
layout(set = 3, binding = 0) readonly buffer ShapeInstanceBlockTransforms {
    float shape_transforms[];
};
layout(set = 3, binding = 1) readonly buffer ShapeInstanceBlockFlags {
    uint shape_flags[];
};
layout(set = 3, binding = 2) readonly buffer ShapeInstanceBlockXformOffsets {
    uint shape_xform_offsets[];
};
layout(set = 3, binding = 3) readonly buffer ShapeInstanceBlockXforms {
    float shape_xforms[];
};

void main() {
    uint base_transform = gl_InstanceIndex * 8;
    float transform[8] = {
        shape_transforms[base_transform + 0],
        shape_transforms[base_transform + 1],
        shape_transforms[base_transform + 2],
        shape_transforms[base_transform + 3],
        shape_transforms[base_transform + 4],
        shape_transforms[base_transform + 5],
        shape_transforms[base_transform + 6],
        shape_transforms[base_transform + 7]
    };

    float xform[8] = {0, 0, 0, 0, 0, 0, 1, 0};
    if (xform_id < MAX_XFORM_ID) {
        uint base_shape_xform = shape_xform_offsets[gl_InstanceIndex];
        uint offset = 6 * base_shape_xform + 6 * xform_id;
        for (uint i = 0; i < 6; ++i) {
            xform[i] = shape_xforms[offset + i];
        }
    }

    v_position_w_m = camera_perspective_m *
                     matrix_for_transform(transform) *
                     matrix_for_xform(xform) *
                     vec4(position, 1.0);

    v_normal_w = (rotation_for_xform(transform) *
                  rotation_for_xform(xform) *
                  vec4(normal, 1.0)).xyz;

    v_color = color;
    v_tex_coord = vec2(
        float(tex_coord.x) / float(chunk_atlas_size.x),
        float(tex_coord.y) / float(chunk_atlas_size.y)
    );

    uint base_flag = gl_InstanceIndex * 2;
    f_flags0 = flags0 & shape_flags[base_flag + 0];
    f_flags1 = flags1 & shape_flags[base_flag + 1];

    gl_Position = v_position_w_m;
}
