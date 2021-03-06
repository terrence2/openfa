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
// Height "pixels" are stored top to bottom in BIT2 and in one of two strange block formats in
// BITE. There is generally some extra random looking data after the row data. I'm not sure if
// this is some arcane internal detail or vital extra global information. The data stored in the
// extra row does appear to be mostly the same as the pixel format, so maybe it's just scratch or
// overflow for the rendering process? Each height pixel contains 3 bytes, each a field of sorts.
//
// Pixel format:
//   color: u8 =>  0xFF (transparent) for water, or 0xDX or 0xCX for land. These are all mapped to
//                 FF00FF in the default palette. Palette data from LAY files need to be overlayed
//                 into the palette before it is used. The limited color range is probably because
//                 the palette is used to simulate time-of-day; selecting a full and realistic
//                 sunset and sunrise ramp for lots of colors would have been hugely difficult.
//   flags: u8 => appears to modify the section of land or water. Seems to correspond to terrain
//                features or buildings. Water is mostly 0 near-shores and 1 when away from land.
//                This is probably meant to control if we draw wave.sh on it or not. There are also
//                3 to 7 for some maps, maybe naval bases? Land has a wider array of options, but
//                still only 0-E. Only Vietnam has 0x10, and these are dots. Maybe AckAck or SAM
//                emplacements?
//    height: u8 => Seems to only go up to 40 or so at tallest. Not sure why more resolution was
//                  not employed here. Seems a waste. Graphed out, whiteness seems to line up with
//                  the taller points, so I'm pretty sure this is just a simple height-map.

