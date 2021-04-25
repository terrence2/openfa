// This file is part of Nitrogen.
//
// Nitrogen is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Nitrogen is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with Nitrogen.  If not, see <http://www.gnu.org/licenses/>.

struct T2Info {
    float base_graticule_lat;
    float base_graticule_lon;
    float span_graticule_lat;
    float span_graticule_lon;
};

vec2
t2_base_graticule(T2Info t2_info) {
    return vec2(
        t2_info.base_graticule_lat,
        t2_info.base_graticule_lon
    );
}

vec2
t2_span_graticule(T2Info t2_info) {
    return vec2(
        t2_info.span_graticule_lat,
        t2_info.span_graticule_lon
    );
}

bool
grat_in_t2(vec2 grat, vec2 base, vec2 span) {
    return  grat.x <= base.x &&
            grat.x > (base.x - span.x) &&
            grat.y >= base.y &&
            grat.y < (base.y + span.y);
}