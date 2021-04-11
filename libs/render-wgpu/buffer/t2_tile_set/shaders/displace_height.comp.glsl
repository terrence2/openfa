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
#include <nitrogen/wgpu-buffer/terrain/include/terrain.glsl>
#include <render-wgpu/buffer/t2_tile_set/include/t2_tile_set.glsl>

layout(local_size_x = 64, local_size_y = 1, local_size_z = 1) in;

layout(set = 0, binding = 0) buffer Vertices { TerrainVertex vertices[]; };

layout(set = 1, binding = 0) uniform T2TerrainInfo { T2Info t2_info; };
layout(set = 1, binding = 1) uniform texture2D height_texture;
layout(set = 1, binding = 2) uniform sampler height_sampler;

void
main()
{
    // One invocation per vertex.
    uint i = gl_GlobalInvocationID.x;
    vec2 v_grat = arr_to_vec2(vertices[i].graticule);
    vec3 v_normal = arr_to_vec3(vertices[i].normal);

    float height = 0;
    vec3 v_position = arr_to_vec3(vertices[i].position);

    vec2 t2_base = t2_base_graticule(t2_info);
    vec2 t2_span = t2_span_graticule(t2_info);

    if (
        v_grat.x >= t2_base.x && v_grat.x < (t2_base.x + t2_span.x) &&
        v_grat.y >= t2_base.y && v_grat.y < (t2_base.y + t2_span.y)
    ) {
        v_position = arr_to_vec3(vertices[i].surface_position);
    }

    /*
    if (vertices[i].graticule[0] > t2_info.base_graticule[0] &&
        vertices[i].graticule[0] <= (t2_info.base_graticule[0] + t2_info.span_graticule[0])) {
        v_position = arr_to_vec3(vertices[i].surface_position);
    }
    */

    vertices[i].position = vec3_to_arr(v_position + (float(height) * v_normal));
}
