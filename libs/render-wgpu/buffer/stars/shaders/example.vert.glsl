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

#include <buffer/raymarching/include/raymarching_library.glsl>

layout(set = 0, binding = 0) buffer InverseViewProjection {
    mat4[] inv_view_proj;
};

layout(location = 0) in vec2 position;
layout(location = 0) out vec3 v_ray;

void main() {
    gl_Position = vec4(position, 0.0, 1.0);
    v_ray = raymarching_view_ray(position, inv_view_proj[0], inv_view_proj[1]);
}