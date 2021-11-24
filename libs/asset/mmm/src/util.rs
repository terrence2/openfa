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
use num_traits::Num;

pub(crate) fn maybe_hex<T>(n: &str) -> Result<T>
where
    T: Num + ::std::str::FromStr,
    <T as Num>::FromStrRadixErr: 'static + ::std::error::Error + Send + Sync,
    <T as ::std::str::FromStr>::Err: 'static + ::std::error::Error + Send + Sync,
{
    ensure!(n.is_ascii(), "non-ascii in number");
    Ok(if let Some(hex) = n.strip_prefix('$') {
        T::from_str_radix(hex, 16)?
    } else {
        n.parse::<T>()?
    })
}

pub fn parse_header_delimited<'a, 'b, I: Iterator<Item = &'a str>>(
    tokens: &'b mut I,
) -> Option<String>
where
    'a: 'b,
{
    // Start of Header (0x01) marks delimiting the string? Must be a dos thing. :shrug:
    // Regardless, we need to accumulate tokens until we find one ending in a 1, since
    // we've split on spaces already.
    let tmp = tokens.next().expect("name");
    assert!(tmp.starts_with(1 as char));
    Some(if tmp.ends_with(1 as char) {
        let end = tmp.len() - 1;
        tmp[1..end].to_owned()
    } else {
        let mut tmp = tmp.to_owned();
        #[allow(clippy::while_let_on_iterator)]
        while let Some(next) = tokens.next() {
            tmp = tmp + " " + next;
            if tmp.ends_with(1 as char) {
                break;
            }
        }
        let end = tmp.len() - 1;
        tmp[1..end].to_owned()
    })
}
