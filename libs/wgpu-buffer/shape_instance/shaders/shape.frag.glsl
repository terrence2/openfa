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

// Inputs
layout(location = 0) smooth in vec4 v_color;
layout(location = 1) smooth in vec2 v_tex_coord;
layout(location = 2) flat in uint f_flags0;
layout(location = 3) flat in uint f_flags1;

// Output
layout(location = 0) out vec4 f_color;

layout(set = 2, binding = 0) uniform texture2D chunk_mega_atlas_texture;
layout(set = 2, binding = 1) uniform sampler chunk_mega_atlas_sampler;

//layout(set = 6, binding = 1) uniform sampler2DArray nose_art; NOSE\\d\\d.PIC
//layout(set = 6, binding = 2) uniform sampler2DArray left_tail_art; LEFT\\d\\d.PIC
//layout(set = 6, binding = 3) uniform sampler2DArray right_tail_art; RIGHT\\d\\d.PIC
//layout(set = 6, binding = 4) uniform sampler2DArray round_art; ROUND\\d\\d.PIC

void main() {
    if ((f_flags0 & 0xFFFFFFFEu) == 0 && f_flags1 == 0) {
        discard;
    } else if (v_tex_coord.x == 0.0) {
        f_color = v_color;
    } else {
        // FIXME: I think this breaks if our mega-atlas spills into a second layer. The layer should be part
        // FIXME: of the texture coordinate we are uploading.
        vec4 tex_color = texture(sampler2D(chunk_mega_atlas_texture, chunk_mega_atlas_sampler), v_tex_coord);
        if ((f_flags0 & 1u) == 1u) {
            f_color = vec4((1.0 - tex_color[3]) * v_color.xyz + tex_color[3] * tex_color.xyz, 1.0);
        } else {
            if (tex_color.a < 0.5)
                discard;
            else
                f_color = tex_color;
        }
    }
}
