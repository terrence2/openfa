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
#include <buffer/stars/include/global.glsl>

layout(location = 0) in vec3 v_ray;
layout(location = 0) out vec4 f_color;

#include <buffer/stars/include/descriptorset.glsl>
#include <buffer/stars/include/library.glsl>

void main() {
    #if SHOW_BINS
        f_color = vec4(show_bins(v_ray), 1.0);
        return;
    #endif

    vec3 star_radiance = vec3(0);
    float star_alpha = 0;
    show_stars(v_ray, star_radiance, star_alpha);
    f_color = vec4(star_radiance, 1.0);
}
