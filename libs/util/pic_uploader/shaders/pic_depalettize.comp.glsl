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
layout(local_size_x = 64, local_size_y = 1, local_size_z = 1) in;

layout(set = 0, binding = 0) readonly buffer Palette { uint palette[256]; };
layout(set = 0, binding = 1) readonly buffer RawData { uint raw_img[]; };
layout(set = 0, binding = 2) writeonly buffer TgtData { uint tgt_img[]; };

vec4
unpackUnorm4x8(uint v)
{
    return vec4(
        ((v >> 0) & 0xFFu) / 255.0,
        ((v >> 8) & 0xFFu) / 255.0,
        ((v >> 16) & 0xFFu) / 255.0,
        ((v >> 24) & 0xFFu) / 255.0
    );
}

void
main() {
    // Unpack 4 packed, 1 byte pixels
    uint block_offset = gl_GlobalInvocationID.x;
    uvec4 p = uvec4(unpackUnorm4x8(raw_img[block_offset]) * 255.0);

    // look up each pixel in the palette, then write back to the target.
    tgt_img[4 * block_offset + 0] = palette[p[0]];
    tgt_img[4 * block_offset + 1] = palette[p[1]];
    tgt_img[4 * block_offset + 2] = palette[p[2]];
    tgt_img[4 * block_offset + 3] = palette[p[3]];
}