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

float fa2r(float d) {
    return d * PI / 8192.0;
}

mat3 from_euler_angles(float roll, float pitch, float yaw)
{
    float sr = sin(roll);
    float cr = cos(roll);
    float sp = sin(pitch);
    float cp = cos(pitch);
    float sy = sin(yaw);
    float cy = cos(yaw);

    return mat3(
    cy * cp,
    sy * cp,
    -sp,

    cy * sp * sr - sy * cr,
    sy * sp * sr + cy * cr,
    cp * sr,

    cy * sp * cr + sy * sr,
    sy * sp * cr - cy * sr,
    cp * cr
    );
}

mat4 matrix_for_xform(float xform[6]) {
    // ma.xform_data[(6 * xform_id) + 0]
    float t0 = xform[0];
    float t1 = xform[1];
    float t2 = xform[2];
    float r0 = xform[3];
    float r1 = xform[4];
    float r2 = xform[5];
    mat4 trans = mat4(
        1.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        t0,  t1,  t2, 1.0
    );
    mat4 rot = mat4(from_euler_angles(r0, r1, r2));
    return trans * rot;
}
