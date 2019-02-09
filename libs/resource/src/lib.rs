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
//extern crate bi;
//extern crate hud;
extern crate lib;
//extern crate pcm;
extern crate lay;
extern crate sh;
extern crate t2;

use failure::Fallible;
use lib::Library;

//pub use bi::AI;
//pub use hud::HUD;
pub use lay::Layer;
pub use sh::CpuShape;
//pub use pcm::Sound;
pub use t2::Terrain;

use std::{cell::RefCell, collections::HashMap, rc::Rc};

// Placeholder
pub struct AI {}
impl AI {
    // fn from_data(data: &[u8]) -> Fallible<Self> {
    //     Ok(AI {})
    // }
}

// Placeholder
pub struct HUD {}
impl HUD {
    // fn from_data(data: &[u8]) -> Fallible<Self> {
    //     Ok(HUD {})
    // }
}

// Placeholder
pub struct Sound {}
impl Sound {
    // fn from_data(data: &[u8]) -> Fallible<Self> {
    //     Ok(Sound {})
    // }
}

pub struct ResourceManager<'a> {
    // The library to load from.
    library: &'a Library,

    // cache_ai: RefCell<HashMap<String, Rc<Box<AI>>>>,
    // cache_hud: RefCell<HashMap<String, Rc<Box<HUD>>>>,
    // cache_layer: RefCell<HashMap<String, Rc<Box<Layer>>>>,
    cache_sh: RefCell<HashMap<String, Rc<Box<CpuShape>>>>,
    // cache_sound: RefCell<HashMap<String, Rc<Box<Sound>>>>,
    // cache_terrain: RefCell<HashMap<String, Rc<Box<Terrain>>>>,
}

impl<'a> ResourceManager<'a> {
    // Create without gfx state management -- generally for tests.
    pub fn new_headless(library: &'a Library) -> Fallible<Self> {
        return Ok(ResourceManager {
            library,
            // cache_ai: RefCell::new(HashMap::new()),
            // cache_hud: RefCell::new(HashMap::new()),
            // cache_layer: RefCell::new(HashMap::new()),
            cache_sh: RefCell::new(HashMap::new()),
            // cache_sound: RefCell::new(HashMap::new()),
            // cache_terrain: RefCell::new(HashMap::new()),
        });
    }

    // pub fn load<T>(&self, name: &str) -> Fallible<Rc<Box<T>>> {
    // }

    pub fn load_sh(&self, name: &str) -> Fallible<Rc<Box<CpuShape>>> {
        assert!(name.ends_with(".SH"));

        // FIXME: I *think* that FA probably got random forests by manually
        // replacing TREE{1,2} with TREE{A,B,C,D}. Is there a way to verify?
        let name = if name == "TREE1.SH" { "TREEA.SH" } else { name };
        let name = if name == "TREE2.SH" { "TREEC.SH" } else { name };

        if let Some(item) = self.cache_sh.borrow().get(name) {
            return Ok(item.clone());
        };

        let content = self.library.load(name)?;
        let sh = CpuShape::from_bytes(&content)?;
        self.cache_sh
            .borrow_mut()
            .insert(name.to_owned(), Rc::new(Box::new(sh)));
        return Ok(self.cache_sh.borrow().get(name).unwrap().clone());
    }

    // pub fn load_sound(&self, name: &str) -> Fallible<Rc<Box<Sound>>> {
    //     Ok(Rc::new(Box::new(Sound {})))
    // }

    // pub fn load_hud(&self, name: &str) -> Fallible<Rc<Box<HUD>>> {
    //     Ok(Rc::new(Box::new(HUD {})))
    // }

    // pub fn load_ai(&self, name: &str) -> Fallible<Rc<Box<AI>>> {
    //     Ok(Rc::new(Box::new(AI {})))
    // }

    pub fn library(&self) -> &Library {
        return self.library;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_load_direct() -> Fallible<()> {
        let lib = Library::from_dir_search(Path::new("../../test_data/unpacked/FA"))?;
        let rm = ResourceManager::new_headless(&lib)?;
        let _sh = rm.load_sh("F22.SH")?;
        //assert_eq!(ot.short_name, "Runway 2");
        return Ok(());
    }
}
