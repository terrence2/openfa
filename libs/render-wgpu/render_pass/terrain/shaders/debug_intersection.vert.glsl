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
#include <common/shader_globals/include/quaternion.glsl>
#include <buffer/global_data/include/library.glsl>

#define EARTH_TO_KM 6370.0

layout(location = 0) in vec4 position;
layout(location = 1) in vec4 color;

layout(location = 0) out smooth vec4 v_color;

void main() {
    v_color = color;
    gl_Position = dbg_geocenter_m_projection() * position;
}
