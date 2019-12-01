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

layout(set = 0, binding = 0) buffer CameraParameters {
    mat4 globals_screen_letterbox_projection;
    mat4 globals_camera_view;
    mat4 globals_camera_projection;
    mat4 globals_camera_inverse_view;
    mat4 globals_camera_inverse_projection;
    vec4 globals_camera_position_tile;
    vec4 globals_camera_position_earth_km;
};

mat4 screen_letterbox_projection() { return globals_screen_letterbox_projection; }
mat4 camera_view()                 { return globals_camera_view; }
mat4 camera_projection()           { return globals_camera_projection; }
mat4 camera_inverse_view()         { return globals_camera_inverse_view; }
mat4 camera_inverse_projection()   { return globals_camera_inverse_projection; }
vec4 camera_position_in_tile()     { return globals_camera_position_tile; }
vec4 camera_position_earth_km()    { return globals_camera_position_earth_km; }

vec3
raymarching_view_ray(vec2 position) {
    vec4 reverse_vec;

    // inverse perspective projection
    reverse_vec = vec4(position, 0.0, 1.0);
    reverse_vec = camera_inverse_projection() * reverse_vec;

    // inverse modelview, without translation
    reverse_vec.w = 0.0;
    reverse_vec = camera_inverse_view() * reverse_vec;

    return vec3(reverse_vec);
}
