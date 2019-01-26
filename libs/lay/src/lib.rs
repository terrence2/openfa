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
use failure::{ensure, Fallible};
use lib::Library;
use packed_struct::packed_struct;
use pal::Palette;
use std::{fs, mem, str, sync::Arc};
//use reverse::bs2s;

packed_struct!(LayerHeader {
     _0 => unkPtr00x100: u32, // Ramp
     _1 => unkPtr01: u32,
     _2 => unkPtr02: u32,
     _3 => unkPtr03: u32,
     _4 => unkPtr04: u32,
     _5 => seven: u32,
     _6 => unkPtr20x100: u32, // Ramp
     _7 => unkPtr21x100: u32,
     _8 => unkPtr22x100: u32,
     _9 => unkPtr23x100: u32,
    _10 => unkPtr24x100: u32,
    _11 => unkPtr25x100: u32,
    _12 => unkPtr26x100: u32,
    _13 => zero30: u32,
    _14 => zero31: u32,
    _15 => zero32: u32,
    _16 => six: u32,
    _17 => unkPtr50x100: u32, // Ramp
    _18 => unkPtr51x100: u32,
    _19 => unkPtr52x100: u32,
    _20 => unkPtr53x100: u32,
    _21 => unkPtr54x100: u32,
    _22 => unkPtr55x100: u32,
    _23 => unkPtr56x100: u32,
    _24 => zero60: u32,
    _25 => zero61: u32,
    _26 => zero62: u32,
    _27 => unkPtr70: u32,
    _28 => ptrSecond: u32,
    _29 => ptrFirst: u32
});

packed_struct!(LayerPlaneHeader {
    _0 => unkMaybe66: u32,
    _1 => unkMaybe2DD: u32,
    _2 => maybeFogUpdateThunk: u32,
    _3 => horizonProcThunk: u32,
    // FE 1F 38 0E 70 62 00 00 30 0B 01 00 18 47 E8 B8
    _4 => unk1FFE: u16,
    _5 => unk0E38: u16,
    _6 => unk00006270: u32,
    _7 => unk00010B30: u32,
    _8 => unkB8E84718: u32,
    // 8 * 4 bytes => 0x20 bytes
    _9 => unkFillAndShape: [u8; 16],
    // 0x30 bytes
    _10 => unkStuff0: [u8; 0x20],
    // 0x50 bytes
    _11 => unkStuff1: [u8; 0x1F]
    // 0x6F bytes
});

pub struct Layer {
    data: Vec<u8>,
    frag_offsets: Vec<usize>,
    fragments: Vec<Palette>,
}

impl Layer {
    pub fn for_index(&self, index: usize, offset: i32) -> Fallible<Palette> {
        let base = if offset >= 0 {
            self.frag_offsets[index] + (offset as usize)
        } else {
            self.frag_offsets[index] - (-offset as usize)
        };
        let slice = &self.data[base..base + 0xC0];
        Palette::from_bytes(slice)
        //return self.fragments[index].clone();
    }

    pub fn from_bytes(data: &[u8], lib: Arc<Box<Library>>) -> Fallible<Layer> {
        let mut pe = peff::PE::parse(data)?;
        pe.relocate(0x0000_0000)?;
        Layer::from_pe("inval", &pe, lib)
    }

