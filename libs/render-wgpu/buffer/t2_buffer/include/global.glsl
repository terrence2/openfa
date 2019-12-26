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

layout(set = 2, binding = 0) uniform texture2D t2_terrain_atlas_texture;
layout(set = 2, binding = 1) uniform sampler t2_terrain_atlas_sampler;

vec4
t2_atlas_color_uv(vec2 tex_coord)
{
    return texture(sampler2D(t2_terrain_atlas_texture, t2_terrain_atlas_sampler), tex_coord);
}
