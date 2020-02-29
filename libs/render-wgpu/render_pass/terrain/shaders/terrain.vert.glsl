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

#define EARTH_TO_KM 6370.0

layout(location = 0) in vec4 graticule;

//layout(location = 0) out smooth vec4 v_position;
//layout(location = 1) out smooth vec4 v_normal;
//layout(location = 2) out smooth vec4 v_color;
//layout(location = 3) out smooth vec2 v_tex_coord;

void main() {
    float lat = graticule[0] * PI / 180.0;
    float lon = graticule[1] * PI / 180.0;
    vec4 pos = vec4(
        EARTH_TO_KM * -sin(lon) * cos(lat),
        EARTH_TO_KM * sin(lat),
        EARTH_TO_KM * cos(lon) * cos(lat),
        1.0
    );

    //gl_Position = camera_projection() * camera_view() * position;
    gl_Position = dbg_geocenter_km_projection() * dbg_geocenter_km_view() * pos;
    //    v_position = tile_to_earth_translation() + (tile_to_earth_scale() * tile_to_earth_rotation() * (vec4(position, 1.0) - tile_center_offset()));
    //    v_normal = tile_to_earth_rotation() * vec4(normal, 1.0);
    //    v_color = color;
    //    v_tex_coord = tex_coord;
}