    fn from_pe(prefix: &str, pe: &peff::PE, lib: Arc<Box<Library>>) -> Fallible<Layer> {
        assert!(prefix.len() > 0);

        let mut frag_offsets = Vec::new();
        let mut fragments = Vec::new();

        let pal_data = lib.load("PALETTE.PAL")?;
        let palette_digest = md5::compute(&pal_data);

        let dump_stuff = false;

        if dump_stuff {
            fs::create_dir("dump").unwrap_or(());
            fs::create_dir(format!("dump/{}", prefix)).unwrap_or(());
        }

        let data = &pe.code;

        let header_ptr: *const LayerHeader = data.as_ptr() as *const _;
        let header: &LayerHeader = unsafe { &*header_ptr };

        assert_eq!(header.unkPtr00x100(), header.unkPtr04());
        assert_eq!(header.unkPtr00x100(), header.unkPtr20x100());
        assert!(header.unkPtr00x100() == header.unkPtr50x100() || header.unkPtr00x100() + 0x100 == header.unkPtr50x100());

        // This segment of data counts up from 00 to FF and is present in every single LAY.
        // It makes little sense as a LUT because if you have the value itself? It is always
        // referenced from the first pointer, so maybe it's some weird indirect LUT using
        // the fact that this data is loaded with PE relocation?
        let ramp_addr = header.unkPtr00x100();
        let ramp_data = &data[ramp_addr as usize..ramp_addr as usize + 0x100];
        let ramp_digest = md5::compute(ramp_data);
        assert_eq!(
            &format!("{:x}", ramp_digest),
            "e2c865db4162bed963bfaa9ef6ac18f0"
        );

        let first_size = 0x100 + 46;
        let first_addr = header.ptrFirst();
        let first_data = &data[first_addr as usize..first_addr as usize + first_size];
        let first_pal_data = &first_data[22 * 3 + 2..];
        let first_pal = Palette::from_bytes(&first_pal_data[0..(16 * 3 + 13) * 3])?;
        frag_offsets.push(header.ptrFirst() as usize + 22 * 3 + 2);
        fragments.push(first_pal);
        if dump_stuff {
            // Seems to be correct for CLOUD and FOG, but really dark for DAY.
            let name = format!("dump/{}/first_data", prefix);
            println!("Dumping {}", name);
            Palette::dump_partial(first_pal_data, 4, &(name.clone() + "2"))?;
        }

        let mut plane_offset = 0;
        let mut offset = first_addr as usize + first_size;
        loop {
            let plane_size = 0x160;
            let _pal_size = 0xc1;
            let hdr_size = mem::size_of::<LayerPlaneHeader>();
            // 0xc1 + 0x6f = 0x130 bytes
            // 0x130 / 16 => 19

            if offset + plane_size >= header.ptrSecond() as usize {
                break;
            }

            // This header recurs every 0x160 bytes, 0x146 bytes after header.ptrFirst().
            let plane_data = &data[offset..offset + hdr_size];
            let plane_ptr: *const LayerPlaneHeader = plane_data.as_ptr() as *const LayerPlaneHeader;
            let plane: &LayerPlaneHeader = unsafe { &*plane_ptr };
            ensure!(
                plane.unkMaybe66() == 0x66 || plane.unkMaybe66() == 0,
                "expected 66"
            );
            ensure!(
                plane.unkMaybe2DD() == 0x2DD || plane.unkMaybe2DD() == 0,
                "expected 2DD"
            );
            ensure!(
                plane.maybeFogUpdateThunk() == 0x2A46
                    || plane.maybeFogUpdateThunk() == 0x2646
                    || plane.maybeFogUpdateThunk() == 0,
                "expected 0x2A46 or 0x2646, not 0x{:4X}",
                plane.maybeFogUpdateThunk()
            );
            ensure!(
                plane.horizonProcThunk() != 0,
                "expected horizon proc thunk to be set"
            );
            ensure!(plane.unk1FFE() == 0x1FFE, "expected 1FFE");
            ensure!(plane.unk0E38() == 0x0E38, "expected 0E38");
            ensure!(plane.unk00006270() == 0x6270, "expected 6270");
            ensure!(plane.unk00010B30() == 0x10B30, "expected 10B30");
            ensure!(plane.unkB8E84718() == 0xB8E84718, "expected B8E84718");

            //println!("PLANE9: {}", bs2s(&plane.unkFillAndShape()));
            //println!("PLANE: {}", str::from_utf8(&plane.unkFillAndShape())?);
            //assert!(str::from_utf8(&plane.shape())?.starts_with("wave1.SH"));
            //println!("SHAPE: {}", str::from_utf8(&plane.shape())?);
            let pal_data = &data[offset + hdr_size + 1 - 0..offset + hdr_size + 0xC0 + 1 - 0];
            let pal = Palette::from_bytes(pal_data)?;
            frag_offsets.push(offset + hdr_size + 1usize);
            fragments.push(pal);
            if dump_stuff {
                // Dump after the fixed header... we claim the palette only extends for 0xC1 bytes...
                let name = format!("dump/{}/first-{}", prefix, plane_offset);
                println!("dumping pal+ {}", name);
                // Palette::dump_partial(
                //     &data[offset + hdr_size..offset + hdr_size + 0xC0],
                //     4,
                //     &(name.clone() + "-0"),
                // )?;

                // 0
                //   CLOUD: white
                //   DAY: sunrise or sunset?
                //   FOG: white with blue ramp
                // 1
                //   CLOUD: white with blue ramp
                //   DAY: fully lit
                //   FOG: black
                // 2
                //   CLOUD: black
                //   DAY: sunrise or sunset
                // 3
                //   DAY: very dark, but clearly has color... twilight?
                // 4
                //   DAY: black
                Palette::dump_partial(
                    pal_data,
                    4,
                    &(name.clone() + "-1"),
                )?;

                // Palette::dump_partial(
                //     &data[offset + hdr_size + 2..offset + hdr_size + 0xC2],
                //     4,
                //     &(name.clone() + "-2"),
                // )?;
            }

            // Why 0xc1 bytes here?
            //            let plane_pal: Vec<u8> = data[offset + hdr_size..offset + hdr_size + _pal_size]
            //                .to_owned()
            //                .to_vec();
            //
            //            // for i in 0..0x36 {
            //            //     plane_pal[i] = 0;
            //            // }
            //            let mut pal_b: Vec<u8> = EMPTY_PAL_PLANE.to_owned().to_vec();
            //            pal_b.append(&mut EMPTY_PAL_PLANE.to_owned().to_vec());
            //            pal_b.append(&mut plane_pal.to_owned());
            //            if dump_stuff {
            //                Palette::dump_partial(
            //                    &pal_b,
            //                    3,
            //                    &format!("dump/{}/pal-{}-{:04X}", prefix, "planes", offset + hdr_size),
            //                ).unwrap();
            //            }

            offset += plane_size;
            plane_offset += 1;
        }

        // decode 7 palettes at ptrSecond
        // These are all gray scales. Maybe for the skybox?
        offset = header.ptrSecond() as usize;
        assert_eq!(palette_digest, md5::compute(&data[offset..offset + 0x300]));
        if dump_stuff {
            let name = format!("dump/{}/second-0", prefix);
            println!("dumping {}", name);
            Palette::dump_partial(&data[offset..offset + 0x300], 4, &name)?;
        }
        offset += 0x300;
        for i in 0..18 {
            //let pal_data = &data[offset..offset + 0x100];
            if dump_stuff {
                let name = format!("dump/{}/second-{}", prefix, i + 1);
                println!("dumping {}", name);
                Palette::dump_partial(&data[offset..offset + 0x100], 1, &name)?;
            }
            offset += 0x100;
        }

        Ok(Layer { data: data.to_vec(), frag_offsets, fragments })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omnilib::OmniLib;

    #[test]
    fn it_can_parse_all_lay_files() -> Fallible<()> {
        let omni = OmniLib::new_for_test_in_games(vec![
            "FA", "USNF97", "ATFGOLD", "ATFNATO", "ATF", "MF", "USNF",
        ])?;
        for (game, name) in omni.find_matching("*.LAY")?.iter() {
            println!("At: {}:{} @ {}", game, name, omni.path(game, name)?);
            let lib = omni.library(game);
            let data = lib.load(name)?;
            let _lay = Layer::from_bytes(&data, lib.clone())?;
        }

        return Ok(());
    }
}
