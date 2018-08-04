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
extern crate failure;

use failure::Error;
use std::collections::HashMap;
use std::mem;

fn mix(h0: u8, h1: u8, h2: u8) -> i16 {
    let mut u = ((h0 as u16) << 8) | ((h1 << 4) | h2) as u16;
    // manual sign extend
    if (h0 & 0x08) > 0 {
        u |= 0xF000;
    }
    return u as i16;
}

pub fn explode(
    name: &str,
    data: &[u8],
    expect_output_size: Option<usize>,
) -> Result<Vec<u8>, Error> {
    // (offset_of_dict_end, offset_of_target, expect_bytes)
    let mut key = HashMap::new();
    // let mut btr80 = Vec::new();
    // btr80.push((54, 12));
    // btr80.push((71, 16));
    // btr80.push((98, 36));
    // btr80.push((102, 40));
    // btr80.push((116, 53));
    // key.insert("./test_data/BTR80.INF.lzss.zip", btr80);

    // let mut krivak = Vec::new();
    // krivak.push((46, 4));
    // krivak.push((56, 32));
    // krivak.push((83, 16));
    // krivak.push((105, 53));
    // krivak.push((133, 96));
    // krivak.push((138, 99));
    // key.insert("./test_data/KRIVAK.INF.lzss.zip", krivak);

    // let mut b52 = Vec::new();
    // b52.push((62, 28));
    // b52.push((77, 12));
    // key.insert("./test_data/B52.INF.lzss.zip", b52);

    // let mut nimz = Vec::new();
    // nimz.push((35, 14));
    // nimz.push((67, 12));
    // nimz.push((99, 9));
    // key.insert("./test_data/NIMZ.INF.lzss.zip", nimz);

    // let mut f22 = Vec::new();
    // f22.push((57, 22));
    // f22.push((68, 4));
    // f22.push((72, 31));
    // f22.push((76, 12));
    // key.insert("./test_data/F22.INF.lzss.zip", f22);

    let mut u48i = Vec::new();
    u48i.push((0x40, 0x19, 0x20));
    key.insert("./test_data/U48I.SEQ.lzss.zip", u48i);

    let mut credits = Vec::new();
    credits.push((0x40, 0x19, 0x20));
    key.insert("./test_data/CREDITS.TXT.lzss.zip", credits);

    let mut have_fallthroughs = false;
    let mut repl_offset = 0;
    let mut offset = 0;
    let mut dict = Vec::with_capacity(expect_output_size.unwrap_or(0));
    let mut out = Vec::with_capacity(expect_output_size.unwrap_or(0));
    while offset < data.len() {
        let mut flag = data[offset];
        offset += 1;
        //println!("FLAG: {:02X}", flag);

        for _ in 0..8 {
            if offset >= data.len() {
                break;
            }
            if flag & 1 == 0 {
                let h0 = data[offset] >> 4;
                let h1 = data[offset] & 0xF;
                let h2 = data[offset + 1] >> 4;
                let h3 = data[offset + 1] & 0xF;
                offset += 2;

                let len = h3 as usize + 3;

                // if key.contains_key(name) {
                //     if repl_offset < key[name].len() {
                //         let (expect_repl_offset, expect_tgt_offset, expect_byte) =
                //             key[name][repl_offset];
                //         assert_eq!(expect_repl_offset, dict.len());
                //         println!(
                //             "at: {:>3}, want: {:>3}, delta: {}, len: {}, actual: {:>4} ({:04X}) | {}",
                //             expect_repl_offset,
                //             expect_tgt_offset,
                //             expect_tgt_offset as i64 - expect_repl_offset as i64,
                //             len,
                //             mix(h2, h0, h1), // The relative difference between these seems correct, so I think h1 is the bottom bit.
                //             mix(h2, h0, h1),
                //             expect_tgt_offset as i64 - mix(h2, h0, h1) as i64
                //         );
                //         assert_eq!(mix(h2, h0, h1) + 18, expect_tgt_offset);
                //     }
                // }

                // There are actually 3 cases here and not just the two you'd expect.
                //   1) base >= 0
                //        These are offsets from the start of the dict into the buffer.
                //        I assume these wrap once we go over... something.
                //        Large enough and they look like negative numbers.
                //   2) base < 0
                //        wtf? we seem to have a bunch that are just 0x20 (' ')... can we prove this is *always* true.
                //   3) dict.len() == 0
                //        Not sure what to do here. Need to find a key in later games.
                //   3) dict.len() <= base
                //        Not sure what to do here either.
                let mut base = mix(h2, h0, h1) as isize + 18;

                if base >= 0 {
                    for i in 0..len {
                        let c = dict[base as usize + i];
                        out.push(c);
                        dict.push(c);
                    }
                } else if ((base as u16 & 0x0FFF) as usize) < dict.len() {
                    let base = base as u16 & 0x0FFF;
                    for i in 0..len {
                        let c = dict[base as usize + i];
                        out.push(c);
                        dict.push(c);
                    }
                } else {
                    println!(
                        "{}| unknown{}: {}, {} of {} len {}: {} vs {} vs {}",
                        repl_offset,
                        name,
                        base,
                        base as u16 & 0x0FFF,
                        dict.len(),
                        len,
                        base % 4095,
                        (base + 1) % 4096,
                        4095 - base
                    );
                    have_fallthroughs = true;
                    for i in 0..len {
                        out.push('@' as u8);
                        dict.push('@' as u8);
                    }
                }

                // let mut filler = false;
                // if base < 0 {
                //     base += 4096;

                //     if (base == 0 && dict.len() == 0) || base >= dict.len() as isize {
                //         println!(
                //             "{}| base: {}, {} of {} len {}: {} vs {} vs {}",
                //             name,
                //             base,
                //             base % 4096,
                //             dict.len(),
                //             len,
                //             base % 4095,
                //             (base + 1) % 4096,
                //             4095 - base
                //         );
                //         // println!("{}", out.iter().map(|&c| c as char).collect::<String>());
                //         //base = 4095 - base;
                //         //base = (base + 1) % 4096;
                //         //base = base % 4095;
                //         //base %= dict.len() as isize;
                //         // println!("after: {}", base);
                //         filler = true;
                //     } else {
                //         println!(
                //             "{}| neg: {}, {} of {} len {}: {} vs {} vs {}",
                //             name,
                //             base,
                //             base % 4096,
                //             dict.len(),
                //             len,
                //             base % 4095,
                //             (base + 1) % 4096,
                //             4095 - base
                //         );
                //     }
                // // if base > 0 {
                // //     base %= dict.len() as isize;
                // // }
                // } else {
                //     println!("{}| reg: {} @ {} -> {}", name, len, base, dict.len(),);
                // }

                // if !filler {
                //     for i in 0..len {
                //         let c = dict[base as usize + i];
                //         out.push(c);
                //         dict.push(c);
                //     }
                // } else {
                //     for i in 0..len {
                //         out.push('@' as u8);
                //         dict.push('@' as u8);
                //     }
                // }

                repl_offset += 1;
            } else {
                out.push(data[offset]);
                dict.push(data[offset]);
                offset += 1;
            }
            flag >>= 1;
        }
    }

    if !have_fallthroughs {
        println!("CLEAN: {}", name);
    }

    //println!("{}", dict.iter().map(|&c| c as char).collect::<String>());

    return Ok(out);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::{fs, io::Read};

    fn find_expect_data(path: &str) -> Option<Vec<u8>> {
        // strip ./test_data/inputs/ and .lzss.zip
        let path_stem = &path.to_owned()[19..path.len() - 9];
        let expect_path = format!("./test_data/expect/{}", path_stem);
        if !Path::new(&expect_path).exists() {
            return None;
        }
        let mut fp = fs::File::open(&expect_path).unwrap();
        let mut contents = Vec::new();
        fp.read_to_end(&mut contents).unwrap();
        return Some(contents);
    }

    #[test]
    fn it_doesnt_crash() {
        let paths = fs::read_dir("./test_data/inputs").unwrap();
        for i in paths {
            let entry = i.unwrap();
            let path = format!("{}", entry.path().display());
            let expect = find_expect_data(&path);
            println!("At: {}", path);
            //let path = format!("test_data/{}.INF.lzss.zip", inf);
            let mut fp = fs::File::open(&path).unwrap();
            let mut contents = Vec::new();
            fp.read_to_end(&mut contents).unwrap();
            let out = explode(&path, &contents, None).unwrap();

            if let Some(want) = &expect {
                if path != "./test_data/inputs/SU37.INF.lzss.zip" {
                    println!("CHECKING: {}", path);
                    println!(
                        "out: {}",
                        out.iter().map(|&c| c as char).collect::<String>()
                    );
                    assert_eq!(want, &out);
                }
            }

            use std::fs::File;
            use std::io::Write;
            let outname = format!(
                "output/{}",
                entry.path().file_stem().unwrap().to_str().unwrap()
            );
            let mut fp = File::create(&outname).unwrap();
            fp.write(&out);
        }
        //let inf = "2S6";
        //let inf = "A10";
    }

    #[test]
    fn it_can_decode_weird_bytes() {
        let paths = vec![
            "./test_data/inputs/U44I.SEQ.lzss.zip",
            "./test_data/inputs/UKRMEDAL.SEQ.lzss.zip",
            "./test_data/inputs/U50I.SEQ.lzss.zip",
            "./test_data/inputs/UKRLOST.SEQ.lzss.zip",
            "./test_data/inputs/$F18H.PIC.lzss.zip",
            "./test_data/inputs/U42I.SEQ.lzss.zip",
            "./test_data/inputs/AIR09.XMI.lzss.zip",
            "./test_data/inputs/L_UKR.PIC.lzss.zip",
            "./test_data/inputs/LINER.AI.lzss.zip",
            "./test_data/inputs/ACTDFT2M.PIC.lzss.zip",
            "./test_data/inputs/FLYCROSS.PIC.lzss.zip",
            "./test_data/inputs/$F18.PIC.lzss.zip",
            "./test_data/inputs/SHWPILOT.TXT.lzss.zip",
            "./test_data/inputs/ACTDFT1L.PIC.lzss.zip",
            "./test_data/inputs/MIX4L.BIN.lzss.zip",
            "./test_data/inputs/U36I.SEQ.lzss.zip",
            "./test_data/inputs/ACTDFT3M.PIC.lzss.zip",
            "./test_data/inputs/_$NIMZ.PIC.lzss.zip",
            "./test_data/inputs/U02I.SEQ.lzss.zip",
            "./test_data/inputs/AIR02.XMI.lzss.zip",
            "./test_data/inputs/LARGE.AI.lzss.zip",
            "./test_data/inputs/NAVYEXPD.PIC.lzss.zip",
            "./test_data/inputs/$F14_P.PIC.lzss.zip",
            "./test_data/inputs/AT2.PIC.lzss.zip",
            "./test_data/inputs/ACTDFT1R.PIC.lzss.zip",
            "./test_data/inputs/ACTDFT2R.PIC.lzss.zip",
            "./test_data/inputs/U03I.SEQ.lzss.zip",
            "./test_data/inputs/MCICONS.PIC.lzss.zip",
            "./test_data/inputs/ACTION0L.PIC.lzss.zip",
            "./test_data/inputs/FAT.MT.lzss.zip",
            "./test_data/inputs/$F18_P.PIC.lzss.zip",
            "./test_data/inputs/CREDITS.SEQ.lzss.zip",
            "./test_data/inputs/ACTDFT1M.PIC.lzss.zip",
            "./test_data/inputs/NAVYDIST.PIC.lzss.zip",
            "./test_data/inputs/F18.INF.lzss.zip",
            "./test_data/inputs/NAVYCRSS.PIC.lzss.zip",
            "./test_data/inputs/U48I.SEQ.lzss.zip",
            "./test_data/inputs/U38I.SEQ.lzss.zip",
            "./test_data/inputs/U10I.SEQ.lzss.zip",
            "./test_data/inputs/AIM9B.PIC.lzss.zip",
            "./test_data/inputs/$SU33.PIC.lzss.zip",
            "./test_data/inputs/AIR26.XMI.lzss.zip",
            "./test_data/inputs/U28I.SEQ.lzss.zip",
            "./test_data/inputs/U46I.SEQ.lzss.zip",
            "./test_data/inputs/$AV8.PIC.lzss.zip",
            "./test_data/inputs/$AV8H.PIC.lzss.zip",
            "./test_data/inputs/ACTDFT3L.PIC.lzss.zip",
            "./test_data/inputs/MM.TXT.lzss.zip",
            "./test_data/inputs/ACTDFT0L.PIC.lzss.zip",
            "./test_data/inputs/EALOGO.SEQ.lzss.zip",
            "./test_data/inputs/U26I.SEQ.lzss.zip",
            "./test_data/inputs/U24I.SEQ.lzss.zip",
            "./test_data/inputs/UKR.PIC.lzss.zip",
            "./test_data/inputs/SU37.INF.lzss.zip",
            "./test_data/inputs/UKRWON.SEQ.lzss.zip",
            "./test_data/inputs/ACTDFT3R.PIC.lzss.zip",
            "./test_data/inputs/U18I.SEQ.lzss.zip",
            "./test_data/inputs/U20I.SEQ.lzss.zip",
            "./test_data/inputs/GSH301.PIC.lzss.zip",
            "./test_data/inputs/ACTDFT0M.PIC.lzss.zip",
            "./test_data/inputs/U08I.SEQ.lzss.zip",
            "./test_data/inputs/MOUSEPTR.PIC.lzss.zip",
            "./test_data/inputs/ACTDFT0R.PIC.lzss.zip",
            "./test_data/inputs/CREDITS.TXT.lzss.zip",
            "./test_data/inputs/AIRMEDAL.PIC.lzss.zip",
            "./test_data/inputs/MIX4.BIN.lzss.zip",
            "./test_data/inputs/U30I.SEQ.lzss.zip",
            "./test_data/inputs/UKRDEAD.SEQ.lzss.zip",
            "./test_data/inputs/$SU33_P.PIC.lzss.zip",
            "./test_data/inputs/U04I.SEQ.lzss.zip",
            "./test_data/inputs/F18C.INF.lzss.zip",
            "./test_data/inputs/U14I.SEQ.lzss.zip",
            "./test_data/inputs/U01I.SEQ.lzss.zip",
            "./test_data/inputs/AIR25.XMI.lzss.zip",
            "./test_data/inputs/SLIDEMID.PIC.lzss.zip",
            "./test_data/inputs/U34I.SEQ.lzss.zip",
            "./test_data/inputs/U40I.SEQ.lzss.zip",
            "./test_data/inputs/U06I.SEQ.lzss.zip",
            "./test_data/inputs/$F18_L.PIC.lzss.zip",
            "./test_data/inputs/U16I.SEQ.lzss.zip",
            "./test_data/inputs/ACTDFLT.PIC.lzss.zip",
            "./test_data/inputs/ACTDFT2L.PIC.lzss.zip",
            "./test_data/inputs/U12I.SEQ.lzss.zip",
            "./test_data/inputs/EJECT.SH.lzss.zip",
            "./test_data/inputs/U22I.SEQ.lzss.zip",
            "./test_data/inputs/SWTCH320.PIC.lzss.zip",
            "./test_data/inputs/U32I.SEQ.lzss.zip",
            "./test_data/inputs/DIALNEG.PIC.lzss.zip",
            "./test_data/inputs/^IMDMGE1.5K.lzss.zip",
            "./test_data/inputs/^IMDMGE2.5K.lzss.zip",
        ];

        for path in paths {
            println!("At: {}", path);
            let mut fp = fs::File::open(&path).unwrap();
            let mut contents = Vec::new();
            fp.read_to_end(&mut contents).unwrap();
            let out = explode(&path, &contents, None).unwrap();

            use std::fs::File;
            use std::io::Write;
            let outname = format!(
                "output/{}",
                Path::new(path).file_stem().unwrap().to_str().unwrap()
            );
            let mut fp = File::create(&outname).unwrap();
            fp.write(&out);
        }
    }
}
