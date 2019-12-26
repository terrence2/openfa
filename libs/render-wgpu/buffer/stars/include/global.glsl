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

// Constants
#define SHOW_BINS 0

// Bin arrangement
#define DEC_BINS 64
struct BandMetadata {
    uint index;
    uint bins_per_row;
    uint base_index;
    uint pad;
};

// Bin Info
struct BinPosition {
    // Base offset into the star index buffer.
    uint index_base;

    // Number of stars in this bin.
    uint num_indexes;
};

struct StarInst {
    float ra;
    float dec;
    float color[3];
    float radius;
};

layout(set = 2, binding = 0) buffer DeclinationBands {
    BandMetadata stars_bands[33];
};
layout(set = 2, binding = 1) buffer BinPositions {
    BinPosition stars_bins[5434];
};
layout(set = 2, binding = 2) buffer Indexes {
    uint stars_indexes[];
};
layout(set = 2, binding = 3) buffer StarBlock {
    StarInst stars_stars[];
};

void v_to_ra_d(vec3 v, out float ra, out float dec) {
    ra = atan(v.x, v.z) + PI;
    dec = atan(v.y, sqrt(v.x * v.x + v.z * v.z));
}

vec3 ra_d_to_v(float ra, float dec) {
    return vec3(
        cos(dec) * sin(ra),
        -sin(dec),
        cos(dec) * cos(ra)
    );
}

BandMetadata band_for_dec(float dec) {
    // Project dec into 0..1
    float decz = ((dec + PI_2) * 2.0) / TAU;

    // Quantize dec into bands.
    uint deci = uint(decz * DEC_BINS);
    return stars_bands[deci];
}

uint bin_for_ra_d(float ra, float dec) {
    BandMetadata band = band_for_dec(dec);
    float raz = ra / TAU;
    uint rai = uint(float(band.bins_per_row) * raz);
    return band.base_index + rai;
}

void show_stars(
    vec3 view,
    out vec3 star_radiance,
    out float star_alpha
) {
    float ra, dec;
    v_to_ra_d(view, ra, dec);

    uint bin = bin_for_ra_d(ra, dec);
    BinPosition pos = stars_bins[bin];

    star_alpha = 0.0;
    for (uint i = pos.index_base; i < pos.index_base + pos.num_indexes; ++i) {
        uint star_index = stars_indexes[i];
        StarInst star = stars_stars[star_index];
        vec3 star_ray = ra_d_to_v(star.ra, star.dec);
        float dist = acos(dot(star_ray, normalize(view)));
        if (dist < star.radius) {
            star_radiance = vec3(
                star.color[0], star.color[1], star.color[2]
            );
            star_alpha = 1.0;
        }
    }
}

vec3 show_bins(vec3 view) {
    float ra, dec;
    v_to_ra_d(view, ra, dec);

    float raz = ra / TAU;
    float decz = ((dec + PI_2) * 2.0) / TAU;

    BandMetadata meta = band_for_dec(dec);
    uint rai = uint(raz * meta.bins_per_row);
    int deci = int(decz * DEC_BINS);

    vec3 clr = vec3(0);
    if ((rai & uint(1)) != 0) {
        if ((deci & 1) != 0) {
            clr = vec3(0.0, 1.0, 1.0);
        } else {
            clr = vec3(1.0, 0.0, 1.0);
        }
    } else {
        if ((deci & 1) != 0) {
            clr = vec3(1.0, 0.0, 1.0);
        } else {
            clr = vec3(0.0, 1.0, 1.0);
        }
    }

    return clr;
}
