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

use failure::Fallible;
use gpu::GPU;
use image::RgbaImage;
use lib::LibStack;
use pal::Palette;
use pic::decode_pic;

pub struct TextureManager<'a> {
    base_palette: Palette,
    gpu: &'a GPU,
    library: &'a LibStack,
}

impl<'a> TextureManager<'a> {
    pub fn new(library: &'a LibStack, gpu: &'a GPU) -> Fallible<Self> {
        let bytes = library.load("PALETTE.PAL")?;
        let base_palette = Palette::from_bytes(&bytes)?;
        return Ok(TextureManager {
            base_palette,
            gpu,
            library,
        });
    }

    pub fn load_texture(&self, name: &str) -> Fallible<()> {
        let bytes = self.library.load(name)?;
        let img = decode_pic(&self.base_palette, &bytes)?.to_rgba();

        //return self.upload_texture(gpu, img);

        

        return Ok(());
    }

    // fn upload_texture(&self, gpu: &'a Gpu, img: RgbaImage) -> Fallible<()> {
    //     let (width, height) = img.dimensions();
    //     let kind = img::Kind::D2(width as img::Size, height as img::Size, 1, 1);
    //     let row_alignment_mask = gpu.limits().min_buffer_copy_pitch_alignment as u32 - 1;
    //     let image_stride = 4u32;
    //     let row_pitch = (width * image_stride + row_alignment_mask) & !row_alignment_mask;
    //     let upload_size = (height * row_pitch) as u64;

    //     // Convert the image into the format the GPU wants while copying it into
    //     // a GPU controlled region of memory, typically still on the CPU.
    //     let buf = gpu.create_upload_buffer(upload_size)?;
    //     gpu.with_mapped_upload_buffer(&buf, |data| {
    //         for y in 0..height as usize {
    //             let from_off = y * (width as usize) * (image_stride as usize);
    //             let to_off = (y + 1) * (width as usize) * (image_stride as usize);
    //             let row = &(*img)[from_off..to_off];
    //             let dest_base = y * (row_pitch as usize);
    //             data[dest_base..dest_base + row.len()].copy_from_slice(row);
    //         }
    //     })?;

    //     // Start copying the image to the GPU.
    //     let buf = gpu.create_image(kind)?;

    //     return Ok(());
    // }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn it_works() -> Fallible<()> {
        let mut win = gfx::Window::new(800, 600, "test-texture")?;
        win.select_any_adapter()?;
        let lib = lib::LibStack::from_dir_search(Path::new("../../test_data/unpacked/FA"))?;
        let texman = TextureManager::new(&lib, win.gpu()?)?;

        let _foo = texman.load_texture("FLARE.PIC")?;

        return Ok(());
    }
}
