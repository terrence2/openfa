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

float
get_layer_density(DensityProfileLayer layer, float altitude) {
    float density = layer.exp_term * exp(layer.exp_scale * altitude) +
        layer.linear_term * altitude + layer.constant_term;
    return clamp(density, 0.0, 1.0);
}

float
get_profile_density(DensityProfile profile, float altitude) {
    return altitude < profile.layer0.width
        ? get_layer_density(profile.layer0, altitude)
        : get_layer_density(profile.layer1, altitude);
}
