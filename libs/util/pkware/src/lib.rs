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

// An implementation of PKWare decompression. This is a direct apples-to-apples
// port of Mark Adler's blast.c/h implementation included in zlib. As such, the
// original license also still holds to this code. It is included in full below:

/* blast.h -- interface for blast.c
 Copyright (C) 2003, 2012 Mark Adler
 version 1.2, 24 Oct 2012

 This software is provided 'as-is', without any express or implied
 warranty.  In no event will the author be held liable for any damages
 arising from the use of this software.

 Permission is granted to anyone to use this software for any purpose,
 including commercial applications, and to alter it and redistribute it
 freely, subject to the following restrictions:

 1. The origin of this software must not be misrepresented; you must not
    claim that you wrote the original software. If you use this software
    in a product, an acknowledgment in the product documentation would be
    appreciated but is not required.
 2. Altered source versions must be plainly marked as such, and must not be
    misrepresented as being the original software.
 3. This notice may not be removed or altered from any source distribution.

 Mark Adler    madler@alumni.caltech.edu
*/
use anyhow::{ensure, Result};
use lazy_static::lazy_static;
use log::trace;

/// Simple interface: uncompress all of data at once from memory to memory.
pub fn explode(data: &[u8], expect_output_size: Option<usize>) -> Result<Vec<u8>> {
    let mut state = State {
        data,
        offset: 0,
        bitbuf: 0,
        bitcnt: 0,
        out: Vec::with_capacity(expect_output_size.unwrap_or(0)),
    };
    state.decomp()?;
    Ok(state.out)
}

struct State<'a> {
    // Fixed-length input data.
    data: &'a [u8],

    // Read offset into input data.
    offset: usize,

    // Buffer of partially read bytes and the number of bytes in that buffer.
    bitbuf: usize,
    bitcnt: usize,

    // Output vector of bytes.
    out: Vec<u8>,
}

/*
 * Huffman code decoding tables.  count[1..MAXBITS] is the number of symbols of
 * each length, which for a canonical code are stepped through in order.
 * symbol[] are the symbol values in canonical order, where the number of
 * entries is the sum of the counts in count[].  The decoding process can be
 * seen in the function decode() below.
 */
pub struct Huffman {
    count: [u16; MAXBITS + 1],
    symbol: [u16; 256],
}

impl Huffman {
    fn new() -> Self {
        Huffman {
            count: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            symbol: [
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0,
            ],
        }
    }
}

/* maximum code length */
const MAXBITS: usize = 13;

/* base for length codes */
const BASE: [i16; 16] = [3, 2, 4, 5, 6, 7, 8, 9, 10, 12, 16, 24, 40, 72, 136, 264];

/* extra bits for length codes */
const EXTRA: [u8; 16] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8];

/* bit lengths of literal codes */
const LITERAL_LENGTHS: [u8; 98] = [
    11, 124, 8, 7, 28, 7, 188, 13, 76, 4, 10, 8, 12, 10, 12, 10, 8, 23, 8, 9, 7, 6, 7, 8, 7, 6, 55,
    8, 23, 24, 12, 11, 7, 9, 11, 12, 6, 7, 22, 5, 7, 24, 6, 11, 9, 6, 7, 22, 7, 11, 38, 7, 9, 8,
    25, 11, 8, 11, 9, 12, 8, 12, 5, 38, 5, 38, 5, 11, 7, 5, 6, 21, 6, 10, 53, 8, 7, 24, 10, 27, 44,
    253, 253, 253, 252, 252, 252, 13, 12, 45, 12, 45, 12, 61, 12, 45, 44, 173,
];

/* bit lengths of length codes 0..15 */
const LENGTH_LENGTHS: [u8; 6] = [2, 35, 36, 53, 38, 23];

/* bit lengths of distance codes 0..63 */
const DISTANCE_LENGTHS: [u8; 7] = [2, 20, 53, 230, 247, 151, 248];

lazy_static! {
    pub static ref LITERAL_CODES: Huffman = construct(&LITERAL_LENGTHS).unwrap();
    pub static ref LENGTH_CODES: Huffman = construct(&LENGTH_LENGTHS).unwrap();
    pub static ref DISTANCE_CODES: Huffman = construct(&DISTANCE_LENGTHS).unwrap();
}

/*
 * Given a list of repeated code lengths rep[0..n-1], where each byte is a
 * count (high four bits + 1) and a code length (low four bits), generate the
 * list of code lengths.  This compaction reduces the size of the object code.
 * Then given the list of code lengths length[0..n-1] representing a canonical
 * Huffman code for n symbols, construct the tables required to decode those
 * codes.  Those tables are the number of codes of each length, and the symbols
 * sorted by length, retaining their original order within each length.  The
 * return value is zero for a complete code set, negative for an over-
 * subscribed code set, and positive for an incomplete code set.  The tables
 * can be used if the return value is zero or positive, but they cannot be used
 * if the return value is negative.  If the return value is zero, it is not
 * possible for decode() using that table to return an error--any stream of
 * enough bits will resolve to a symbol.  If the return value is positive, then
 * it is possible for decode() using that table to return an error for received
 * codes past the end of the incomplete lengths.
 */
