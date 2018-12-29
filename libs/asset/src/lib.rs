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

// Load assets. This module offers a few advantages over doing this manually:
//   1) Loads assets off the main thread
//   2) Loads multiple assets in parallel
//   3)
//
// Open question: are we responsible for upload to the GPU? If not, who is?
use failure::{Fallible};
use lib::LibStack;
use std::{cell::RefCell, collections::HashMap, sync::Arc};
use t2::Terrain;

pub struct AssetLoader {
    lib: Arc<Box<LibStack>>,

    cache_ai: RefCell<HashMap<String, Arc<Box<u32>>>>,
    cache_terrain: RefCell<HashMap<String, Arc<Box<Terrain>>>>,
}

impl AssetLoader {
    pub fn new(lib: Arc<Box<LibStack>>) -> Fallible<Self> {
        Ok(AssetLoader {
            lib,
            cache_ai: RefCell::new(HashMap::new()),
            cache_terrain: RefCell::new(HashMap::new()),
        })
    }

    pub fn load_ai(&self, filename: &str) -> Fallible<Arc<Box<u32>>> {
        if !self.cache_ai.borrow().contains_key(filename) {
            let _data = self.lib.load(filename)?;
            let ai = 0;
            self.cache_ai
                .borrow_mut()
                .insert(filename.to_owned(), Arc::new(Box::new(ai)));
        }
        Ok(self.cache_ai.borrow()[filename].clone())
    }

    pub fn load_t2(&self, filename: &str) -> Fallible<Arc<Box<Terrain>>> {
        if !self.cache_terrain.borrow().contains_key(filename) {
            let data = self.lib.load(filename)?;
            let terrain = Terrain::from_bytes(&data)?;
            self.cache_terrain
                .borrow_mut()
                .insert(filename.to_owned(), Arc::new(Box::new(terrain)));
        }
        Ok(self.cache_terrain.borrow()[filename].clone())
    }

    pub fn load_sound(&self, _filename: &str) -> Fallible<Arc<Box<u32>>> {
        Ok(Arc::new(Box::new(0)))
    }

    pub fn load_hud(&self, _filename: &str) -> Fallible<Arc<Box<u32>>> {
        Ok(Arc::new(Box::new(0)))
    }

    pub fn load_sh(&self, _filename: &str) -> Fallible<Arc<Box<u32>>> {
        Ok(Arc::new(Box::new(0)))
    }
}

#[cfg(test)]
extern crate omnilib;

#[cfg(test)]
mod tests {
    use super::*;
    use omnilib::OmniLib;

    #[test]
    fn it_works() -> Fallible<()> {
        let omni = OmniLib::new_for_test_in_games(vec!["FA"])?;
        for lib in omni.libraries() {
            let asset_loader = AssetLoader::new(lib.clone())?;
            for filename in lib.find_matching("*.T2")? {
                println!("name: {:?}", filename);
                let foo = asset_loader.load_t2(&filename)?;
                println!("res: {:?}", foo.name);
            }
        }
        Ok(())
    }
}