/* color byte usage:
Mostly D2. Some maps have D0 -> DA.
Only Viet has C2->C7.

These are palette indexes into a part of the palette that is not filled in, in the default palette.
These parts of the palette need to be loaded in from LAY files. Presumably, this is so that the
game can change teh palette to simulate sunrise, sunset, and nighttime, as well as different fog
levels and probably other things, just by swapping around the palette.

At a guess, the newer maps only have a single color either because they were expecting most or all
users to be able to run with texture mapping by 1998.

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
#![allow(clippy::transmute_ptr_to_ptr)]

use failure::{bail, ensure, Fallible};
use lazy_static::lazy_static;
use log::trace;
use packed_struct::packed_struct;
use std::{collections::HashMap, mem, str};

// Lat/Lon of lower left corner of every map that is shipped with FA.
// TODO: 3rd party maps will need a way to specify. For now we will use a default.
lazy_static! {
    static ref MAP_POSITIONS: HashMap<&'static str, [f32; 2]> = {
        let mut table = HashMap::new();
        table.insert("Panama", [11.77, -82.86]);
        table.insert("The Baltics", [63.60, 21.20]);
        table.insert("Cuba", [26.11, -85.43]); // ^^ UL ^^
        table.insert("Egypt", [33.54, 30.5]);
        table.insert("France", [53.97, 0.04]);
        table.insert("Greece", [41.84, 21.04]);
        table.insert("Iraq", [33.44, 44.75]);
        table.insert("Kuril Islands", [52.53, 146.82]); // vv LL vv
        table.insert("Ukraine", [48.50, 24.1]);
        table.insert("Taiwan", [27.93, 117.6]);
        table
    };
}

#[derive(Copy, Clone)]
pub struct Sample {
    pub color: u8,
    pub modifiers: u8,
    pub height: u8,
}

impl Default for Sample {
    fn default() -> Self {
        Self {
            color: 0xD0,
            modifiers: 0,
            height: 0,
        }
    }
}

impl Sample {
    fn from_bytes(data: &[u8]) -> Self {
        Self {
            color: data[0],
            modifiers: data[1],
            height: data[2],
        }
    }

    fn new(color: u8, modifiers: u8, height: u8) -> Self {
        assert!(
            color == 0xFF
                || color == 0xD0
                || color == 0xD1
                || color == 0xD2
                || color == 0xD3
                || color == 0xD4
                || color == 0xD5
                || color == 0xD7
                || color == 0xD6
                || color == 0xD8
                || color == 0xD9
                || color == 0xDA
                || color == 0xC2
                || color == 0xC4
                || color == 0xC5
                || color == 0xC6
                || color == 0xC7
        );
        assert!(modifiers <= 14 || modifiers == 16);
        Sample {
            color,
            modifiers,
            height,
        }
    }
}

pub struct Terrain {
    name: String,
    _pic_file: String,
    width: u32,
    height: u32,
    width_ft: f32,
    height_ft: f32,
    origin_latitude: f32,
    origin_longitude: f32,
    pub samples: Vec<Sample>,
}

impl Terrain {
    pub fn from_bytes(data: &[u8]) -> Fallible<Self> {
        let magic = &data[0..4];
        if magic == MAGIC_BITE {
            return Self::from_bite(data);
        }
        if magic == MAGIC_BIT2 {
            return Self::from_bit2(data);
        }

        bail!(
            "do not know how to parse a T2 with magic header of {:?}",
            magic
        )
    }

    fn from_bite(data: &[u8]) -> Fallible<Self> {
        // Between USNF and MF, the format of the header changed without changing the
        // magic BITE, so we need to do a bit of digging to find out which header to use.
        // The newer format adds a description, so if there is a .PIC after the magic
        // then it is the older format.
        let maybe_pic = read_name(&data[4..19])?;
        if maybe_pic.ends_with(".PIC") {
            return Self::from_bite0(data);
        }
        Self::from_bite1(data)
    }
}

fn read_name(n: &[u8]) -> Fallible<String> {
    let end_offset: usize = n.iter().position(|&c| c == 0).unwrap_or(n.len() - 1);
    Ok(str::from_utf8(&n[..end_offset])?.to_owned())
}

const MAGIC_BITE: &[u8] = &[b'B', b'I', b'T', b'E'];

/*
The earlier BITE format has the pic right after the magic and a big
string of 0 bytes in the middle of the important bits.

After USNF, it looks like the BITE format changed without changing the
magic word. Now there is a description / name after the magic and the pic
is moved lower. There is also a big block of zeros removed from the middle.
*/
packed_struct!(BITEHeader0 {
    _0 => magic: [u8; 4],

    _1 => pic_file: [u8; 16],

    _2a => pad0a: [u8; 32],
    _2b => pad0b: [u8; 6],

    _3 => unk0: u32,
    _4 => unk1: u32,
    _5 => unk2: u32,
    _6 => unk3: u32,

    _7a => pad1a: [u8; 32],
    _7b => pad1b: [u8; 32],
    _7c => pad1c: [u8; 13],

    _9 => unk4: u16,

    _a => width_ft: u16,
    _b => height_ft: u16,

    _c => unk5: [u8; 5],

    _d => width: u16 as usize,
    _e => height: u16 as usize,

    _f => block_size: u16 as usize,
    _10 => block_count_x: u16 as usize,
    _11 => block_count_z: u16 as usize
});

