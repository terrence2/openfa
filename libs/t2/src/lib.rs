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

// Each T2 file has the following format.
//
// magic:      [u8;4]  => "BIT2" in ascii
// name/descr: [u8;80] => "The Baltics", "North/South Korea", etc.
// pic_file:   [u8;16] => "bal.PIC"
//
// Followed by some numbers. I'm not sure if the pic_file portion is 15 bytes or 16 bytes. If it
// is actually 15, that would make the next fields (typically) 0x2000, instead of 32. In any case,
// we have to have an extra pad byte somewhere because the "pixels" are absolutely at 49 bytes
// offset from a 16 byte pic file size. Weird alignment.
// 0            4            8            12           16          20  21
// 20 00 00 00  00 00 00 00  20 00 00 00  00 00 00 00  00 00 00 00 00  08 00 00
//
// 24  25       28  29       32  33     35      37           41           45
// 00  20 00 00 00  20 00 00 00  95 00  03 00   00 01 00 00  00 01 00 00  95 00 00 00   FF 01 00
//
// unknown: [u8;41]
// width: u32
// height: u32
// pixels: [[u8;3]; width * (height + 1)]
//
// Height "pixels" are stored bottom to top. There is one extra row containing random looking data.
// I'm not sure if this is some arcane internal detail or vital extra global information. The data
// stored in the extra row does appear to be mostly the same as the pixel format, so maybe it's just
// scratch or overflow for the rendering process? Each height pixel contains 3 bytes, each a field
// of sorts.
//
// Pixel format:
//   kind: u8 =>  0xFF for water, or 0xDX or 0xCX for land. I'm not sure what the bottom nibble is.
//                It is 2 for almost all maps. Some have 0-A here. Only the Vietnam map has 0xC in
//                the top nibble. I'll need to map these and see if there are any correspondences.
//   flags: u8 => appears to modify the section of land or water. Seems to correspond to terrain
//                features or buildings. Water is mostly 0 near-shores and 1 when away from land.
//                This is probably meant to control if we draw wave.sh on it or not. There are also
//                3 to 7 for some maps, maybe naval bases? Land has a wider array of options, but
//                still only 0-E. Only Vietnam has 0x10, and these are dots. Maybe AckAck or SAM
//                emplacements?
//    height: u8 => Seems to only go up to 40 or so at tallest. Not sure why more resolution was
//                  not employed here. Seems a waste. Graphed out, whiteness seems to line up with
//                  the taller points, so I'm pretty sure this is just a simple height-map.

/* Header usage:
Mostly D2. Some maps have D0 -> DA.
Only Viet has C2->C7.
Pakistan          D2
Persian Gulf      D2
Panama            D2
North Vietnam     C2, C4, C5, C6, C7
North/South Korea D2
Iraq              D2
Taiwan            D2
Greece            D2
Egypt             D0, D1, D2, D3, D4, D5, D6, D7, D8
France            D0, D1, D2, D3, D4, D5, D6, D7, D8
Cuba              D2
Vladivostok       D0, D1, D2, D3, D4, D5, D6, D7, D8
The Baltics       D2
Falkland Islands  D2
Kuril Islands     D0, D1, D2, D3, D4, D5, D6, D7, D8
Ukraine           D0, D2, D3, D4, D6, D7, D8, D9
t
*/

/* Flag byte usage on land
// 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14 || 16
Pakistan           {4, 3, 2, 0}
Persian Gulf       {2, 4, 0, 6, 3}
Panama             {8, 0, 10, 11, 9, 12, 7}
North Vietnam      {12, 0, 9, 7, 10, 16}
North/South Korea  {12, 8, 7, 0, 10, 11}
Iraq               {4, 3, 6, 0, 2}
Taiwan             {13, 7, 8, 14, 6, 12, 9, 0}
Greece             {2, 4, 0, 3}
Egypt              {0, 3, 2, 4, 6}
France             {4, 6, 2, 3, 0}
Cuba               {10, 8, 9, 7, 0}
Vladivostok        {2, 3, 4, 0, 6}
The Baltics        {3, 0, 2, 4}
Falkland Islands   {1, 3, 0, 4}
Kuril Islands      {0, 2}
Ukraine            {2, 4, 6, 3, 0, 5}
*/

/* Flag byte usage on water
// 0, 1 + 2, 3, 4 || 7, 8, 9
Pakistan           {0, 1}
Persian Gulf       {2, 4, 1, 0}
Panama             {1, 0}
North Vietnam      {1, 0}
North/South Korea  {1, 0}
Iraq               {0, 1}
Taiwan             {7, 8, 1, 0, 9}
Greece             {0, 1, 4}
Egypt              {0, 1}
France             {1, 0}
Cuba               {0, 1}
Vladivostok        {0, 1}
The Baltics        {0, 1}
Falkland Islands   {3, 1, 0}
Kuril Islands      {0, 1}
Ukraine            {0, 1}
*/

#[macro_use] extern crate failure;
#[macro_use] extern crate packed_struct;
extern crate reverse;
extern crate image;

use failure::Error;
use reverse::bs2s;
use std::{mem, str};
use std::fs;
use std::io::prelude::*;
use std::collections::HashSet;

pub struct Terrain {
    pub name: String,
    pub pic_file: String,
    data: Vec<u8>
}

const MAGIC: &[u8] = &['B' as u8, 'I' as u8, 'T' as u8, '2' as u8];

