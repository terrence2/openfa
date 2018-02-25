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
#[macro_use] extern crate packed_struct;
#[macro_use] extern crate failure;
extern crate image;
extern crate pal;
extern crate reverse;

use std::mem;
use failure::Error;
use image::{DynamicImage, ImageRgb8, ImageRgba8};
use pal::Palette;

packed_struct!(Header {
    _0 => format: u16,
    _1 => width: u32,
    _2 => height: u32,
    _3 => pixels_offset: u32 as usize,
    _4 => pixels_size: u32 as usize,
    _5 => palette_offset: u32 as usize,
    _6 => palette_size: u32 as usize,
    _7 => spans_offset: u32 as usize,
    _8 => spans_size: u32 as usize,
    _9 => rowheads_offset: u32 as usize,
    _a => rowheads_size: u32 as usize
});

packed_struct!(Span {
    _row   => row: u16 as u32,
    _start => start: u16 as u32,
    _end   => end: u16 as u32,
    _index => index: u32 as usize
});

pub fn decode_pic(path: &str, system_palette: &Palette, data: &[u8]) -> Result<DynamicImage, Error> {
    let header = Header::overlay(data)?;
    if header.format() == 0 {
        let pixels = &data[header.pixels_offset()..header.pixels_offset() + header.pixels_size()];
        let palette = &data[header.palette_offset()..header.palette_offset() + header.palette_size()];
        let local_palette = Palette::from_bytes(&palette)?;
        let mut imgbuf = image::ImageBuffer::new(header.width(), header.height());
        for (i, p) in imgbuf.pixels_mut().enumerate() {
            let pix = pixels[i] as usize;
            let clr = if pix < local_palette.color_count {
                local_palette.rgb(pix)?
            } else {
                system_palette.rgb(pix)?
            };
            *p = clr;
        }
        return Ok(ImageRgb8(imgbuf));
    } else if header.format() == 1 {
        let pixels = &data[header.pixels_offset()..header.pixels_offset() + header.pixels_size()];
        let palette = &data[header.palette_offset()..header.palette_offset() + header.palette_size()];
        let local_palette = Palette::from_bytes(&palette)?;
        let mut imgbuf = image::ImageBuffer::new(header.width(), header.height());
        let spans = &data[header.spans_offset()..header.spans_offset() + header.spans_size()];
        assert_eq!(header.spans_size() % mem::size_of::<Span>(), 0);
        let span_cnt = header.spans_size() / mem::size_of::<Span>() - 1;
        for i in 0..span_cnt {
            let span = Span::overlay(&data[header.spans_offset() + i * mem::size_of::<Span>()..])?;
            assert!(span.row() < header.height());
            assert!(span.index() < header.pixels_size());
            assert!(span.start() < header.width());
            assert!(span.end() < header.width());
            assert!(span.start() <= span.end());
            assert!(span.index() + ((span.end() - span.start()) as usize) < header.pixels_size());

            for (j, column) in (span.start()..span.end() + 1).enumerate() {
                let offset = span.index() + j;
                let pix = pixels[offset] as usize;
                let clr = if pix < local_palette.color_count {
                    local_palette.rgba(pix)?
                } else {
                    system_palette.rgba(pix)?
                };
                imgbuf.put_pixel(column, span.row(), clr);
            }
        }
        return Ok(ImageRgba8(imgbuf));
    }

    // Otherwise it's just a normal jpeg.
    return Ok(image::load_from_memory(data)?);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::prelude::*;

    #[test]
    fn show_all_type1() {
        let mut fp = fs::File::open("../pal/test_data/PALETTE.PAL").unwrap();
        let mut palette_data = Vec::new();
        fp.read_to_end(&mut palette_data).unwrap();
        let palette = Palette::from_bytes(&palette_data).unwrap();

        let mut rv: Vec<String> = Vec::new();
        let paths = fs::read_dir("./test_data").unwrap();
        for i in paths {
            let entry = i.unwrap();
            let path = format!("{}", entry.path().display());
            println!("AT: {}", path);

            let mut fp = fs::File::open(entry.path()).unwrap();
            let mut data = Vec::new();
            fp.read_to_end(&mut data).unwrap();

            //if data[0] == 1u8 {
                let img = decode_pic(&path, &palette, &data).unwrap();
                let ref mut fout = fs::File::create(path.to_owned() + ".png").unwrap();
                img.save(fout, image::PNG).unwrap();
            //}
        }
    }
}
