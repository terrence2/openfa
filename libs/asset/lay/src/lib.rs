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
use anyhow::{ensure, Result};
use packed_struct::packed_struct;
use pal::Palette;
use peff::PortableExecutable;
use std::{fs, mem, str};

#[packed_struct]
struct LayerHeader {
    unk_ptr_00x100: u32, // Ramp
    unk_ptr_01: u32,
    unk_ptr_02: u32,
    unk_ptr_03: u32,
    unk_ptr_04: u32,
    seven: u32,
    unk_ptr_20x100: u32, // Ramp
    unk_ptr_21x100: u32,
    unk_ptr_22x100: u32,
    unk_ptr_23x100: u32,
    unk_ptr_24x100: u32,
    unk_ptr_25x100: u32,
    unk_ptr_26x100: u32,
    zero30: u32,
    zero31: u32,
    zero32: u32,
    six: u32,
    unk_ptr_50x100: u32, // Ramp
    unk_ptr_51x100: u32,
    unk_ptr_52x100: u32,
    unk_ptr_53x100: u32,
    unk_ptr_54x100: u32,
    unk_ptr_55x100: u32,
    unk_ptr_56x100: u32,
    zero60: u32,
    zero61: u32,
    zero62: u32,
    unk_ptr_70: u32,
    ptr_second: u32,
    ptr_first: u32,
}

#[packed_struct]
struct LayerPlaneHeader {
    maybe_66: u32,
    maybe_2dd: u32,
    maybe_fog_update_thunk: u32,
    horizon_proc_thunk: u32,
    // FE 1F 38 0E 70 62 00 00 30 0B 01 00 18 47 E8 B8
    unk_1ffe: u16,
    unk_0e38: u16,
    unk_00006270: u32,
    unk_00010b30: u32,
    unk_b8e84718: u32,
    // 8 * 4 bytes => 0x20 bytes
    unk_fill_and_shape: [u8; 16],
    // 0x30 bytes
    unk_stuff0: [u8; 0x20],
    // 0x50 bytes
    unk_stuff1: [u8; 0x1D], // 0x6F bytes
}

const PALETTE_SLICE_SIZE: usize = 0xC0;

pub struct Layer {
    data: Vec<u8>,
    frag_offsets: Vec<usize>,
}

impl Layer {
    pub fn for_index(&self, index: usize) -> Result<Palette> {
        let base = self.frag_offsets[index];
        let slice = &self.data[base..base + PALETTE_SLICE_SIZE];
        Palette::from_bytes(slice)
    }

    pub fn num_indices(&self) -> usize {
        self.frag_offsets.len()
    }

    pub fn from_bytes(data: &[u8], palette: &Palette) -> Result<Layer> {
        let mut pe = PortableExecutable::from_bytes(data)?;
        pe.relocate(0x0000_0000)?;
        Layer::from_pe("inval", &pe, palette)
    }

