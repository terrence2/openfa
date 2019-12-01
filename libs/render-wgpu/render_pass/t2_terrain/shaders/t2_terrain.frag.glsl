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
#include <buffer/t2_buffer/include/global.glsl>

#include <buffer/atmosphere/include/global.glsl>
#include <buffer/atmosphere/include/descriptorset.glsl>
#include <buffer/atmosphere/include/library.glsl>

layout(location = 0) in vec4 v_color;
layout(location = 1) in vec2 v_tex_coord;

layout(location = 0) out vec4 f_color;

void main() {
    if (v_tex_coord.x == 0.0) {
        discard;
    } else {
        f_color = t2_atlas_color_uv(v_tex_coord);
    }
}
