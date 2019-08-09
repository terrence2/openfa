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

layout(set = 1, binding = 0) uniform ConstantData {
    AtmosphereParameters atmosphere;
} cd;
layout(set = 1, binding = 1) uniform sampler2D transmittance_texture;
layout(set = 1, binding = 2) uniform sampler3D scattering_texture;
layout(set = 1, binding = 3) uniform sampler3D single_mie_scattering_texture;
layout(set = 1, binding = 4) uniform sampler2D irradiance_texture;
