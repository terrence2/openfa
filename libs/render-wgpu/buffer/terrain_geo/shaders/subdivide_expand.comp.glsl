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

    // Do normal interpolation the normal way.
    vec3 na = vec3(target_vertices[dep_a].normal[0], target_vertices[dep_a].normal[1], target_vertices[dep_a].normal[2]);
    vec3 nb = vec3(target_vertices[dep_b].normal[0], target_vertices[dep_b].normal[1], target_vertices[dep_b].normal[2]);
    vec3 tmp = na + nb;
    vec3 nt = tmp / length(tmp);
    // Note clamp to 1 to avoid NaN from acos.
    float w = acos(min(1, dot(na, nt)));

    // Use the haversine geodesic midpoint method to compute graticule.
    // j/k => a/b
    float phi_a = target_vertices[dep_a].graticule[0];
    float theta_a = target_vertices[dep_a].graticule[1];
    float phi_b = target_vertices[dep_b].graticule[0];
    float theta_b = target_vertices[dep_b].graticule[1];
    // bx = cos(φk) · cos(θk−θj)
    float beta_x = cos(phi_b) * cos(theta_b - theta_a);
    // by = cos(φk) · sin(θk−θj)
    float beta_y = cos(phi_b) * sin(theta_b - theta_a);
    // φi = atan2(sin(φj) + sin(φk), √((cos(φj) + bx)^2 + by^2))
    float cpa_beta_x = cos(phi_a) + beta_x;
    float phi_t = atan(
        sin(phi_a) + sin(phi_b),
        sqrt(cpa_beta_x * cpa_beta_x + beta_y * beta_y)
    );
    // θi = θj + atan2(by, cos(φj) + bx)
    float theta_t = theta_a + atan(beta_y, cos(phi_a) + beta_x);

    // Use the clever tan method from figure 35.
    vec3 pa = vec3(target_vertices[dep_a].position[0], target_vertices[dep_a].position[1], target_vertices[dep_a].position[2]);
    vec3 pb = vec3(target_vertices[dep_b].position[0], target_vertices[dep_b].position[1], target_vertices[dep_b].position[2]);
    float x = length(pb - pa) / 2.0;
    // Note that the angle we get is not the same as the opposite-over-adjacent angle we want.
    // It seems to be related to that angle though, by being 2x that angle; thus, divide by 2.
    float y = x * tan(w / 2);
    vec3 midpoint = (pa + pb) / 2.0;
    vec3 pt = midpoint + y * nt;

    target_vertices[offset].position[0] = pt.x;
    target_vertices[offset].position[1] = pt.y;
    target_vertices[offset].position[2] = pt.z;
    target_vertices[offset].normal[0] = nt.x;
    target_vertices[offset].normal[1] = nt.y;
    target_vertices[offset].normal[2] = nt.z;
    target_vertices[offset].graticule[0] = phi_t;
    target_vertices[offset].graticule[1] = theta_t;
}