    fn from_pe(prefix: &str, pe: &peff::PortableExecutable, palette: &Palette) -> Result<Layer> {
        assert!(!prefix.is_empty());

        let mut frag_offsets = Vec::new();

        let palette_digest = md5::compute(palette.as_bytes());

        let dump_stuff = false;

        if dump_stuff {
            fs::create_dir("dump").unwrap_or(());
            fs::create_dir(format!("dump/{prefix}")).unwrap_or(());
        }

        let data = &pe.code;
        let header = LayerHeader::overlay_prefix(data)?;

        assert_eq!(header.unk_ptr_00x100(), header.unk_ptr_04());
        assert_eq!(header.unk_ptr_00x100(), header.unk_ptr_20x100());
        assert!(
            header.unk_ptr_00x100() == header.unk_ptr_50x100()
                || header.unk_ptr_00x100() + 0x100 == header.unk_ptr_50x100()
        );

        // This segment of data counts up from 00 to FF and is present in every single LAY.
        // It makes little sense as a LUT because if you have the value itself? It is always
        // referenced from the first pointer, so maybe it's some weird indirect LUT using
        // the fact that this data is loaded with PE relocation?
        let ramp_addr = header.unk_ptr_00x100();
        let ramp_data = &data[ramp_addr as usize..ramp_addr as usize + 0x100];
        let ramp_digest = md5::compute(ramp_data);
        assert_eq!(
            &format!("{ramp_digest:x}"),
            "e2c865db4162bed963bfaa9ef6ac18f0"
        );

        const FIRST_SIZE: usize = 0x100 + 46;
        let first_addr = header.ptr_first();
        frag_offsets.push(header.ptr_first() as usize + 65);
        if dump_stuff {
            let first_data = &data[first_addr as usize..first_addr as usize + FIRST_SIZE];
            let first_pal_data = &first_data[22 * 3 + 2..];
            let _first_pal = Palette::from_bytes(&first_pal_data[0..(16 * 3 + 13) * 3])?;

            // Seems to be correct for CLOUD and FOG, but really dark for DAY.
            let name = format!("dump/{prefix}/first_data");
            Palette::dump_partial(first_pal_data, 4, &(name + "2"))?;
        }

        let mut offset = first_addr as usize + FIRST_SIZE;
        loop {
            const HDR_SIZE: usize = mem::size_of::<LayerPlaneHeader>();
            const PLANE_SIZE: usize = 0x160;
            // 0xc1 + 0x6f = 0x130 bytes
            // 0x130 / 16 => 19

            if offset + PLANE_SIZE >= header.ptr_second() as usize {
                break;
            }

            // This header recurs every 0x160 bytes, 0x146 bytes after header.ptr_first().
            let plane_data = &data[offset..offset + HDR_SIZE];
            let plane = LayerPlaneHeader::overlay_prefix(plane_data)?;
            ensure!(
                plane.maybe_66() == 0x66 || plane.maybe_66() == 0,
                "expected 66"
            );
            ensure!(
                plane.maybe_2dd() == 0x2DD || plane.maybe_2dd() == 0,
                "expected 2DD"
            );
            ensure!(
                plane.maybe_fog_update_thunk() == 0x2A46
                    || plane.maybe_fog_update_thunk() == 0x2646
                    || plane.maybe_fog_update_thunk() == 0,
                "expected 0x2A46 or 0x2646, not 0x{:4X}",
                plane.maybe_fog_update_thunk()
            );
            ensure!(
                plane.horizon_proc_thunk() != 0,
                "expected horizon proc thunk to be set"
            );
            ensure!(plane.unk_1ffe() == 0x1FFE, "expected 1FFE");
            ensure!(plane.unk_0e38() == 0x0E38, "expected 0E38");
            ensure!(plane.unk_00006270() == 0x0000_6270, "expected 6270");
            ensure!(plane.unk_00010b30() == 0x0001_0B30, "expected 10B30");
            ensure!(plane.unk_b8e84718() == 0xB8E8_4718, "expected B8E84718");

            frag_offsets.push(offset + HDR_SIZE);
            offset += PLANE_SIZE;
        }

        // decode 7 palettes at ptr_second
        // These are all gray scales. Maybe for the skybox?
        offset = header.ptr_second() as usize;
        assert_eq!(
            palette_digest.0,
            md5::compute(&data[offset..offset + 0x300]).0
        );
        if dump_stuff {
            let name = format!("dump/{prefix}/second-0");
            println!("dumping {name}");
            Palette::dump_partial(&data[offset..offset + 0x300], 4, &name)?;
        }
        offset += 0x300;
        for i in 0..18 {
            //let pal_data = &data[offset..offset + 0x100];
            if dump_stuff {
                let name = format!("dump/{prefix}/second-{}", i + 1);
                println!("dumping {name}");
                Palette::dump_partial(&data[offset..offset + 0x100], 1, &name)?;
            }
            offset += 0x100;
        }

        Ok(Layer {
            data: data.to_vec(),
            frag_offsets,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lib::Libs;

    #[test]
    fn it_can_parse_all_lay_files() -> Result<()> {
        let libs = Libs::for_testing()?;
        for (game, palette, catalog) in libs.all() {
            for fid in catalog.find_with_extension("LAY")? {
                let meta = catalog.stat(fid)?;
                println!("At: {}:{:13} @ {}", game.test_dir, meta.name(), meta.path());
                let _layer = Layer::from_bytes(catalog.read(fid)?.as_ref(), palette)?;
            }
        }

        Ok(())
    }
}
