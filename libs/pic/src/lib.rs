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
//           40 00 00 00     ; 64
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

extern crate memmap;
extern crate failure;
extern crate image;
extern crate reverse;

use reverse::b2h;
use std::mem;
//use image::{ImageBuffer, ImageRgba8};
use failure::Error;
use memmap::MmapOptions;
use std::io::Write;
use std::fs::File;


#[repr(C)]
#[repr(packed)]
struct Header {
    _format: u16,
    _width: u32,
    _height: u32,
    _always_64: u32,
    _pixels_size: u32,
    _palette_offset: u32,
    _palette_size: u32,
    _unknown_offset: u32,
    _unknown_size: u32,
    _rowheads_offset: u32,
    _rowheads_size: u32,
    _padding: [u8; 22]
}

impl Header {
    fn width(&self) -> u32 { self._width }
    fn height(&self) -> u32 { self._height }
    fn pixels_size(&self) -> usize { self._pixels_size as usize }
    fn palette_offset(&self) -> usize { self._palette_offset as usize }
    fn palette_size(&self) -> usize { self._palette_size as usize }
    fn rowheads_offset(&self) -> usize { self._rowheads_offset as usize }
    fn rowheads_size(&self) -> usize { self._rowheads_size as usize }
    fn unknown_offset(&self) -> usize { self._unknown_offset as usize }
    fn unknown_size(&self) -> usize { self._unknown_size as usize }
}

#[repr(C)]
#[repr(packed)]
struct Element {
    _row: u16,
    _start: u16,
    _end: u16,
    _index: u32
}

impl Element {
    fn row(&self) -> usize { self._row as usize }
    fn start(&self) -> usize { self._start as usize }
    fn end(&self) -> usize { self._end as usize }
    fn index(&self) -> usize { self._index as usize }
}


pub fn decode_pic(path: &str, data: &[u8]) -> Result<(), Error> {
    let header_ptr: *const Header = data[0..].as_ptr() as *const _;
    let header: &Header = unsafe { &*header_ptr };

    let mut v = Vec::new();
//    for &b in &data[0..mem::size_of::<Header>()] {
//        b2h(b, &mut v);
//        v.push(' ');
//    }
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
//    for &b in &data[header.palette_offset()..header.palette_offset() + header.palette_size()] {
//        b2h(b, &mut v);
//        v.push(' ');
//    }
    assert!(header.unknown_offset() > 0);
    assert!(header.unknown_offset() < data.len());
    assert!(header.unknown_offset() + header.unknown_size() <= data.len());
    assert!(header.unknown_size() % mem::size_of::<Element>() == 0);
    //assert!(header.pixels_size() % 3 == 0);

    let mut prior_row = 0;
    let mut prior_index = 0;
    let element_cnt = header.unknown_size() / mem::size_of::<Element>();
    for i in 0..element_cnt {
        let element_ptr: *const Element = data[header.unknown_offset() + i * mem::size_of::<Element>()..].as_ptr() as *const _;
        let element: &Element = unsafe { &*element_ptr };
        assert!(element.row() >= prior_row);
        prior_row = element.row();
        assert!(element.index() < header.pixels_size());
        assert!(element.start() < header.width() as usize);
        assert!(element.end() < header.width() as usize);
        assert!(element.start() <= element.end());
        

    }


    for (i, &b) in data[header.unknown_offset()..header.unknown_offset() + header.unknown_size()].iter().enumerate() {
        b2h(b, &mut v);
        v.push(' ');
        if (i + 11) % 10 == 0 {
            v.push(' ');
        }
    }
    let s = v.iter().collect::<String>();

    assert!(data.len() >= mem::size_of::<Header>() + header.pixels_size());

    println!("{:32}: {:4}x{:<4}: {:7}pix, {:6}+{:<6} => {}", path, header.width(), header.height(),
             header.width() * header.height(), header.pixels_size(), header.unknown_size(),
             s);
    return Ok(());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::prelude::*;

    #[test]
    fn show_all_type1() {
        let mut rv: Vec<String> = Vec::new();
        let paths = fs::read_dir("./test_data").unwrap();
        for i in paths {
            let entry = i.unwrap();
            let path = format!("{}", entry.path().display());

            /*
            let file = File::open("README.md")?;
            let mmap = unsafe { MmapOptions::new().map(&file)? };
            assert_eq!(b"# memmap", &mmap[0..8]);
            */

            let mut fp = fs::File::open(entry.path()).unwrap();
            let mmap = unsafe { MmapOptions::new().map(&fp).unwrap() };
            //let mut data = Vec::new();
            //fp.read_to_end(&mut data).unwrap();

            if mmap[0] == 1u8 {
                decode_pic(&path, &mmap);
            }
        }
    }
}
