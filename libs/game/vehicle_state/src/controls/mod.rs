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

// Controls are generally scriptable components that take in user input.

pub(crate) mod pitch_inceptor;
pub(crate) mod throttle;

// Takes position and returns the modified value.
pub(crate) fn inceptor_position_tick(target: f64, dt: f64, mut position: f64) -> f64 {
    if target > position {
        position += dt;
        if target < position {
            position = target;
        }
    } else if target < position {
        position -= dt;
        if target > position {
            position = target;
        }
    }
    position.max(-1.).min(1.)
}
