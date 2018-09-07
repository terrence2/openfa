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

#[macro_use]
extern crate failure;
#[cfg(feature = "dx12")]
extern crate gfx_backend_dx12;
#[cfg(feature = "metal")]
extern crate gfx_backend_metal;
#[cfg(feature = "vulkan")]
extern crate gfx_backend_vulkan;
extern crate gfx_hal as hal;
extern crate glsl_to_spirv;
extern crate image;
extern crate winit;

mod backend;
mod gpu;
mod window;

pub use gpu::Gpu;
pub use window::Window;
