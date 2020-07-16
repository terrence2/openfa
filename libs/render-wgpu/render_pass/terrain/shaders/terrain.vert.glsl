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
#include <wgpu-render/shader_shared/include/consts.glsl>
#include <wgpu-render/shader_shared/include/quaternion.glsl>
#include <wgpu-buffer/global_data/include/global_data.glsl>

#define EARTH_TO_KM 6370.0

layout(location = 0) in vec3 position;
layout(location = 1) in vec3 normal;
layout(location = 2) in vec2 graticule;

layout(location = 0) out vec4 v_color;

layout(set = 2, binding = 0) uniform itexture2D srtm_index_texture;
layout(set = 2, binding = 1) uniform sampler srtm_index_sampler;
layout(set = 2, binding = 2) uniform itexture2DArray srtm_atlas_texture;
layout(set = 2, binding = 3) uniform sampler srtm_atlas_sampler;

void main() {
    // FIXME: no need for a center indicator on the projection matrix, just scale.
    gl_Position = dbg_geocenter_m_projection() * vec4(position, 1);

    // Map latitude in -60 -> 60 to 0 to ?? (1 for now, but we need metadata here).
    float latitude = graticule.x;
    float t = (latitude + (60.0 * PI / 180.0)) / (120.0 * PI / 180.0);

    // Map longitude from -180 -> 180 to 0 to ??
    float longitude = graticule.y;
    float s = (longitude + PI) / (2.0 * PI);

    // Map s, t onto the actual subsection of the atlas that is used.
    float tile_extent = 512.0 * 4096.0;
    float fract_lon = (360.0 * 60.0 * 60.0) / tile_extent;
    float fract_lat = (120.0 * 60.0 * 60.0) / tile_extent;

    // Note: layer 0 happens to be our 4096 scale top level, so just use it for now.
    /*
    ivec4 height_texel = texture(
        isampler2DArray(srtm_atlas_texture, srtm_atlas_sampler),
        vec3(
            1.0 - s * fract_lon,
            t * fract_lat,
            0
        )
    );
    float height = height_texel.r / 255.0;
    v_color = vec4(height, height, height, 1);
    */

    v_color = vec4(1, 0, 1, 1);
}
