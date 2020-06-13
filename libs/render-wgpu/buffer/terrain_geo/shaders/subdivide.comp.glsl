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

struct PatchVertex {
    float position[3];
    float normal[3];
    float graticule[2];
};

layout(local_size_x = 64, local_size_y = 1, local_size_z = 1) in;
layout(binding = 0) buffer PatchVertices { PatchVertex patch_vertices[]; };
layout(binding = 1) buffer writeonly SubdivideVertices { PatchVertex subdivide_vertices[]; };

void
main()
{
    uint i = gl_GlobalInvocationID.x;
    subdivide_vertices[i * 3 + 0] = patch_vertices[i * 3 + 0];
    subdivide_vertices[i * 3 + 1] = patch_vertices[i * 3 + 1];
    subdivide_vertices[i * 3 + 2] = patch_vertices[i * 3 + 2];
}
