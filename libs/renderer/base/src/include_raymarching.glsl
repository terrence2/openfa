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

vec3
raymarching_view_ray(vec2 position, mat4 inverse_view, mat4 inverse_projection) {
    vec4 reverse_vec;

    // inverse perspective projection
    reverse_vec = vec4(position, 0.0, 1.0);
    reverse_vec = inverse_projection * reverse_vec;

    // inverse modelview, without translation
    reverse_vec.w = 0.0;
    reverse_vec = inverse_view * reverse_vec;

    return vec3(reverse_vec);
}