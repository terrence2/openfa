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
#include <wgpu-buffer/global_data/include/global_data.glsl>

layout(location = 0) in vec3 position;
layout(location = 1) in vec3 normal;
layout(location = 2) in vec2 graticule;

void main() {
    // FIXME: no need for a center indicator on the projection matrix, just scale.
    gl_Position = dbg_geocenter_m_projection() * vec4(position, 1);
}