// Note: constructing all 3 vectors takes ~1us; not really worth optimizating
// further as they are cached by lazy_static!.
fn construct(rep: &[u8]) -> Result<Huffman> {
    trace!("constructing huffman tables");

    let mut length = [0usize; 256]; /* code lengths */

    /* convert compact repeat counts into symbol bit length list */
    let mut symbol = 0; /* current symbol when stepping through length[] */
    for &r in rep.iter() {
        let sym_count = (r >> 4) + 1; // top 4 bits is the code count
        let sym_len = r & 15; // bottom 4 bits is the code lengths
        for _ in 0..sym_count {
            length[symbol] = sym_len as usize; // we're only using 4 bits here... does this get expanded later?
            symbol += 1;
        }
    }
    let n = symbol;

    /* count number of codes of each length */
    let mut h = Huffman::new();
    debug_assert!(n < 256);
    for sym_len in length.iter().take(n) {
        debug_assert!(*sym_len <= MAXBITS);
        h.count[*sym_len] += 1;
    }
    if h.count[0] as usize == n {
        /* no codes! Complete, but decode() will fail */
        return Ok(h);
    }

    /* check for an over-subscribed or incomplete set of lengths */
    let mut left = 1; /* one possible code of zero length */
    for len in 0..=MAXBITS {
        left <<= 1; /* one more bit, double codes left */
        /* over-subscribed--return negative; left > 0 means incomplete */
        ensure!(left >= h.count[len], "codes are over-subscribed");
        left -= h.count[len]; /* deduct count from possible codes */
    }

    /* generate offsets into symbol table for each length for sorting */
    let mut offs = [0usize; MAXBITS + 1];
    for len in 1..MAXBITS {
        offs[len + 1] = offs[len] + (h.count[len] as usize);
    }

    /*
     * put symbols in table sorted by length, by symbol order within each
     * length
     */
    for symbol in 0..n {
        if length[symbol] != 0 {
            h.symbol[offs[length[symbol]]] = symbol as u16;
            offs[length[symbol]] += 1;
        }
    }

    Ok(h)
}

impl<'a> State<'a> {
    /*
     * Decode PKWare Compression Library stream.
     *
     * Format notes:
     *
     * - First byte is 0 if literals are uncoded or 1 if they are coded.  Second
     *   byte is 4, 5, or 6 for the number of extra bits in the distance code.
     *   This is the base-2 logarithm of the dictionary size minus six.
     *
     * - Compressed data is a combination of literals and length/distance pairs
     *   terminated by an end code.  Literals are either Huffman coded or
     *   uncoded bytes.  A length/distance pair is a coded length followed by a
     *   coded distance to represent a string that occurs earlier in the
     *   uncompressed data that occurs again at the current location.
     *
     * - A bit preceding a literal or length/distance pair indicates which comes
     *   next, 0 for literals, 1 for length/distance.
     *
     * - If literals are uncoded, then the next eight bits are the literal, in the
     *   normal bit order in th stream, i.e. no bit-reversal is needed. Similarly,
     *   no bit reversal is needed for either the length extra bits or the distance
     *   extra bits.
     *
     * - Literal bytes are simply written to the output.  A length/distance pair is
     *   an instruction to copy previously uncompressed bytes to the output.  The
     *   copy is from distance bytes back in the output stream, copying for length
     *   bytes.
     *
     * - Distances pointing before the beginning of the output data are not
     *   permitted.
     *
     * - Overlapped copies, where the length is greater than the distance, are
     *   allowed and common.  For example, a distance of one and a length of 518
     *   simply copies the last byte 518 times.  A distance of four and a length of
     *   twelve copies the last four bytes three times.  A simple forward copy
     *   ignoring whether the length is greater than the distance or not implements
     *   this correctly.
     */
    fn decomp(&mut self) -> Result<()> {
        /* read header */
        let lit = self.bits(8)?;
        ensure!(lit <= 1, "invalid header");
        let dict = self.bits(8)?;
        ensure!((4..=6).contains(&dict), "invalid dict-bits");

        /* decode literals and length/distance pairs */
        loop {
            if self.bits(1)? != 0 {
                /* get length */
                let len_code = self.decode(&LENGTH_CODES)? as usize;
                let len = BASE[len_code] + i16::from(self.bits(EXTRA[len_code])?);
                /* end code */
                if len == 519 {
                    break;
                }

                /* get distance */
                let dist_shift = if len == 2 { 2 } else { dict };
                let mut dist = self.decode(&DISTANCE_CODES)? << dist_shift;
                dist += u16::from(self.bits(dist_shift)?);
                dist += 1;

                /* copy length bytes from distance bytes back */
                let base = self.out.len() - dist as usize;
                for i in 0..len as usize {
                    let b = self.out[base + i];
                    self.out.push(b);
                }
            } else {
                /* get literal and write it */
                let symbol = if lit == 1 {
                    self.decode(&LITERAL_CODES)?
                } else {
                    u16::from(self.bits(8)?)
                };
                debug_assert!(symbol < 256);
                self.outb(symbol as u8);
            }
        }

        Ok(())
    }

