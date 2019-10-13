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

layout(location = 0) smooth in vec4 v_color;
layout(location = 1) smooth in vec2 v_tex_coord;
layout(location = 2) flat in uint f_flags0;
layout(location = 3) flat in uint f_flags1;

layout(location = 0) out vec4 f_color;

//layout(set = 5, binding = 0) uniform sampler2DArray mega_atlas;
layout(set = 5, binding = 0) uniform texture2DArray mega_atlas_texture;
layout(set = 5, binding = 1) uniform sampler mega_atlas_sampler;

void main() {
    if ((f_flags0 & 0xFFFFFFFE) == 0 && f_flags1 == 0) {
        discard;
    } else if (v_tex_coord.x == 0.0) {
        f_color = v_color;
    } else {
        vec4 tex_color = texture(sampler2DArray(mega_atlas_texture, mega_atlas_sampler), vec3(v_tex_coord, 0));

        if ((f_flags0 & 1) == 1) {
            f_color = vec4((1.0 - tex_color[3]) * v_color.xyz + tex_color[3] * tex_color.xyz, 1.0);
        } else {
            if (tex_color.a < 0.5)
                discard;
            else
                f_color = tex_color;
        }
    }
}