impl Terrain {
    // This format is used exclusively by the first game in the series: USNF.
    // It has the same BITE header as in later games, but is missing the name
    // field.
    fn from_bite0(data: &[u8]) -> Fallible<Self> {
        let header_ptr: *const BITEHeader0 = data.as_ptr() as *const _;
        let header: &BITEHeader0 = unsafe { &*header_ptr };
        ensure!(header.magic() == MAGIC_BITE, "missing magic");
        ensure!(header.pad0a() == [0u8; 32], "pad0a");
        ensure!(header.pad0b() == [0u8; 6], "pad0b");
        ensure!(header.pad1a() == [0u8; 32], "pad1a");
        ensure!(header.pad1b() == [0u8; 32], "pad1b");
        ensure!(header.pad1c() == [0u8; 13], "pad1c");

        let pic_file = read_name(&header.pic_file())?;

        trace!(
            "T2:BIT0: {:08X} {:08X} {:08X} {:08X} - {:04X} {:04X} {:04X} - {:?} - {}x{} [{}, {}, {}]",
            header.unk0(),
            header.unk1(),
            header.unk2(),
            header.unk3(),
            header.unk4(),
            header.width_ft(),
            header.height_ft(),
            header.unk5(),
            header.width(),
            header.height(),
            header.block_size(),
            header.block_count_x(),
            header.block_count_z(),
        );

        // We can now skip the row offsets block to get to the height entries.
        let offsets_start = mem::size_of::<BITEHeader0>();
        let offsets_size = header.height() * mem::size_of::<u32>();
        let num_pix = header.width() * header.height();
        let data_start = offsets_start + offsets_size;
        let entries = &data[data_start..];

        let blk_size = header.block_size();
        ensure!(blk_size == 16, "expect block size of 16");
        let block_count_z = header.block_count_z();
        let block_count_x = header.block_count_x();
        ensure!(block_count_x == block_count_z, "only support square maps");

        let mut samples = vec![Default::default(); num_pix];

        ensure!(block_count_x == 13, "can't handle other sizes");
        let mut off = 12 * 3; // Looks like there's 4 uints?
        for blkz in 0..block_count_z {
            for blkx in 0..block_count_x {
                // For each pixel in the block from bottom to top...
                for j in 0..blk_size {
                    for i in 0..blk_size {
                        let data = &entries[off..off + 3];
                        off += 3;
                        let mut x_pos = blkx * blk_size + i;
                        let mut z_pos = blkz * blk_size + (j + 4) % 16;
                        if j >= 12 {
                            x_pos = (x_pos + 16) % 208;
                            if blkx == 12 {
                                z_pos = (z_pos + 16) % 208;
                            }
                        }
                        let index = z_pos * header.width() as usize + x_pos;
                        samples[index] = Sample::from_bytes(data);
                    }
                }
            }
        }

        let name = "Ukraine".to_owned();
        let [lat_deg, lon_deg] = MAP_POSITIONS.get::<str>(&name).unwrap_or(&[0f32, 0f32]);

        let terrain = Terrain {
            name,
            _pic_file: pic_file,
            width_ft: ((header.width_ft() as u32) << 8) as f32,
            height_ft: ((header.height_ft() as u32) << 8) as f32,
            width: header.width() as u32,
            height: header.height() as u32,
            origin_latitude: *lat_deg,
            origin_longitude: *lon_deg,
            samples,
        };
        Ok(terrain)
    }
}

packed_struct!(BITEHeader1 {
    _0 => magic: [u8; 4],

    // Note: we have to split this up because Debug is only
    // implemented up through array sizes of 32.
    _1a => name0: [u8; 32],
    _1b => name1: [u8; 32],
    _1c => name2: [u8; 16],

    _3 => pic_file: [u8; 15],

    _4 => unk0: [u32; 5],
    _9 => unk_pad0: [u8; 1],
    _10 => unk_a: u16,

    _11 => width_ft: u16,
    _12 => height_ft: u16,

    _9a => unk_after: [u8; 5],

    _14 => width: u16 as usize,
    _15 => height: u16 as usize,

    _16 => block_size: u16 as usize,
    _17 => block_count_z: u16 as usize,
    _18 => block_count_x: u16 as usize
});