    /*
     * Decode a code from the stream s using huffman table h.  Return the symbol or
     * a negative value if there is an error.  If all of the lengths are zero, i.e.
     * an empty code, or if the code is incomplete and an invalid code is received,
     * then -9 is returned after reading MAXBITS bits.
     *
     * Format notes:
     *
     * - The codes as stored in the compressed data are bit-reversed relative to
     *   a simple integer ordering of codes of the same lengths.  Hence below the
     *   bits are pulled from the compressed data one at a time and used to
     *   build the code value reversed from what is in the stream in order to
     *   permit simple integer comparisons for decoding.
     *
     * - The first code for the shortest length is all ones.  Subsequent codes of
     *   the same length are simply integer decrements of the previous code.  When
     *   moving up a length, a one bit is appended to the code.  For a complete
     *   code, the last code of the longest length will be all zeros.  To support
     *   this ordering, the bits pulled during decoding are inverted to apply the
     *   more "natural" ordering starting with all zeros and incrementing.
     */
    fn decode(&mut self, h: &Huffman) -> Result<u16> {
        let mut bitbuf = self.bitbuf;
        let mut left = self.bitcnt;
        let mut code: usize = 0;
        let mut first = 0;
        let mut index = 0;
        let mut len = 1;
        let mut next = 1;

        loop {
            while left > 0 {
                left -= 1;

                code |= (bitbuf & 1) ^ 1; /* invert code */
                bitbuf >>= 1;
                let count = h.count[next] as usize;
                next += 1;
                if code < first + count {
                    /* if length len, return symbol */
                    self.bitbuf = bitbuf;
                    self.bitcnt = (self.bitcnt as isize - len as usize as isize) as usize & 7;
                    return Ok(h.symbol[index + (code - first)]);
                }
                index += count; /* else update for next length */
                first += count;
                first <<= 1;
                code <<= 1;
                len += 1;
            }
            left = (MAXBITS + 1) - len;
            if left == 0 {
                break;
            }
            debug_assert_eq!(bitbuf, 0);
            bitbuf = self.inb()? as usize;
        }
        unreachable!("ran out of codes")
    }

    /*
     * Return need bits from the input stream.  This always leaves less than
     * eight bits in the buffer.  bits() works properly for need == 0.
     *
     * Format notes:
     *
     * - Bits are stored in bytes from the least significant bit to the most
     *   significant bit.  Therefore bits are dropped from the bottom of the bit
     *   buffer, using shift right, and new bytes are appended to the top of the
     *   bit buffer, using shift left.
     */
    fn bits(&mut self, need: u8) -> Result<u8> {
        debug_assert!(need <= 8, "need too many bits");

        /* load at least `need` bits into val */
        let mut val = self.bitbuf;
        while self.bitcnt < need as usize {
            val |= (self.inb()? as usize) << self.bitcnt;
            self.bitcnt += 8;
        }

        /* drop need bits and update buffer, always zero to seven bits left */
        self.bitbuf = val >> need;
        self.bitcnt -= need as usize;

        /* return need bits, zeroing the bits above that */
        Ok((val & ((1 << need) - 1)) as u8)
    }

    // Read in one byte.
    fn inb(&mut self) -> Result<u8> {
        ensure!(self.offset < self.data.len(), "overflowed buf");
        let out = self.data[self.offset];
        self.offset += 1;
        Ok(out)
    }

    // Write out one byte.
    fn outb(&mut self, symbol: u8) {
        self.out.push(symbol as u8);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, io::Read};

    #[test]
    fn it_doesnt_crash() {
        if let Ok(paths) = fs::read_dir("../../../test_data/pkware") {
            for i in paths {
                let entry = i.unwrap();
                let path = format!("{}", entry.path().display());
                println!("At: {}", path);
                let mut fp = fs::File::open(path).unwrap();
                let mut contents = Vec::new();
                fp.read_to_end(&mut contents).unwrap();
                let _out = explode(&contents, None).unwrap();
            }
        }
    }
}

#[cfg(all(feature = "benchmark", test))]
mod bench {
    extern crate test;
    use self::test::Bencher;
    use super::*;
    use std::{fs, io::Read};

    #[bench]
    fn bench_construct_literals(b: &mut Bencher) {
        b.iter(|| construct(&LITERAL_LENGTHS).unwrap());
    }

    #[bench]
    fn bench_construct_lengths(b: &mut Bencher) {
        b.iter(|| construct(&LENGTH_LENGTHS).unwrap());
    }

    #[bench]
    fn bench_construct_distances(b: &mut Bencher) {
        b.iter(|| construct(&DISTANCE_LENGTHS).unwrap());
    }

    // This measures <16ms on an i7-4790K.
    // `perf stat` on blast on the same file gets between 15 and 22ms.
    #[bench]
    fn bench_decode_one(b: &mut Bencher) {
        let path = "test_data/C_INTRO.11K.pkware.zip";
        let mut fp = fs::File::open(path).unwrap();
        let mut contents = Vec::new();
        b.iter(|| {
            fp.read_to_end(&mut contents).unwrap();
            explode(&contents, None).unwrap();
        })
    }
}
