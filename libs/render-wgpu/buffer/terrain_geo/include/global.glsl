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

struct TerrainVertex {
    // Note that we cannot use vec3 here as that packs into vec4 in a struct storage buffer context, unlike in a
    // vertex context where it packs properly. :shrug:
    float position[3];
    float normal[3];
    float graticule[2];
};

// 3 vertices per patch stride in the upload buffer.
#define PATCH_UPLOAD_STRIDE 3

struct SubdivisionContext {
    uint target_stride;
    uint target_subdivision_level;
    uint pad[2];
};

struct SubdivisionExpandContext {
    uint current_target_subdivision_level;
    uint skip_vertices_in_patch;
    uint compute_vertices_in_patch;
    uint pad[1];
};
