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
#include <buffer/terrain_geo/include/global.glsl>

layout(local_size_x = 64, local_size_y = 1, local_size_z = 1) in;
layout(binding = 0) uniform SubdivisionCtx { SubdivisionContext context; };
layout(binding = 1) uniform ExpansionCtx { SubdivisionExpandContext expand; };
layout(binding = 2) buffer TargetVertices { TerrainVertex target_vertices[]; };
layout(binding = 3) buffer IndexDependencyLut { uint index_dependency_lut[]; };

void
main()
{
    // The iteration vector is over expand.compute_vertices_in_patch * num_patches.
    uint i = gl_GlobalInvocationID.x;

    // Find our patch offset and our offset within the current work set.
    uint patch_id = i / expand.compute_vertices_in_patch;
    uint relative_offset = i % expand.compute_vertices_in_patch;

    // To get the buffer offset we find our base patch offset, skip the prior computed vertices, then offset.
    uint patch_base = context.target_stride * patch_id;
    uint patch_offset = expand.skip_vertices_in_patch + relative_offset;
    uint offset = patch_base + patch_offset;

    // There are two dependencies per input, uploaded sequentially. Note that the deps are per-patch.
    uint dep_a = patch_base + index_dependency_lut[patch_offset * 2 + 0];
    uint dep_b = patch_base + index_dependency_lut[patch_offset * 2 + 1];

    target_vertices[offset].position[0] = (target_vertices[dep_a].position[0] + target_vertices[dep_b].position[0]) / 2.0;
    target_vertices[offset].position[1] = (target_vertices[dep_a].position[1] + target_vertices[dep_b].position[1]) / 2.0;
    target_vertices[offset].position[2] = (target_vertices[dep_a].position[2] + target_vertices[dep_b].position[2]) / 2.0;
    target_vertices[offset].normal[0] = 1.0;
    target_vertices[offset].normal[1] = 1.0;
    target_vertices[offset].normal[2] = 1.0;
    target_vertices[offset].graticule[0] = 0.0;
    target_vertices[offset].graticule[1] = 0.0;
}
