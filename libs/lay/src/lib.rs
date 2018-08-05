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
#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate failure;
extern crate i386;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
#[macro_use]
extern crate packed_struct;
extern crate md5;
extern crate pal;
extern crate peff;
extern crate reverse;
extern crate simplelog;

use failure::Error;
use pal::Palette;
use reverse::bs2s;
use std::{fs, mem, str};

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
    _4 => unk0E38: u16,
    _5 => unk1FFE: u16,
    _6 => unk00006270: u32,
    _7 => unk00010B30: u32,
    _8 => unkB8E84718: u32,
    // 8 * 4 bytes => 0x20 bytes
    _9 => unkFillAndShape: [u8; 16],
    // 0x30 bytes
    _10 => unkStuff0: [u8; 0x20],
    _11 => unkStuff1: [u8; 0x1F]
});

pub struct Layer {}

const EMPTY_PAL_PLANE: [u8; 0x23F] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

impl Layer {
    pub fn from_pe(prefix: &str, pe: &peff::PE) -> Result<(), Error> {
        println!("IN IN IN: {}", prefix);
        fs::create_dir("dump");
        fs::create_dir(format!("dump/{}", prefix));
        let data = &pe.code;

        let header_ptr: *const LayerHeader = data.as_ptr() as *const _;
        let header: &LayerHeader = unsafe { &*header_ptr };

        assert_eq!(header.unkPtr00x100(), header.unkPtr04());
        assert_eq!(header.unkPtr00x100(), header.unkPtr20x100());
        assert_eq!(header.unkPtr00x100(), header.unkPtr50x100());

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

        let mut offset = first_addr as usize + first_size;
        loop {
            let plane_size = 0x160;
            let pal_size = 0xc1;
            let hdr_size = mem::size_of::<LayerPlaneHeader>();

            if offset + plane_size >= header.ptrSecond() as usize {
                break;
            }

            let plane_data = &data[offset..offset + hdr_size];
            let plane_ptr: *const LayerPlaneHeader = unsafe { mem::transmute(plane_data.as_ptr()) };
            let plane: &LayerPlaneHeader = unsafe { &*plane_ptr };
            //assert!(str::from_utf8(&plane.shape())?.starts_with("wave1.SH"));
            //println!("SHAPE: {}", str::from_utf8(&plane.shape())?);
            let mut plane_pal: Vec<u8> = data[offset + hdr_size..offset + hdr_size + pal_size]
                .to_owned()
                .to_vec();
            // for i in 0..0x36 {
            //     plane_pal[i] = 0;
            // }
            let mut pal_b: Vec<u8> = EMPTY_PAL_PLANE.to_owned().to_vec();
            pal_b.append(&mut EMPTY_PAL_PLANE.to_owned().to_vec());
            pal_b.append(&mut plane_pal.to_owned());
            let pal = Palette::from_bytes(&pal_b)?;
            pal.dump_png(&format!(
                "dump/{}/pal-{}-{:04X}",
                prefix,
                "planes",
                offset + hdr_size
            )).unwrap();

            offset += plane_size;
        }

        // decode 7 palettes at ptrSecond
        offset = header.ptrSecond() as usize;
        let pal_data = &data[offset..offset + 0x300];
        let pal = Palette::from_bytes(pal_data)?;
        pal.dump_png(&format!("dump/{}/pal-second{}-{:04X}", prefix, "0", offset))
            .unwrap();
        offset += 0x300;
        for i in 0..18 {
            let pal_data = &data[offset..offset + 0x100];
            let mut pal_b: Vec<u8> = EMPTY_PAL_PLANE.to_owned().to_vec();
            pal_b.append(&mut EMPTY_PAL_PLANE.to_owned().to_vec());
            pal_b.append(&mut pal_data.to_owned());
            let pal = Palette::from_bytes_prescaled(&pal_b)?;
            pal.dump_png(&format!("dump/{}/pal-second{}-{:04X}", prefix, i, offset))
                .unwrap();
            offset += 0x100;
        }

        /*
        let addrs = vec![
            ("ptr00", header.unkPtr00x100()),
            ("ptr01", header.unkPtr01()),
            ("ptr02", header.unkPtr02()),
            ("ptr03", header.unkPtr03()),
            ("ptr04", header.unkPtr04()),
            ("ptr20", header.unkPtr20x100()),
            ("ptr21", header.unkPtr21x100()),
            ("ptr22", header.unkPtr22x100()),
            ("ptr23", header.unkPtr23x100()),
            ("ptr24", header.unkPtr24x100()),
            ("ptr25", header.unkPtr25x100()),
            ("ptr26", header.unkPtr26x100()),
            ("ptr50", header.unkPtr50x100()),
            ("ptr51", header.unkPtr51x100()),
            ("ptr52", header.unkPtr52x100()),
            ("ptr53", header.unkPtr53x100()),
            ("ptr54", header.unkPtr54x100()),
            ("ptr55", header.unkPtr55x100()),
            ("ptr56", header.unkPtr56x100()),
            //header.unkPtr70(),
            ("ptr71", header.ptrSecond()),
            ("ptrFirst", header.ptrFirst()),
        ];
        fs::create_dir(prefix);
        for &(name, addr) in addrs.iter() {
            let pal_data = &data[addr as usize..addr as usize + 0x100];
            let digest = md5::compute(pal_data);
            println!("At: {}, {:04X}, {:x}", name, addr, digest);
            let mut foo = pal_data.to_owned();
            foo.append(&mut pal_data.to_owned());
            foo.append(&mut pal_data.to_owned());
            println!("len: {:04X}", foo.len());
            let pal = Palette::from_bytes_prescaled(&foo)?;
            pal.dump_png(&format!("dump/{}/pal-{}-{:04X}", prefix, name, addr));
        }
        */

        return Ok(());
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