impl Terrain {
    fn from_bite1(data: &[u8]) -> Fallible<Self> {
        let header_ptr: *const BITEHeader1 = data.as_ptr() as *const _;
        let header: &BITEHeader1 = unsafe { &*header_ptr };
        ensure!(header.magic() == MAGIC_BITE, "missing magic");

        let name = read_name(&header.name0())?
            + &read_name(&header.name1())?
            + &read_name(&header.name2())?;
        let pic_file = read_name(&header.pic_file())?;
        let [lat_deg, lon_deg] = MAP_POSITIONS.get::<str>(&name).unwrap_or(&[0f32, 0f32]);

        trace!(
            "T2:BITE1: {:?} {:?} {:04X} {:?}- {}x{} ({:04X}x{:04X}ft) [{}, {}, {}]",
            header.unk0(),
            header.unk_pad0(),
            header.unk_a(),
            header.unk_after(),
            header.width(),
            header.height(),
            header.width_ft(),
            header.height_ft(),
            header.block_size(),
            header.block_count_z(),
            header.block_count_x(),
        );

        // We can now skip the row offsets block to get to the height entries.
        let offsets_start = mem::size_of::<BITEHeader1>();
        let offsets_size = header.height() * mem::size_of::<u32>();
        let num_pix = header.width() * header.height();
        let data_start = offsets_start + offsets_size;
        let entries = &data[data_start..];

        let blk_size = header.block_size();
        ensure!(blk_size == 16, "expect block size of 16");
        let block_count_z = header.block_count_z();
        let block_count_x = header.block_count_x();
        ensure!(block_count_x == block_count_z, "only support square maps");

        // For each block in the input.
        let mut samples = vec![Default::default(); num_pix];

        if block_count_x == 16 {
            // This loop works for 16x16 block maps (BAL/KURILE)
            let mut off = 0;
            for blkz in 0..block_count_z {
                for blkx in 0..block_count_x {
                    // For each pixel in the block from bottom to top...
                    for j in 0..blk_size {
                        for i in 0..blk_size {
                            let data = &entries[off..off + 3];
                            off += 3;
                            let x_pos = blkx * blk_size + i;
                            let z_pos = blkz * blk_size + j;
                            let index = z_pos * header.width() as usize + x_pos;
                            samples[index] = Sample::from_bytes(data);
                        }
                    }
                }
            }
        } else {
            // This loop handles 13x13 block maps (NOT BAL/KURILE)
            ensure!(block_count_x == 13, "can't handle other sizes");
            let mut off = 12 * 3; // Looks like there's 4 uints?
            for blkz in 0..block_count_z {
                for blkx in 0..block_count_x {
                    // For each pixel in the block from bottom to top...
                    for j in 0..blk_size {
                        for i in 0..blk_size {
                            let data = &entries[off..off + 3];
                            off += 3;
                            let mut x_pos = blkx * blk_size + i;
                            let mut z_pos = blkz * blk_size + (j + 4) % 16;
                            if j >= 12 {
                                x_pos = (x_pos + 16) % 208;
                                if blkx == 12 {
                                    z_pos = (z_pos + 16) % 208;
                                }
                            }
                            let index = z_pos * header.width() as usize + x_pos;
                            samples[index] = Sample::from_bytes(data);
                        }
                    }
                }
            }
        }

        let terrain = Terrain {
            name,
            _pic_file: pic_file,
            width_ft: ((header.width_ft() as u32) << 8) as f32,
            height_ft: ((header.height_ft() as u32) << 8) as f32,
            width: header.width() as u32,
            height: header.height() as u32,
            origin_latitude: *lat_deg,
            origin_longitude: *lon_deg,
            samples,
        };
        Ok(terrain)
    }
}

const MAGIC_BIT2: &[u8] = &[b'B', b'I', b'T', b'2'];

packed_struct!(BIT2Header {
    _0 => magic: [u8; 4],

    // Actually 80 bytes, but split up because Debug is not implemented for arrays past 32.
    _1a => name0: [u8; 32],
    _1b => name1: [u8; 32],
    _1c => name2: [u8; 16],

    _2 => pic_file:  [u8; 15],

    _3 => unk0: [u32; 6],

    _4 => width_ft: u32,
    _5 => height_ft: u32,

    _6 => unk_zero: u16,
    _7 => unk1: u16,
    _8 => unk_small: u16,

    _12 => width: u32,
    _13 => height: u32,

    _14 => unk2: u32

    // data
});

