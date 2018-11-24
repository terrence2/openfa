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

extern crate failure;
extern crate image;
extern crate reverse;

use failure::{ensure, Fallible};
use std::cmp;
use std::{mem, str};

pub struct Sample {
    kind: u8,
    modifiers: u8,
    pub height: u8,
}

impl Sample {
    fn new(kind: u8, modifiers: u8, height: u8) -> Self {
        assert!(
            kind == 0xFF
                || kind == 0xD0
                || kind == 0xD1
                || kind == 0xD2
                || kind == 0xD3
                || kind == 0xD4
                || kind == 0xD5
                || kind == 0xD7
                || kind == 0xD6
                || kind == 0xD8
                || kind == 0xD9
                || kind == 0xDA
                || kind == 0xC2
                || kind == 0xC4
                || kind == 0xC5
                || kind == 0xC6
                || kind == 0xC7
        );
        assert!(modifiers <= 14 || modifiers == 16);
        Sample {
            kind,
            modifiers,
            height,
        }
    }
}

pub struct Terrain {
    pub name: String,
    pub pic_file: String,
    pub width: usize,
    pub height: usize,
    pub samples: Vec<Sample>,
    _extra: Vec<u8>,
}
const MAGIC: &[u8] = &[b'B', b'I', b'T', b'2'];

fn read_name(n: &[u8]) -> Fallible<String> {
    let end_offset: usize = n.iter().position(|&c| c == 0).unwrap_or(n.len() - 1);
    Ok(str::from_utf8(&n[..end_offset])?.to_owned())
}

impl Terrain {
    pub fn from_bytes(data: &[u8]) -> Fallible<Self> {
        let magic = &data[0..4];
        ensure!(magic == MAGIC, "missing magic");

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
        let width = dwords[0] as usize;
        let height = dwords[1] as usize;
        let npix = width * height;

        // Followed by many 3-byte entries.
        // How many? 4 fewer than 258 * 258 (for the normal size).
        // Probably not a coincidence.
        let data_start = 84 + 16 + 49;
        let data_end = data_start + npix * 3;
        let entries = &data[data_start..data_end];
        let mut samples = Vec::new();

        for i in 0..npix {
            let kind = entries[i * 3];
            let mods = entries[i * 3 + 1];
            let height = entries[i * 3 + 2];
            samples.push(Sample::new(kind, mods, height))
        }
        let extra = data[data_end..].to_owned();
        let terrain = Terrain {
            name,
            pic_file,
            width,
            height,
            samples,
            _extra: extra,
        };
        Ok(terrain)
    }

    fn make_debug_images(&self, path: &str) -> Fallible<()> {
        let mut metabuf = image::ImageBuffer::new(self.width as u32, self.height as u32);
        let mut heightbuf = image::ImageBuffer::new(self.width as u32, self.height as u32);
        for (pos, sample) in self.samples.iter().enumerate() {
            let mut metaclr = if sample.modifiers == 16 {
                image::Rgb {
                    data: [255, 0, 255],
                }
            } else {
                image::Rgb {
                    data: [
                        sample.modifiers * 18,
                        sample.modifiers * 18,
                        sample.modifiers * 18,
                    ],
                }
            };
            if sample.kind == 0xFF {
                if sample.modifiers <= 1 {
                    metaclr.data[2] = 0xFF;
                } else {
                    metaclr.data = [0xff, 0x00, 0xff];
                }
            }
            let w = (pos % self.width) as u32;
            let h = (self.height - (pos / self.width) - 1) as u32;
            metabuf.put_pixel(w, h, metaclr);
            heightbuf.put_pixel(
                w,
                h,
                image::Rgb {
                    data: [
                        cmp::min(255usize, sample.height as usize * 4) as u8,
                        cmp::min(255usize, sample.height as usize * 4) as u8,
                        cmp::min(255usize, sample.height as usize * 4) as u8,
                    ],
                },
            );
        }

        let img = image::ImageRgb8(metabuf);
        img.save(path.to_owned() + ".meta.png")?;

        let img = image::ImageRgb8(heightbuf);
        img.save(path.to_owned() + ".height.png")?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::fs;
    use std::io::prelude::*;

    #[test]
    fn it_works() -> Fallible<()> {
        let mut rv: Vec<String> = Vec::new();
        let paths = fs::read_dir("./test_data")?;
        for i in paths {
            let entry = i?;
            let path = format!("{}", entry.path().display());
            if path.ends_with("T2") {
                let mut fp = fs::File::open(entry.path())?;
                let mut data = Vec::new();
                fp.read_to_end(&mut data)?;
                let terrain = Terrain::from_bytes(&data)?;
                assert!(terrain.pic_file.len() > 0);
                terrain.make_debug_images(&path)?;
            }
        }
        rv.sort();

        for v in rv {
            println!("{}", v);
        }

        Ok(())
    }
}
