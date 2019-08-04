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
#include "include_atmosphere.glsl"
#include "lut_shared_builder.glsl"

vec4
compute_direct_irradiance(
    AtmosphereParameters atmosphere,
    sampler2D transmittance_texture,
    vec2 rmus
) {
    float r = rmus.x;
    float mu_s = rmus.y;

    float alpha_s = atmosphere.sun_angular_radius;
    // Approximate average of the cosine factor mu_s over the visible fraction of
    // the Sun disc.
    float average_cosine_factor =
        mu_s < -alpha_s
        ? 0.0
        : (mu_s > alpha_s
          ? mu_s
          : (mu_s + alpha_s) * (mu_s + alpha_s) / (4.0 * alpha_s));

    vec4 transmittance = get_transmittance_to_top_atmosphere_boundary(
        rmus,
        transmittance_texture,
        atmosphere.bottom_radius,
        atmosphere.top_radius
    );
    return atmosphere.sun_irradiance * transmittance * average_cosine_factor;
}

void
compute_direct_irradiance_program(
    vec2 frag_coord,
    AtmosphereParameters atmosphere,
    sampler2D transmittance_lambda,
    writeonly image2D irradiance_lambda
) {
    const vec2 TEXTURE_SIZE = vec2(IRRADIANCE_TEXTURE_WIDTH, IRRADIANCE_TEXTURE_HEIGHT);
    vec2 uv = frag_coord / TEXTURE_SIZE;
    vec2 rmus = irradiance_uv_to_rmus(
        uv,
        atmosphere.bottom_radius,
        atmosphere.top_radius
    );
    vec4 direct_irradiance = compute_direct_irradiance(
        atmosphere,
        transmittance_lambda,
        rmus
    );
    imageStore(
        irradiance_lambda,
        ivec2(frag_coord),
        direct_irradiance
    );
}
