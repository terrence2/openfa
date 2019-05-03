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

/// Renders raw assets into GPU primitives.
mod arc_ball_camera;
mod dlg;
mod sh;
mod t2;
mod utility;

pub use crate::{
    arc_ball_camera::ArcBallCamera,
    dlg::dlg_renderer::DialogRenderer,
    sh::raw_sh_renderer::{DrawMode, RawShRenderer},
    sh::sh_renderer::{DrawMode as DrawMode2, ShRenderer},
    t2::t2_renderer::T2Renderer,
    utility::pal_renderer::PalRenderer,
};
