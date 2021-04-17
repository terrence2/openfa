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
layout(local_size_x = 16, local_size_y = 16, local_size_z = 1) in;

struct PicUploadInfo {
    uint width;
    uint height;
    uint stride;
};

layout(set = 0, binding = 0) uniform Info { PicUploadInfo info; };
layout(set = 0, binding = 1) buffer Palette { uint palette[256]; };
layout(set = 0, binding = 2) buffer PicData { uint raw_img[]; };
layout(set = 0, binding = 3, rgba8) writeonly uniform image2D target_texture;

void
main() {
    // Layout is over target coordinates, since those are always aligned to the block size.
    uvec2 tgt_coord = gl_GlobalInvocationID.xy;

    uint clr = 0;
    if (tgt_coord.x < info.width && tgt_coord.y < info.height) {
        // Note: source buffer is packed, stride is for target texture
        uint src_coord = info.width * tgt_coord.y + tgt_coord.x;
    }

}