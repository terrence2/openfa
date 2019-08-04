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

layout(set = 2, binding = 0) buffer DeclinationBands {
    BandMetadata arr[33];
} starbox_bands;
layout(set = 2, binding = 1) buffer BinPositions {
    BinPosition arr[5434];
} starbox_bins;
layout(set = 2, binding = 2) buffer Indexes {
    uint arr[];
} starbox_indexes;
layout(set = 2, binding = 3) buffer StarBlock {
    StarInst arr[];
} starbox_stars;

