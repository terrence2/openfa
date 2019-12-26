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

layout(set = 1, binding = 0) uniform texture2D glyph_cache_glyph_texture;
layout(set = 1, binding = 1) uniform sampler glyph_cache_glyph_sampler;

float
glyph_alpha_uv(vec2 tex_coord)
{
    return texture(sampler2D(glyph_cache_glyph_texture, glyph_cache_glyph_sampler), tex_coord).r;
}
