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
extern crate failure;
extern crate lib;
extern crate pic;

use failure::Fallible;
use lib::LibStack;

pub struct TextureManager<'a> {
    library: &'a LibStack,
}

impl<'a> TextureManager<'a> {
    pub fn new(library: &'a LibStack) -> Fallible<Self> {
        return Ok(TextureManager { library });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let _ = TextureManager::new();
    }
}
