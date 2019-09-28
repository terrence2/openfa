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

// Bin arrangement
#define DEC_BINS 64
struct BandMetadata {
    uint index;
    uint bins_per_row;
    uint base_index;
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
