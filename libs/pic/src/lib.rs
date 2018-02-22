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

extern crate failure;
extern crate image;

use std::mem;
//use image::{ImageBuffer, ImageRgba8};
use failure::Error;

#[repr(C)]
#[repr(packed)]
struct Header {
    format: u16,
    width: u32,
    height: u32,
    always_64: u32,
    pixels_size: u32,
    palette_offset: u32,
    palette_size: u32,
    unknown0: u32,
    unknown1: u32,
    rowheads_offset: u32,
    rowheads_size: u32,
    padding: [u8; 22]
}

fn n2h(n: u8) -> char {
    match n {
        0 => '0',
        1 => '1',
        2 => '2',
        3 => '3',
        4 => '4',
        5 => '5',
        6 => '6',
        7 => '7',
        8 => '8',
        9 => '9',
        10 => 'A',
        11 => 'B',
        12 => 'C',
        13 => 'D',
        14 => 'E',
        15 => 'F',
        _ => panic!("expected a nibble, got: {}", n)
    }
}

fn b2h(b: u8, v: &mut Vec<char>) {
    v.push(n2h(b >> 4));
    v.push(n2h(b & 0xF));
}

pub fn decode_pic(data: &[u8]) -> Result<(), Error> {
    let header_ptr: *const Header = data[0..].as_ptr() as *const _;
    let header: &Header = unsafe { &*header_ptr };

    let mut v = Vec::new();
    for &b in &data[mem::size_of::<Header>()..] {
        b2h(b, &mut v);
        v.push(' ');
    }
    let s = v.iter().collect::<String>();
    println!("{:4}x{:4}: {}", header.width, header.height, s);
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

            let mut fp = fs::File::open(entry.path()).unwrap();
            let mut data = Vec::new();
            fp.read_to_end(&mut data).unwrap();

            if data[0] == 1u8 {
                decode_pic(&data);
            }
        }
    }
}
