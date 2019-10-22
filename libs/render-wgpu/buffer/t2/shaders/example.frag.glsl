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

layout(location = 0) in vec4 v_color;
layout(location = 1) in vec2 v_tex_coord;

layout(location = 0) out vec4 f_color;

layout(set = 3, binding = 0) uniform sampler2D tex;

void main() {
    if (v_tex_coord.x == 0.0) {
        f_color = v_color;
    } else {
        f_color = texture(tex, v_tex_coord);
    }
}