fn read_name(n: &[u8]) -> Result<String, Error> {
    let end_offset: usize = n.iter().position(|&c| c == 0).unwrap_or(n.len() - 1);
    return Ok(str::from_utf8(&n[..end_offset])?.to_owned());
}

impl Terrain {
    fn from_bytes(path: &str, data: &[u8]) -> Result<Self, Error> {
        let magic = &data[0..4];
        assert_eq!(magic, MAGIC);

        // Followed by 80 bytes of name / description.
        let name = read_name(&data[4..84])?;

        // Followed by __ bytes containing the pic file.
        // TODO: I'm not super sure if this is 15 bytes or 16 bytes. If it's 16 bytes then
        //       the u32 after is 0x20. If it's 15 bytes then it's 0x2000. We need to lose
        //       one byte between this and the w/h and this might be the right place to do
        //       it. I cannot yet tell from context.
        let pic_file = read_name(&data[84..84 + 16])?;

        // Followed by some numbers... let's skip past those for now.
        // 0            4            8            12           16          20  21
        // 20 00 00 00  00 00 00 00  20 00 00 00  00 00 00 00  00 00 00 00 00  08 00 00
        //
        // 24  25       28  29       32  33     35      37           41           45
        // 00  20 00 00 00  20 00 00 00  95 00  03 00   00 01 00 00  00 01 00 00  95 00 00 00   FF 01 00
        let dwords: &[u32] = unsafe { mem::transmute(&data[84 + 16 + 37..]) };
        let width = dwords[0];
        let height = dwords[1];
        let mut npix = width * height;

        let mut imgbuf = image::ImageBuffer::new(width, height);

        // Followed by many 3-byte entries.
        // How many? 4 fewer than 258 * 258 (for the normal size).
        // Probably not a coincidence.
        let mut entries = &data[84 + 16 + 49..];
        let mut offset = 0;
        let mut min = u16::max_value();
        let mut max = u16::min_value();
        let mut fl = HashSet::new();
        while offset < entries.len() {
            let pos = (offset / 3usize) as u32;
            if pos < npix {
                let hdr = entries[offset];
                assert!(hdr == 0xFF ||
                        hdr == 0xD0 ||
                        hdr == 0xD1 ||
                        hdr == 0xD2 ||
                        hdr == 0xD3 ||
                        hdr == 0xD4 ||
                        hdr == 0xD5 ||
                        hdr == 0xD7 ||
                        hdr == 0xD6 ||
                        hdr == 0xD8 ||
                        hdr == 0xD9 ||
                        hdr == 0xDA ||
                        hdr == 0xC2 ||
                        hdr == 0xC4 ||
                        hdr == 0xC5 ||
                        hdr == 0xC6 ||
                        hdr == 0xC7);
                if hdr != 0xFF {
                    fl.insert(hdr);
                }

                // 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14 || 16
                let flag = entries[offset + 1];
                assert!(flag <= 14 || flag == 16);

                // 0, 1, 2, 3, 4 || 7, 8 || 10
//                if hdr == 0xFF {
//                    fl.insert(flag);
//                }

//                let mut clr = image::Rgb { data: [
//                    entries[offset + 2],
//                    entries[offset + 2],
//                    entries[offset + 2],
//                ] };
                let mut clr = if flag == 16 {
                    image::Rgb { data: [ 255, 0, 255 ] }
                } else {
                    image::Rgb { data: [flag * 18, flag * 18, flag * 18 ] }
                };
                if hdr == 0xFF {
                    if flag <= 1 {
                        clr.data[2] = 0xFF;
                    } else {
                        clr.data = [0xff, 0x00, 0xff];
                    }
                }
                imgbuf.put_pixel(pos % width, height - (pos / width) - 1, clr);
            }

            offset += 3;
        }
        //println!("CNT: {}", entries.len() / 3);
        let cnt = entries.len() / 3;
        let sqrt = (cnt as f64).sqrt();

        let img = image::ImageRgb8(imgbuf);
        let ref mut fout = fs::File::create(path.to_owned() + ".png").unwrap();
        img.save(fout, image::PNG).unwrap();

        let mut foo = fl.iter().collect::<Vec<_>>();
        foo.sort();
        let foo = foo.iter().map(|f| format!("{:2X}", f)).collect::<Vec<_>>();

        println!("{:30}| {:10}| cnt:{} => {:.4} ; {}x{} ; {}/{} | {:?}",
                 name, pic_file, cnt, sqrt, width, height, min, max, foo);// bs2s(&data[84+16+49..]));
        return Ok(Terrain {
            name,
            pic_file,
            data: (&data[84..]).to_owned()
        });
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::prelude::*;
    use super::*;

    #[test]
    fn it_works() {
        let mut rv: Vec<String> = Vec::new();
        let paths = fs::read_dir("./test_data").unwrap();
        for i in paths {
            let entry = i.unwrap();
            let path = format!("{}", entry.path().display());
            if path.ends_with("T2") {
                //println!("AT: {}", path);
                let mut fp = fs::File::open(entry.path()).unwrap();
                let mut data = Vec::new();
                fp.read_to_end(&mut data).unwrap();
                let terrain = Terrain::from_bytes(&path, &data);
            }
        }
        rv.sort();

        for v in rv {
            println!("{}", v);
        }
    }
}
