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
extern crate failure;
extern crate gpu;
extern crate image;
extern crate lib;
extern crate pal;
extern crate pic;
extern crate vulkano;

use failure::Fallible;
use lib::LibStack;
use pal::Palette;
use pic::decode_pic;
use std::sync::Arc;
use vulkano::{
    device::{Queue, Device},
    format::Format,
    image::{Dimensions, ImmutableImage},
    sampler::{Filter, MipmapMode, Sampler, SamplerAddressMode},
    sync::GpuFuture,
};

pub struct TextureManager<'a> {
    system_palette: Palette,
    library: &'a LibStack,
}

impl<'a> TextureManager<'a> {
    pub fn new(library: &'a LibStack) -> Fallible<Self> {
        let bytes = library.load("PALETTE.PAL")?;
        let system_palette = Palette::from_bytes(&bytes)?;
        return Ok(TextureManager {
            system_palette,
            library,
        });
    }

    pub fn load_texture(
        &self,
        filename: &str,
        queue: Arc<Queue>,
    ) -> Fallible<(Arc<ImmutableImage<Format>>, Box<GpuFuture>)> {
        let data = self.library.load(filename)?;
        let image_buf = decode_pic(&self.system_palette, &data)?.to_rgba();
        let image_dim = image_buf.dimensions();
        let image_data = image_buf.into_raw().clone();

        println!("X: {}", image_dim.0 * image_dim.1);

        let dimensions = Dimensions::Dim2d {
            width: image_dim.0,
            height: image_dim.1,
        };
        let (texture, tex_future) = ImmutableImage::from_iter(
            image_data.iter().cloned(),
            dimensions,
            Format::R8G8B8A8Uint,
            queue,
        )?;

        return Ok((texture, Box::new(tex_future) as Box<GpuFuture>));
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
extern crate omnilib;

#[cfg(test)]
mod tests {
    use super::*;
    use omnilib::OmniLib;
    use gpu::{GraphicsConfigBuilder, GraphicsWindow};

    #[test]
    fn it_works() -> Fallible<()> {
        let mut futures = Vec::new();
        let window = GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?;
        let omni = OmniLib::new_for_test()?;//_in_games(vec!["FA"])?;
        for lib in omni.libraries() {
            let texman = TextureManager::new(&lib)?;
            let (_texture, future) = texman.load_texture("FLARE.PIC", window.queue())?;
            futures.push(future);
        }

        for f in futures {
            let rv = f.then_signal_semaphore_and_flush()?.cleanup_finished();

        }

        return Ok(());
    }
}
