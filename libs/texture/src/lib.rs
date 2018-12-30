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
extern crate image;
extern crate lib;
extern crate pal;
extern crate pic;
extern crate render;

use failure::Fallible;
use lib::Library;
use pal::Palette;
use pic::decode_pic;
use std::sync::Arc;
use render::Renderer;

pub struct TextureManager<'a> {
    system_palette: Palette,
    library: &'a Library,
    renderer: Arc<Renderer>,
}

impl<'a> TextureManager<'a> {
    pub fn new(library: &'a Library, renderer: Arc<Renderer>) -> Fallible<Self> {
        let bytes = library.load("PALETTE.PAL")?;
        let system_palette = Palette::from_bytes(&bytes)?;
        return Ok(TextureManager {
            system_palette,
            library,
            renderer
        });
    }

    pub fn load_texture(
        &self,
        filename: &str
    ) -> Fallible<usize> {
        let data = self.library.load(filename)?;
        let image_buf = decode_pic(&self.system_palette, &data)?.to_rgba();

        let id = self.renderer.upload_texture(image_buf)?;
        Ok(id)
    }
}

#[cfg(test)]
extern crate omnilib;

#[cfg(test)]
extern crate window;

#[cfg(test)]
mod tests {
    use super::*;
    use omnilib::OmniLib;
    use window::{GraphicsConfigBuilder, GraphicsWindow};

    #[test]
    fn it_works() -> Fallible<()> {
        let window = Arc::new(GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?);
        let renderer = Arc::new(Renderer::new(window.clone()));
        let omni = OmniLib::new_for_test()?; //_in_games(vec!["FA"])?;
        for lib in omni.libraries() {
            let texman = TextureManager::new(&lib, renderer.clone())?;
            let tex_id = texman.load_texture("FLARE.PIC")?;
        }

        return Ok(());
    }
}
