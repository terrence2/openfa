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

layout(location = 0) in vec2 position;
layout(location = 1) in vec2 tex_coord;

layout(set = 1, binding = 0) buffer LayoutData {
    mat4 screen_projection;
    vec4 text_color;
};

layout(location = 0) out vec2 v_tex_coord;
layout(location = 1) flat out vec4 v_color;

void main() {
    gl_Position = screen_projection * vec4(position, 0.0, 1.0);
    v_tex_coord = tex_coord;
    v_color = text_color;
}
