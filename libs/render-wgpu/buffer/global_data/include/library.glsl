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
    vec4 camera_graticule_radians_meters;
    mat4 globals_camera_view;
    mat4 globals_camera_projection;
    mat4 debug_geocenter_km_view;
    mat4 debug_geocenter_m_projection;
    mat4 local_geocenter_km_inverse_view;
    mat4 local_geocenter_km_inverse_projection;
    mat4 globals_tile_to_earth;
    mat4 globals_tile_to_earth_rotation;
    mat4 globals_tile_to_earth_scale;
    vec4 globals_tile_to_earth_translation;
    vec4 globals_tile_center_offset;
    vec4 globals_camera_position_tile;
    vec4 globals_camera_position_earth_km;
};

mat4 screen_letterbox_projection() { return globals_screen_letterbox_projection; }
vec4 camera_graticule_rad_m()      { return camera_graticule_radians_meters; }
mat4 camera_view()                 { return globals_camera_view; }
mat4 camera_projection()           { return globals_camera_projection; }
vec4 camera_position_in_tile()     { return globals_camera_position_tile; }
vec4 camera_position_earth_km()    { return globals_camera_position_earth_km; }
mat4 dbg_geocenter_km_view()       { return debug_geocenter_km_view; }
mat4 dbg_geocenter_m_projection()  { return debug_geocenter_m_projection; }
mat4 tile_to_earth()               { return globals_tile_to_earth; }
mat4 tile_to_earth_rotation()      { return globals_tile_to_earth_rotation; }
mat4 tile_to_earth_scale()         { return globals_tile_to_earth_scale; }
vec4 tile_to_earth_translation()   { return globals_tile_to_earth_translation; }
vec4 tile_center_offset()          { return globals_tile_center_offset; }

vec3
raymarching_view_ray(vec2 position)
{
    vec4 reverse_vec;

    // inverse perspective projection
    reverse_vec = vec4(position, 0.0, 1.0);
    reverse_vec = local_geocenter_km_inverse_projection * reverse_vec;

    // inverse modelview, without translation
    reverse_vec.w = 0.0;
    reverse_vec = local_geocenter_km_inverse_view * reverse_vec;

    return vec3(reverse_vec);
}
