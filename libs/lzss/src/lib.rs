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
use std::mem;

pub fn explode(
    _name: &str,
    input_data: &[u8],
    expect_output_size: Option<usize>,
) -> Result<Vec<u8>, Error> {
    let mut input_offset = 0;
    let mut dict: [u8; 4096] = [' ' as u8; 4096];
    let mut dict_offset = 0;
    let mut out = Vec::with_capacity(expect_output_size.unwrap_or(0));
    while input_offset < input_data.len() {
        let mut flag = input_data[input_offset];
        input_offset += 1;

        for _ in 0..8 {
            if input_offset >= input_data.len() {
                break;
            }
            if flag & 1 == 0 {
                let i0 = input_data[input_offset] as usize;
                let i1 = input_data[input_offset + 1] as usize;
                input_offset += 2;

                let len = (i1 & 0xF) + 3;
                let base = (i0 | ((i1 >> 4) << 8)) + 18;

                for i in 0..len {
                    let c = dict[(base + i) % 4096];
                    out.push(c);
                    dict[dict_offset] = c;
                    dict_offset = (dict_offset + 1) % 4096;
                }
            } else {
                out.push(input_data[input_offset]);
                dict[dict_offset] = input_data[input_offset];
                dict_offset = (dict_offset + 1) % 4096;
                input_offset += 1;
            }
            flag >>= 1;
        }
    }
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
    }
}