impl Terrain {
    fn from_bit2(data: &[u8]) -> Fallible<Self> {
        let header_pointer: &[BIT2Header] = unsafe { mem::transmute(data) };
        let header = &header_pointer[0];

        // 4 byte of magic
        ensure!(header.magic() == MAGIC_BIT2, "missing magic");

        // 80 bytes of name / description
        let name = read_name(&header.name0())?
            + &read_name(&header.name1())?
            + &read_name(&header.name2())?;
        let [lat_deg, lon_deg] = MAP_POSITIONS.get::<str>(&name).unwrap_or(&[0f32, 0f32]);

        // Followed by 15 bytes containing the pic file.
        let pic_file = read_name(&header.pic_file())?;
        trace!("Loaded T2 with name: {}, pic_file: {}", name, pic_file);

        // Followed by a bunch of ints.
        ensure!(header.unk0()[1] == 0, "expected 0 in unk0[1]");
        ensure!(header.unk0()[3] == 0, "expected 0 in unk0[3]");
        ensure!(header.unk0()[4] == 0, "expected 0 in unk0[4]");
        ensure!(header.unk0()[5] == 524_288, "expected 524288 in unk0[5]");
        if header.unk_small() == 3 {
            ensure!(header.width() == 256, "if 3, expect 256");
            ensure!(header.height() == 256, "if 3, expect 256");
        }
        trace!(
            "unk: {:?} {:08X} {:?}; {}x{} (0x{:06X}x{:06X}ft)",
            header.unk0(),
            header.unk1(),
            header.unk_small(),
            header.width(),
            header.height(),
            header.width_ft(),
            header.height_ft(),
        );

        // Followed by many 3-byte entries.
        let npix = (header.width() * header.height()) as usize;
        let data_start = mem::size_of::<BIT2Header>();
        let data_end = data_start + npix * 3;
        let entries = &data[data_start..data_end];
        let mut samples = Vec::new();
        for i in 0..npix {
            let color = entries[i * 3];
            let modifiers = entries[i * 3 + 1];
            let height = entries[i * 3 + 2];
            samples.push(Sample::new(color, modifiers, height))
        }

        let terrain = Terrain {
            name,
            _pic_file: pic_file,
            width_ft: header.width_ft() as f32,
            height_ft: header.height_ft() as f32,
            width: header.width(),
            height: header.height(),
            origin_latitude: *lat_deg,
            origin_longitude: *lon_deg,
            samples,
        };
        Ok(terrain)
    }
}

impl Terrain {
    /*
    The french map is:
        Total (/208):
           miles ->      290 (1.4)
           meters -> 466,710 (2243)
           feet -> 1,531,000 (7360)

           miles ->      300 (1.44)
           meters -> 482,803 (2321)
           feet -> 1,584,000 (7615)

    Possible scales:
        0x00000008 => 8
        0x00000800 => 2048
        0x00080000 => 524288

        0x00000019 => 25
        0x00001900 => 6400
        0x00190000 => 1638400    <- most likely unit is feet
        0x19000000 => 419,430,400

        0x0000001a => 26
        0x00001a00 => 6656
        0x001a0000 => 1703936

    The cuban map is:
        Size: 256x256
        miles:      343 (1.34)
        meters: 552,005 (2,156)
        feet: 1,811,040 (7,074)

     Possible scales:
        0x0000001a => 26
        0x00001a00 => 6656
        0x001a0000 => 1703936   <- most likely is again feet
    */
    #[cfg(test)]
    fn make_debug_images(&self, path: &str) -> Fallible<()> {
        use std::cmp;

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
            if sample.color == 0xFF {
                if sample.modifiers <= 1 {
                    metaclr.data[2] = 0xFF;
                } else {
                    metaclr.data = [0xff, 0x00, 0xff];
                }
            }
            let w = (pos % self.width as usize) as u32;
            let h = (self.height as usize - (pos / self.width as usize) - 1) as u32;
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

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn extent_east_west_in_ft(&self) -> f32 {
        self.width_ft
    }

    pub fn extent_north_south_in_ft(&self) -> f32 {
        self.height_ft
    }

    pub fn origin_latitude(&self) -> f32 {
        self.origin_latitude
    }

    pub fn origin_longitude(&self) -> f32 {
        self.origin_longitude
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use lib::CatalogBuilder;

    const DUMP: bool = false;

    #[test]
    fn it_can_parse_all_t2_files() -> Fallible<()> {
        let (mut catalog, inputs) = CatalogBuilder::build_and_select(&["*:*.T2".to_owned()])?;
        for &fid in &inputs {
            let label = catalog.file_label(fid)?;
            catalog.set_default_label(&label);
            let game = label.split(':').last().unwrap();
            let meta = catalog.stat_sync(fid)?;
            println!(
                "At: {}:{:13} @ {}",
                game,
                meta.name,
                meta.path
                    .unwrap_or_else(|| "<none>".into())
                    .to_string_lossy()
            );
            let contents = catalog.read_sync(fid)?;
            let terrain = Terrain::from_bytes(&contents)?;
            if DUMP {
                terrain.make_debug_images(&format!("../../dump/t2/{}_{}", game, meta.name))?;
            }
        }

        Ok(())
    }
}
