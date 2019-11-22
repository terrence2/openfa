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
use nalgebra::{Isometry3, Matrix4, Perspective3, Point3};

mod arc_ball_camera;
mod ufo_camera;

pub use arc_ball_camera::ArcBallCamera;
pub use ufo_camera::UfoCamera;

pub trait CameraAbstract {
    fn view(&self) -> Isometry3<f32>;
    fn projection(&self) -> Perspective3<f64>;
    fn view_matrix(&self) -> Matrix4<f32>;
    fn projection_matrix(&self) -> Matrix4<f32>;
    fn inverted_projection_matrix(&self) -> Matrix4<f32>;
    fn inverted_view_matrix(&self) -> Matrix4<f32>;
    fn position(&self) -> Point3<f32>;
}

pub struct IdentityCamera;

impl CameraAbstract for IdentityCamera {
    fn view(&self) -> Isometry3<f32> {
        Isometry3::identity()
    }

    fn projection(&self) -> Perspective3<f64> {
        Perspective3::new(1f64, 90f64, 0.1f64, 100f64)
    }

    fn view_matrix(&self) -> Matrix4<f32> {
        Matrix4::identity()
    }

    fn projection_matrix(&self) -> Matrix4<f32> {
        Matrix4::identity()
    }

    fn inverted_projection_matrix(&self) -> Matrix4<f32> {
        Matrix4::identity()
    }

    fn inverted_view_matrix(&self) -> Matrix4<f32> {
        Matrix4::identity()
    }

    fn position(&self) -> Point3<f32> {
        Point3::new(0f32, 0f32, 0f32)
    }
}
