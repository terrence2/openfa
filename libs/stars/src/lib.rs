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

/// Parse and provide the contents of the Yale Bright Star Catalogue
/// for use in rendering the skybox.
use failure::{ensure, Fallible};
use packed_struct::packed_struct;
use std::mem;

/*
    The catalog header tells the program what to expect in each The first 28
    bytes of each file contains the following information:

    Integer*4 STAR0	Subtract from star number to get sequence number
    Integer*4 STAR1	First star number in file
    Integer*4 STARN	Number of stars in file
            If negative, coordinates are J2000 instead of B1950
    Integer*4 STNUM	0 if no star ID numbers are present
            1 if star ID numbers are in catalog file
            2 if star ID numbers are region nnnn (GSC)
            3 if star ID numbers are region nnnnn (Tycho)
            4 if star ID numbers are integer*4 not real*4
            <0 No ID number, but object name of -STNUM characters
            at end of entry
    Integer*4 MPROP	0 if no proper motion is included
            1 if proper motion is included
            2 if radial velocity is included
    Integer*4 NMAG	Number of magnitudes present (0-10)
            If negative, coordinates are J2000 instead of B1950
    Integer*4 NBENT	Number of bytes per star entry
*/
packed_struct!(Header {
    _0 => star0: u32,
    _1 => star1: u32,
    _2 => star_n: i32,
    _3 => st_num: u32,
    _4 => m_prop: u32,
    _5 => n_mag: u32,
    _6 => nb_ent: u32
});

/*
    Each entry in the catalog contains the following information:

    Real*4 XNO		Catalog number of star [optional]
    Real*8 SRA0		B1950 Right Ascension (radians)
    Real*8 SDEC0		B1950 Declination (radians)
    Character*2 ISP		Spectral type (2 characters)
    Integer*2 MAG(NMAG)	V Magnitude * 100 [0-10 may be present]
    Real*4 XRPM		R.A. proper motion (radians per year) [optional]
    Real*4 XDPM		Dec. proper motion (radians per year) [optional]
    Real*8 SVEL		Radial velocity in kilometers per second (optional)
    Character*(-STNUM)	Object name [optional, precludes catalog number]

    Catalog numbers may be omitted to save space if they are monotonically
    increasing integers. Proper motions may be omitted if they are not known.
    There may be up to 10 magnitudes.
*/
packed_struct!(SAOEntry {
    //_0 => xno: f32, <- st_num == 0
    _1 => sra0: f64,
    _2 => sdec0: f64,
    _3 => isp: [u8; 2],
    _4 => mag: u16,
    _5 => xrpm: f32,
    _6 => xdpm: f32
    //_7 => svel: f64
    //_8 => name: &[u8]
});

impl SAOEntry {
    pub fn magnitude(&self) -> f32 {
        f32::from(self.mag()) / 100f32
    }

    pub fn right_ascension(&self) -> f32 {
        self.sra0() as f32
    }

    pub fn declination(&self) -> f32 {
        self.sdec0() as f32
    }

    pub fn color(&self) -> u32 {
        match self.isp()[0] as char {
            'A' => {}
            'B' => {}
            'F' => {}
            'G' => {}
            'K' => {}
            'M' => {}
            'N' => {}
            'O' => {}
            'P' => {}
            'R' => {}
            'S' => {}
            ' ' => {}
            '+' => {}
            _ => {}
        }
        0xFF_FF_FF_FF
    }
}

//const BSC_DATA: &'static [u8] = include_bytes!("../assets/BSC5.stars");
const SAO_DATA: &[u8] = include_bytes!("../assets/SAO.pc");

pub struct Stars {
    n_stars: usize,
    entries: &'static [SAOEntry],
}

impl Stars {
    pub fn new() -> Fallible<Self> {
        const HDR_SIZE: usize = mem::size_of::<Header>();

        #[allow(clippy::transmute_ptr_to_ptr)]
        let header_a: &[Header] = unsafe { mem::transmute(&SAO_DATA[0..HDR_SIZE]) };
        let header = &header_a[0];
        assert_eq!(header.star0(), 0);
        assert_eq!(header.star1(), 1);
        assert!(header.star_n() > 0);
        assert_eq!(header.st_num(), 0);
        assert_eq!(header.m_prop(), 1);
        assert_eq!(header.n_mag(), 1);
        assert_eq!(header.nb_ent(), 28);

        #[allow(clippy::transmute_ptr_to_ptr)]
        let entries: &[SAOEntry] = unsafe { mem::transmute(&SAO_DATA[HDR_SIZE..]) };
        Ok(Self {
            n_stars: header.star_n() as usize,
            entries,
        })
    }

    pub fn entry(&self, n: usize) -> Fallible<&'static SAOEntry> {
        ensure!(n < self.n_stars, "star out of bounds");
        Ok(&self.entries[n])
    }

    pub fn catalog_size(&self) -> usize {
        self.n_stars
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    #[test]
    fn it_can_parse_stars() -> Fallible<()> {
        let stars = Stars::new()?;

        let mut visible = 0;
        for i in 0..stars.catalog_size() {
            let entry = stars.entry(i)?;
            if entry.magnitude() < 7f32 {
                visible += 1;
            }
            assert!(entry.sra0() >= 0f64);
            assert!(entry.sra0() <= 2f64 * PI);
            assert!(entry.sdec0() >= -PI / 2f64);
            assert!(entry.sdec0() <= PI / 2f64);
        }
        assert!(visible > 10_000);
        assert!(visible < 20_000);

        Ok(())
    }

    #[test]
    fn band_by_ascension() -> Fallible<()> {
        let stars = Stars::new()?;

        const MAG: f32 = 6.5f32;
        const RA_BINS: usize = 512;
        const DEC_BINS: usize = 256;
        let mut bins: Vec<Vec<Vec<u32>>> = Vec::with_capacity(RA_BINS);
        bins.resize_with(RA_BINS, || Vec::with_capacity(DEC_BINS));
        for bin in bins.iter_mut() {
            bin.resize_with(DEC_BINS, Vec::new);
        }

        use std::collections::HashSet;
        let mut isps = HashSet::new();
        for i in 0..stars.catalog_size() {
            let entry = stars.entry(i)?;
            if entry.magnitude() <= MAG {
                let s = format!("{}{}", entry.isp()[0] as char, entry.isp()[1] as char);
                isps.insert(s);
                // println!("spec: {}{}", entry.isp()[0] as char, entry.isp()[1] as char);

                let ra = entry.sra0();
                let dec = entry.sdec0();
                let ra_bin = (ra * RA_BINS as f64 / (PI * 2f64)) as usize;
                let dec_bin = (((dec + PI) * DEC_BINS as f64) / (PI * 2f64)) as usize;
                bins[ra_bin][dec_bin].push(i as u32);

                // FIXME: actually push into every bin in a range of 0.001
            }
        }

        let mut foo = isps.iter().cloned().collect::<Vec<_>>();
        foo.sort();
        for isp in &foo {
            println!("ISP: {}", isp);
        }

        let mut max_bin = 0;
        let mut total = 0;
        for ra_bins in &bins {
            for dec_bin in ra_bins {
                if dec_bin.len() > max_bin {
                    max_bin = dec_bin.len();
                }
                total += dec_bin.len();
            }
        }

        println!(
            "max in bin: {} of {} bins with {} stars below {} => {} bytes unpacked",
            max_bin,
            RA_BINS * DEC_BINS,
            total,
            MAG,
            max_bin * RA_BINS * DEC_BINS * 4,
        );

        Ok(())
    }
}
