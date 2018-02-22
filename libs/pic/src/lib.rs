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

extern crate image;

use image::ImageBuffer;

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

pub fn decode_pic(data: &[u8]) -> Result<ImageBuffer> {
    panic!("do the thing")
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
