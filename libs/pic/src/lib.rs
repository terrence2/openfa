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
extern crate pal;
extern crate reverse;

use reverse::b2h;
use std::mem;
use failure::Error;
use image::{DynamicImage, ImageRgba8};
use pal::Palette;

macro_rules! _make_packed_struct_accessor {
    ($field:ident, $field_name:ident, $field_ty:ty, $output_ty:ty) => {
        fn $field_name(&self) -> $output_ty {
            self.$field as $output_ty
        }
    };

    ($field:ident, $field_name:ident, $field_ty:ty, ) => {
        fn $field_name(&self) -> $field_ty {
            self.$field as $field_ty
        }
    }
}

macro_rules! packed_struct {
    ($name:ident {
        $( $field:ident => $field_name:ident : $field_ty:ty $(as $field_name_ty:ty),* ),+
    }) => {
        #[repr(C)]
        #[repr(packed)]
        struct $name {
            $(
                $field: $field_ty
            ),+
        }

        impl $name {
            $(
                _make_packed_struct_accessor!($field, $field_name, $field_ty, $($field_name_ty),*);
            )+
        }
    }
}

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
    _0 => row: u16 as u32,
    _1 => start: u16 as u32,
    _2 => end: u16 as u32,
    _3 => index: u32 as usize
});

//#[repr(C)]
//#[repr(packed)]
//struct Header {
//    _format: u16,
//    _width: u32,
//    _height: u32,
//    _pixels_offset: u32,
//    _pixels_size: u32,
//    _palette_offset: u32,
//    _palette_size: u32,
//    _spans_offset: u32,
//    _spans_size: u32,
//    _rowheads_offset: u32,
//    _rowheads_size: u32,
//}
//
//impl Header {
//    fn format(&self) -> u16 { self._format }
//    fn width(&self) -> u32 { self._width }
//    fn height(&self) -> u32 { self._height }
//    fn pixels_offset(&self) -> usize { self._pixels_offset as usize }
//    fn pixels_size(&self) -> usize { self._pixels_size as usize }
//    fn palette_offset(&self) -> usize { self._palette_offset as usize }
//    fn palette_size(&self) -> usize { self._palette_size as usize }
//    fn spans_offset(&self) -> usize { self._spans_offset as usize }
//    fn spans_size(&self) -> usize { self._spans_size as usize }
//    fn rowheads_offset(&self) -> usize { self._rowheads_offset as usize }
//    fn rowheads_size(&self) -> usize { self._rowheads_size as usize }
//}

//#[repr(C)]
//#[repr(packed)]
//struct Span {
//    _row: u16,
//    _start: u16,
//    _end: u16,
//    _index: u32,
//}
//
//impl Span {
//    fn row(&self) -> u32 { self._row as u32 }
//    fn start(&self) -> u32 { self._start as u32 }
//    fn end(&self) -> u32 { self._end as u32 }
//    fn index(&self) -> usize { self._index as usize }
//}


pub fn decode_pic(path: &str, system_palette: &Palette, data: &[u8]) -> Result<DynamicImage, Error> {
    let header_ptr: *const Header = data[0..].as_ptr() as *const _;
    let header: &Header = unsafe { &*header_ptr };

    let pixels = &data[header.pixels_offset()..header.pixels_offset() + header.pixels_size()];
    let palette = &data[header.palette_offset()..header.palette_offset() + header.palette_size()];
    let spans = &data[header.spans_offset()..header.spans_offset() + header.spans_size()];
    let rowheads = &data[header.rowheads_offset()..header.rowheads_offset() + header.rowheads_size()];

    let local_palette = Palette::from_bytes(&palette)?;

    assert!(header.spans_offset() > 0);
    assert!(header.spans_offset() < data.len());
    assert!(header.spans_offset() + header.spans_size() <= data.len());
    assert_eq!(header.spans_size() % mem::size_of::<Span>(), 0);

    let mut imgbuf = image::ImageBuffer::new(header.width(), header.height());

    let mut min_pix = 999999999usize;
    let mut max_pix = 0usize;
    let span_cnt = header.spans_size() / mem::size_of::<Span>() - 1;
    for i in 0..span_cnt {
        let span_ptr: *const Span = data[header.spans_offset() + i * mem::size_of::<Span>()..].as_ptr() as *const _;
        let span: &Span = unsafe { &*span_ptr };
        assert!(span.row() < header.height());
        assert!(span.index() < header.pixels_size());
        assert!(span.start() < header.width());
        assert!(span.end() < header.width());
        assert!(span.start() <= span.end());
        assert!(span.index() + ((span.end() - span.start()) as usize) < header.pixels_size());

        //println!("At row {} from {} @ {} to {} @ {}", span.row(), span.start(), span.index(), span.end(), span.index() + (span.end() + 1 - span.start()) as usize);
        for (j, column) in (span.start()..span.end() + 1).enumerate() {
            let offset = span.index() + j;
            let pix = pixels[offset] as usize;
            if pix < min_pix {
                min_pix = pix;
            }
            if pix > max_pix {
                max_pix = pix;
            }
            let clr = if pix < local_palette.color_count {
                local_palette.rgba(pix)?
            } else {
                system_palette.rgba(pix)?
            };
            imgbuf.put_pixel(column, span.row(), clr);
        }
    }

    println!("Range: {} -> {}", min_pix, max_pix);

    assert!(data.len() >= mem::size_of::<Header>() + header.pixels_size());

    println!("{:32}: {:6} {:4}x{:<4}: {:6} in {:>6} spans", path, header.palette_size(), header.width(), header.height(),
             header.pixels_size(), header.spans_size() / 10 - 1);

    return Ok(ImageRgba8(imgbuf));
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

            if data[0] == 1u8 {
                let img = decode_pic(&path, &palette, &data).unwrap();
                let ref mut fout = fs::File::create(path.to_owned() + ".png").unwrap();
                img.save(fout, image::PNG).unwrap();
            }
        }
    }
}
