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
mod t2;
mod utility;

pub use crate::{arc_ball_camera::ArcBallCamera, t2::t2_renderer::T2Renderer, utility::pal_renderer::PalRenderer};

/*
use failure::Fallible;
use image::{ImageBuffer, Rgba};
use std::{cell::RefCell, collections::HashMap, sync::Arc};
use vulkano::{
    device::Device,
    format::Format,
    image::{Dimensions, ImmutableImage},
    sampler::{Filter, MipmapMode, Sampler, SamplerAddressMode},
    sync::GpuFuture,
};
use window::GraphicsWindow;


pub struct Renderer {
    window: Arc<GraphicsWindow>,

    // Asset loaders should not need to know about the details of how we track
    // resources in the GPU.
    resources: RefCell<Resources>,
}

pub struct Resources {
    outstanding_loads: Vec<Box<GpuFuture>>,

    id_index: usize,
    textures: HashMap<usize, Arc<ImmutableImage<Format>>>,
}

impl Renderer {
    pub fn new(window: Arc<GraphicsWindow>) -> Renderer {
        Renderer {
            window,
            resources: RefCell::new(Resources {
                outstanding_loads: Vec::new(),
                id_index: 1,
                textures: HashMap::new(),
            }),
        }
    }

    pub fn upload_texture(&self, image_buf: ImageBuffer<Rgba<u8>, Vec<u8>>) -> Fallible<usize> {
        let image_dim = image_buf.dimensions();
        let image_data = image_buf.into_raw().clone();

        let dimensions = Dimensions::Dim2d {
            width: image_dim.0,
            height: image_dim.1,
        };
        let (texture, tex_future) = ImmutableImage::from_iter(
            image_data.iter().cloned(),
            dimensions,
            Format::R8G8B8A8Srgb,
            self.window.queue(),
        )?;

        let mut resources = self.resources.borrow_mut();

        resources
            .outstanding_loads
            .push(Box::new(tex_future) as Box<GpuFuture>);

        let id = resources.id_index;
        resources.id_index += 1;
        resources.textures.insert(id, texture);
        Ok(id)
    }

    pub fn make_sampler(device: Arc<Device>) -> Fallible<Arc<Sampler>> {
        let sampler = Sampler::new(
            device.clone(),
            Filter::Linear,
            Filter::Linear,
            MipmapMode::Nearest,
            SamplerAddressMode::Repeat,
            SamplerAddressMode::Repeat,
            SamplerAddressMode::Repeat,
            0.0,
            1.0,
            0.0,
            0.0,
        )?;

        Ok(sampler)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use window::GraphicsConfigBuilder;

    #[test]
    fn it_works() -> Fallible<()> {
        let window = Arc::new(GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?);
        let renderer = Renderer::new(window);

        Ok(())
    }
}
*/
