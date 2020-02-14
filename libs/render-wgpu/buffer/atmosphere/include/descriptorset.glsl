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

layout(set = 1, binding = 0) uniform CameraAndSun { vec4 sun_direction; };
layout(set = 1, binding = 1) uniform AtmosphereParams { AtmosphereParameters atmosphere; };
layout(set = 1, binding = 2) uniform texture2D transmittance_texture;
layout(set = 1, binding = 3) uniform sampler transmittance_sampler;
layout(set = 1, binding = 4) uniform texture2D irradiance_texture;
layout(set = 1, binding = 5) uniform sampler irradiance_sampler;
layout(set = 1, binding = 6) uniform texture3D scattering_texture;
layout(set = 1, binding = 7) uniform sampler scattering_sampler;
layout(set = 1, binding = 8) uniform texture3D single_mie_scattering_texture;
layout(set = 1, binding = 9) uniform sampler single_mie_scattering_sampler;
