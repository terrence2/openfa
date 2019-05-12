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
use nalgebra::Matrix4;

mod arc_ball_camera;
pub use arc_ball_camera::ArcBallCamera;

pub trait CameraAbstract {
    fn view_matrix(&self) -> Matrix4<f32>;
    fn projection_matrix(&self) -> Matrix4<f32>;
}

pub struct IdentityCamera;

impl CameraAbstract for IdentityCamera {
    fn view_matrix(&self) -> Matrix4<f32> {
        Matrix4::identity()
    }

    fn projection_matrix(&self) -> Matrix4<f32> {
        Matrix4::identity()
    }
}
