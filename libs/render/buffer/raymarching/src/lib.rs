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
//use camera::CameraAbstract;
use failure::Fallible;
use log::trace;
use std::sync::Arc;
use vulkano::{
    buffer::{BufferUsage, CpuAccessibleBuffer},
    impl_vertex,
};
use window::GraphicsWindow;

#[derive(Copy, Clone)]
pub struct RaymarchingVertex {
    position: [f32; 2],
}

impl_vertex!(RaymarchingVertex, position);

pub struct RaymarchingBuffer {
    // FIXME: expose these via a method once we are using proper buffers.
    pub vertex_buffer: Arc<CpuAccessibleBuffer<[RaymarchingVertex]>>,
    pub index_buffer: Arc<CpuAccessibleBuffer<[u32]>>,
}

impl RaymarchingBuffer {
    pub fn new(window: &GraphicsWindow) -> Fallible<Self> {
        // Compute vertices such that we can handle any aspect ratio, or set up the camera to handle this?
        let x0 = -1f32;
        let x1 = 1f32;
        let y0 = -1f32;
        let y1 = 1f32;
        let verts = vec![
            RaymarchingVertex { position: [x0, y0] },
            RaymarchingVertex { position: [x0, y1] },
            RaymarchingVertex { position: [x1, y0] },
            RaymarchingVertex { position: [x1, y1] },
        ];
        let indices = vec![0u32, 1u32, 2u32, 3u32];

        trace!(
            "uploading vertex buffer with {} bytes",
            std::mem::size_of::<RaymarchingVertex>() * verts.len()
        );
        let vertex_buffer =
            CpuAccessibleBuffer::from_iter(window.device(), BufferUsage::all(), verts.into_iter())?;

        trace!(
            "uploading index buffer with {} bytes",
            std::mem::size_of::<u32>() * indices.len()
        );
        let index_buffer = CpuAccessibleBuffer::from_iter(
            window.device(),
            BufferUsage::all(),
            indices.into_iter(),
        )?;

        Ok(Self {
            vertex_buffer,
            index_buffer,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use window::{GraphicsConfigBuilder, GraphicsWindow};

    #[test]
    fn it_works() -> Fallible<()> {
        let window = GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?;
        let _raymarching_renderer = RaymarchingBuffer::new(&window)?;
        Ok(())
    }
}
