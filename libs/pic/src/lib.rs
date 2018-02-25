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
//
// There appears to be a 40 byte header of this form:
// 00000000  00 00           ; fmt
//           80 02 00 00     ; width
//           e0 01 00 00     ; height
//           40 00 00 00     ; 64 (pixels_offset)
//           00 b0
// 00000010  04 00           ; pixels_size
//           c0 b7 04 00     ; palette_offset
//           00 03 00 00     ; palette_size
//           00 00 00 00     ; unk0
//           ca 12
// 00000020  00 00           ; unk1
//           40 b0 04 00     ; rowheads_offset
//           80 07 00 00     ; rowheads_size
//           00 00 00 00 00 00
// 00000030  00 00 00 00 00 00 00 00  00 00 00 00 00 00 00 00

// 00000000  01 00
//           C8 00 00 00
//           C8 00 00 00
//           40 00 00 00 pixels_offset
//           40 9C 00 00 pixels_size
//           5A A4 00 00   <- Should be palette, but is garbage?
//           C0 00 00 00
//           80 9C 00 00 spans_offset
//           DA 07 00 00 spans_size
//           00 00 00 00 rowheads_offset
//           00 00 00 00 rowheads_size
//           00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00

extern crate failure;
extern crate image;
extern crate reverse;

use reverse::b2h;
use std::{cmp, mem};
use failure::Error;
use std::io::Write;
use std::fs::File;
use image::{Pixel, Rgb, Rgba};


#[repr(C)]
#[repr(packed)]
struct Header {
    _format: u16,
    _width: u32,
    _height: u32,
    _pixels_offset: u32,
    _pixels_size: u32,
    _palette_offset: u32,
    _palette_size: u32,
    _spans_offset: u32,
    _spans_size: u32,
    _rowheads_offset: u32,
    _rowheads_size: u32,
}

impl Header {
    fn format(&self) -> u16 { self._format }
    fn width(&self) -> u32 { self._width }
    fn height(&self) -> u32 { self._height }
    fn pixels_offset(&self) -> usize { self._pixels_offset as usize }
    fn pixels_size(&self) -> usize { self._pixels_size as usize }
    fn palette_offset(&self) -> usize { self._palette_offset as usize }
    fn palette_size(&self) -> usize { self._palette_size as usize }
    fn spans_offset(&self) -> usize { self._spans_offset as usize }
    fn spans_size(&self) -> usize { self._spans_size as usize }
    fn rowheads_offset(&self) -> usize { self._rowheads_offset as usize }
    fn rowheads_size(&self) -> usize { self._rowheads_size as usize }
}

#[repr(C)]
#[repr(packed)]
struct Span {
    _row: u16,
    _start: u16,
    _end: u16,
    _index: u32,
}

impl Span {
    fn row(&self) -> u32 { self._row as u32 }
    fn start(&self) -> u32 { self._start as u32 }
    fn end(&self) -> u32 { self._end as u32 }
    fn index(&self) -> usize { self._index as usize }
}


pub fn decode_pic(path: &str, system_palette: &[Rgba<u8>], data: &[u8]) -> Result<(), Error> {
    let header_ptr: *const Header = data[0..].as_ptr() as *const _;
    let header: &Header = unsafe { &*header_ptr };

    assert_eq!(header.rowheads_offset(), 0);
    assert_eq!(header.rowheads_size(), 0);

    let pixels = &data[header.pixels_offset()..header.pixels_offset() + header.pixels_size()];
    let palette = &data[header.palette_offset()..header.palette_offset() + header.palette_size()];
    let spans = &data[header.spans_offset()..header.spans_offset() + header.spans_size()];
    let rowheads = &data[header.rowheads_offset()..header.rowheads_offset() + header.rowheads_size()];

    /*
    let mut local_palette = Vec::new();
    let palette = if header.palette_offset() > 0 {
        let palette_data = &data[header.palette_offset()..header.palette_offset() + header.palette_size()];
        assert_eq!(header.palette_size() % 3, 0);
        let color_count = header.palette_size() / 3;
        for i in 0..color_count {
            local_palette.push(Rgba { data: [
                palette_data[i * 3 + 0] * 3,
                palette_data[i * 3 + 1] * 3,
                palette_data[i * 3 + 2] * 3,
                255
            ] });
        }
        &local_palette
    } else {
        system_palette
    };
    println!("Pal size: {}", palette.len());
    */

    let mut v = Vec::new();
    for &b in &data[0..mem::size_of::<Header>()] {
        b2h(b, &mut v);
        v.push(' ');
    }
//    for &b in &data[mem::size_of::<Header>()..mem::size_of::<Header>() + header.pixels_size() as usize] {
//        b2h(b, &mut v);
//        v.push(' ');
//    }
//    v.push(' ');
//    v.push(' ');
//    v.push(' ');
//    v.push(' ');
//    v.push(' ');
//    for &b in &data[mem::size_of::<Header>() + header.pixels_size() as usize..] {
//        b2h(b, &mut v);
//        v.push(' ');
//    }
    // palette
    println!("Reading from {:x} to {:x}", header.palette_offset(), header.palette_offset() + header.palette_size());
    for &b in &data[header.palette_offset()..header.palette_offset() + header.palette_size()] {
        b2h(b, &mut v);
        v.push(' ');
    }
    let s = v.iter().collect::<String>();
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
            let clr = if pix < palette.len() {
                Rgba { data: [
                    palette[pix * 3 + 0] * 3,
                    palette[pix * 3 + 1] * 3,
                    palette[pix * 3 + 2] * 3,
                    255
                ] }
            } else {
                assert!(false);
                system_palette[pix]
            };
//            let clr = system_palette[pix].to_rgba();
//            let clr = Rgba { data: [
//                palette[pix + 0] * 3,
//                palette[pix + 1] * 3,
//                palette[pix + 2] * 3,
//                255
//            ] };
            imgbuf.put_pixel(column, span.row(), clr);
        }
    }

    println!("Range: {} -> {}", min_pix, max_pix);

    assert!(data.len() >= mem::size_of::<Header>() + header.pixels_size());

    println!("{:32}: {:6} {:4}x{:<4}: {:6} in {:>6} spans => {}", path, header.palette_size(), header.width(), header.height(),
             header.pixels_size(), header.spans_size() / 10 - 1, s);

    let ref mut fout = File::create(path.to_owned() + ".png").unwrap();
    image::ImageRgba8(imgbuf).save(fout, image::PNG).unwrap();

    return Ok(());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::prelude::*;

    #[test]
    fn show_all_type1() {
        let mut fp = fs::File::open("PALETTE.PAL").unwrap();
        let mut palette_data = Vec::new();
        fp.read_to_end(&mut palette_data).unwrap();
        let mut palette = Vec::new();
        for i in 0..0x100 {
            palette.push(Rgba { data: [
                palette_data[i * 3 + 0] * 3,
                palette_data[i * 3 + 1] * 3,
                palette_data[i * 3 + 2] * 3,
                255,
            ]});
        }

        let mut rv: Vec<String> = Vec::new();
        let paths = fs::read_dir("./test_data").unwrap();
        for i in paths {
            let entry = i.unwrap();
            let path = format!("{}", entry.path().display());
//            if path != "./test_data/IFMVLA.PIC" {
//                continue;
//            }

            let mut fp = fs::File::open(entry.path()).unwrap();
            let mut data = Vec::new();
            fp.read_to_end(&mut data).unwrap();

            if data[0] == 1u8 {
                decode_pic(&path, &palette, &data);
            }
        }
    }
}
