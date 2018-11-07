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
extern crate packed_struct;
extern crate pal;
extern crate reverse;

use failure::{ensure, Fallible};
use image::{DynamicImage, ImageRgba8};
use packed_struct::packed_struct;
use pal::Palette;
use std::mem;

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

pub fn decode_pic(system_palette: &Palette, data: &[u8]) -> Fallible<DynamicImage> {
    let header = Header::overlay(data)?;
    if header.format() == 0 {
        let pixels = &data[header.pixels_offset()..header.pixels_offset() + header.pixels_size()];
        let palette =
            &data[header.palette_offset()..header.palette_offset() + header.palette_size()];
        let local_palette = Palette::from_bytes(&palette)?;
        let mut imgbuf = image::ImageBuffer::new(header.width(), header.height());
        for (i, p) in imgbuf.pixels_mut().enumerate() {
            let pix = pixels[i] as usize;
            let mut clr = if pix < local_palette.color_count {
                local_palette.rgba(pix)?
            } else {
                system_palette.rgba(pix)?
            };
            if pix == 0xFF {
                clr.data[3] = 0x00;
            }
            *p = clr;
        }
        return Ok(ImageRgba8(imgbuf));
    } else if header.format() == 1 {
        let pixels = &data[header.pixels_offset()..header.pixels_offset() + header.pixels_size()];
        let palette =
            &data[header.palette_offset()..header.palette_offset() + header.palette_size()];
        let local_palette = Palette::from_bytes(&palette)?;
        let mut imgbuf = image::ImageBuffer::new(header.width(), header.height());
        let _spans = &data[header.spans_offset()..header.spans_offset() + header.spans_size()];
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

            for (j, column) in (span.start()..=span.end()).enumerate() {
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
    Ok(image::load_from_memory(data)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::prelude::*;

    #[test]
    fn show_all_type1() -> Fallible<()> {
        let mut fp = fs::File::open("../pal/test_data/PALETTE.PAL")?;
        let mut palette_data = Vec::new();
        fp.read_to_end(&mut palette_data)?;
        let palette = Palette::from_bytes(&palette_data)?;

        let paths = fs::read_dir("./test_data")?;
        for i in paths {
            let entry = i?;
            let path = format!("{}", entry.path().display());
            if !path.ends_with("PIC") {
                continue;
            }
            println!("AT: {}", path);

            let mut fp = fs::File::open(entry.path())?;
            let mut data = Vec::new();
            fp.read_to_end(&mut data)?;

            let _img = decode_pic(&palette, &data)?;
            // img.save(fs::File::create(path.to_owned() + ".png")?, image::PNG)?;
        }

        Ok(())
    }
}
