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

// Inputs
layout(location = 0) smooth in vec4 v_position_w_m;
layout(location = 1) smooth in vec4 v_color;
layout(location = 2) smooth in vec3 v_normal_w;
layout(location = 3) smooth in vec2 v_tex_coord;
layout(location = 4) flat in uint f_flags0;
layout(location = 5) flat in uint f_flags1;

// Output
layout(location = 0) out vec4 f_color;

#include <wgpu-buffer/shader_shared/include/consts.glsl>
#include <wgpu-buffer/atmosphere/include/global.glsl>
#include <wgpu-buffer/atmosphere/include/descriptorset.glsl>
#include <wgpu-buffer/atmosphere/include/library.glsl>
#include <wgpu-buffer/global_data/include/global_data.glsl>
#include <wgpu-buffer/world/include/world.glsl>

layout(set = 2, binding = 0) uniform texture2D chunk_mega_atlas_texture;
layout(set = 2, binding = 1) uniform sampler chunk_mega_atlas_sampler;
//layout(set = 2, binding = 2) uniform ChunkMegaAtlasProperties {
//    uint chunk_mega_atlas_width;
//    uint chunk_mega_atlas_height;
//    uint padding[2];
//};

//layout(set = 6, binding = 1) uniform sampler2DArray nose_art; NOSE\\d\\d.PIC
//layout(set = 6, binding = 2) uniform sampler2DArray left_tail_art; LEFT\\d\\d.PIC
//layout(set = 6, binding = 3) uniform sampler2DArray right_tail_art; RIGHT\\d\\d.PIC
//layout(set = 6, binding = 4) uniform sampler2DArray round_art; ROUND\\d\\d.PIC

vec4
diffuse_color(out bool should_discard)
{
    should_discard = false;
    if ((f_flags0 & 0xFFFFFFFEu) == 0 && f_flags1 == 0) {
        should_discard = true;
        return vec4(0);
    } else if (v_tex_coord.x == 0.0) {
        return v_color;
    } else {
        // FIXME: I think this breaks if our mega-atlas spills into a second layer. The layer should be part
        // FIXME: of the texture coordinate we are uploading.
        vec4 tex_color = texture(sampler2D(chunk_mega_atlas_texture, chunk_mega_atlas_sampler), v_tex_coord);
        if ((f_flags0 & 1u) == 1u) {
            return vec4((1.0 - tex_color[3]) * v_color.xyz + tex_color[3] * tex_color.xyz, 1.0);
        } else if (tex_color.a < 0.5) {
            should_discard = true;
            return vec4(0);
        } else {
            return tex_color;
        }
    }
}

void main() {
    bool should_discard;
    vec4 diffuse = diffuse_color(should_discard);
    if (should_discard) {
        discard;
    }

    vec3 camera_position_w_km = camera_position_km.xyz;
    vec3 sun_direction_w = sun_direction.xyz;
    float s = 1. / 1000.;
    mat4 inverse_scale = mat4(
        s, 0, 0, 0,
        0, s, 0, 0,
        0, 0, s, 0,
        0, 0, 0, 1
    );
    vec4 intersect_w_km = camera_inverse_view_km * inverse_scale * camera_inverse_perspective_m * v_position_w_m;
    vec3 camera_direction_w = normalize(intersect_w_km.xyz - camera_position_w_km);

    vec3 radiance = radiance_at_point(
        intersect_w_km.xyz,
        v_normal_w,
        diffuse.rgb,
        sun_direction_w,
        camera_position_w_km,
        camera_direction_w
    );

    vec3 color = tone_mapping(radiance);

    f_color = vec4(color, diffuse.a);

    //float cos_ang = dot(v_normal_w, sun_direction.xyz);
    //f_color = cos_ang * diffuse;
    //f_color = pos;
}

