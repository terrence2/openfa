// This file is part of Nitrogen.
//
// Nitrogen is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Nitrogen is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with Nitrogen.  If not, see <http://www.gnu.org/licenses/>.
#version 450
#include <nitrogen/wgpu-buffer/shader_shared/include/buffer_helpers.glsl>
#include <nitrogen/wgpu-buffer/shader_shared/include/consts.glsl>
#include <nitrogen/wgpu-buffer/terrain/include/terrain.glsl>
#include <nitrogen/wgpu-buffer/terrain/include/layout_accumulate.glsl>
#include <wgpu-buffer/t2_tile_set/include/t2_tile_set.glsl>

layout(local_size_x = 8, local_size_y = 8, local_size_z = 1) in;

layout(set = 2, binding = 0) uniform T2TerrainInfo { T2Info t2_info; };
layout(set = 2, binding = 1) uniform texture2D height_texture;
layout(set = 2, binding = 2) uniform sampler height_sampler;
layout(set = 2, binding = 3) uniform texture2D atlas_texture;
layout(set = 2, binding = 4) uniform sampler atlas_sampler;
layout(set = 2, binding = 5) uniform texture2D base_color_texture;
layout(set = 2, binding = 6) uniform sampler base_color_sampler;
//layout(set = 2, binding = 7) uniform utexture2D index_texture;
//layout(set = 2, binding = 8) uniform sampler index_sampler;


void
main()
{
    ivec2 coord = ivec2(gl_GlobalInvocationID.xy);

    // Do a depth check to see if we're even looking at terrain.
    float depth = texelFetch(sampler2D(terrain_deferred_depth, terrain_linear_sampler), coord, 0).x;
    if (depth > -1) {
        // Project the graticule into uv for the given t2 tile.
        vec2 grat = texelFetch(sampler2D(terrain_deferred_texture, terrain_linear_sampler), coord, 0).xy;
        vec2 t2_base = t2_base_graticule(t2_info);
        vec2 t2_span = t2_span_graticule(t2_info);
        vec2 uv = vec2(
            ((grat.y - t2_base.y) / t2_span.y) * cos(grat.x),
            1. - (t2_base.x - grat.x) / t2_span.x
        );
        bool inside = all(bvec4(greaterThanEqual(uv, vec2(0)), lessThanEqual(uv, vec2(1))));


        vec4 new = texture(sampler2D(base_color_texture, base_color_sampler), uv);


        // Blend based on whether we are inside.
        vec4 old = imageLoad(terrain_color_acc, coord);
        vec4 result = mix(old, new, vec4(min(min(float(inside), new.a), t2_info.blend_factor)));
        imageStore(terrain_color_acc, coord, result);
    }
}