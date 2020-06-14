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
layout(binding = 1) buffer TargetVertices { TerrainVertex subdivide_vertices[]; };
layout(binding = 2) buffer UploadVertices { TerrainVertex patch_upload_vertices[]; };

// We upload the frame's patches in one big block for performance, but we need to
// expand into a much bigger buffer where those cannot be adjacent. Copying patch
// seed vertices into the rendering vertex buffer is the first step.
void
main()
{
    // One invocation per vertex.
    uint i = gl_GlobalInvocationID.x;

    // Find our patch offset and our offset within the uploaded patch.
    uint patch_id = i / PATCH_UPLOAD_STRIDE;
    uint offset = i % PATCH_UPLOAD_STRIDE;

    // Project our input into the target patch.
    subdivide_vertices[patch_id * context.target_stride + offset] = patch_upload_vertices[i];
}
