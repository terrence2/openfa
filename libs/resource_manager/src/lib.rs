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
extern crate jt;
extern crate lib;
extern crate nt;
extern crate ot;
extern crate pt;

use failure::Fallible;
use jt::ProjectileType;
use lib::LibStack;
use nt::NpcType;
use ot::ObjectType;
use pt::PlaneType;
use std::{cell::RefCell, collections::HashMap, rc::Rc};

pub enum ResourceItem {
    JT(ProjectileType),
    NT(NpcType),
    OT(ObjectType),
    PT(PlaneType),
}

impl ResourceItem {
    pub fn to_object_type(&self) -> Fallible<&ObjectType> {
        if let ResourceItem::OT(ref obj) = self {
            return Ok(obj);
        }
        bail!("ResourceItem: not an object type")
    }

    pub fn to_npc_type(&self) -> Fallible<&NpcType> {
        if let ResourceItem::NT(ref npc) = self {
            return Ok(npc);
        }
        bail!("ResourceItem: not an npc type")
    }
}

#[derive(Clone)]
pub struct ResourceRef(Rc<Box<ResourceItem>>);

impl ResourceRef {
    fn new(item: ResourceItem) -> Self {
        ResourceRef(Rc::new(Box::new(item)))
    }

    pub fn is_object_type(&self) -> bool {
        self.0.to_object_type().is_ok()
    }

    pub fn to_object_type(&self) -> Fallible<&ObjectType> {
        self.0.to_object_type()
    }

    pub fn is_npc_type(&self) -> bool {
        self.0.to_npc_type().is_ok()
    }

    pub fn to_npc_type(&self) -> Fallible<&ObjectType> {
        self.0.to_object_type()
    }
}

pub struct ResourceManager {
    // The library to load from.
    library: LibStack,

    // Cache immutable resources. Use interior mutability for ease of use.
    cache: RefCell<HashMap<String, ResourceRef>>,

    cache_ot: RefCell<HashMap<String, Rc<Box<ObjectType>>>>,
}

impl ResourceManager {
    // Create without gfx state management -- generally for tests.
    pub fn new_headless(library: LibStack) -> Fallible<ResourceManager> {
        return Ok(ResourceManager {
            library,
            cache: RefCell::new(HashMap::new()),
            cache_ot: RefCell::new(HashMap::new()),
        });
    }

    pub fn load(&self, name: &str) -> Fallible<ResourceRef> {
        if let Some(item) = self.cache.borrow().get(name) {
            return Ok(item.clone());
        };

        let ext = name.rsplitn(2, ".").collect::<Vec<&str>>();
        let item = match ext[0] {
            "NT" => {
                let content = self.library.load_text(name)?;
                let ot = NpcType::from_str(&content)?;
                ResourceItem::NT(ot)
            }
            "OT" => {
                let content = self.library.load_text(name)?;
                let ot = ObjectType::from_str(&content)?;
                ResourceItem::OT(ot)
            }
            "PT" => {
                let content = self.library.load_text(name)?;
                let ot = PlaneType::from_str(&content)?;
                ResourceItem::PT(ot)
            }
            _ => bail!("resource: unknown type {}", name),
        };
        self.cache
            .borrow_mut()
            .insert(name.to_owned(), ResourceRef::new(item));
        if let Some(item) = self.cache.borrow().get(name) {
            return Ok(item.clone());
        }
        panic!("unreachable")
    }

    pub fn load_ot(&self, name: &str) -> Fallible<Rc<Box<ObjectType>>> {
        assert!(name.ends_with(".OT"));

        if let Some(item) = self.cache_ot.borrow().get(name) {
            return Ok(item.clone());
        };

        let content = self.library.load_text(name)?;
        let ot = ObjectType::from_str(&content)?;
        self.cache_ot
            .borrow_mut()
            .insert(name.to_owned(), Rc::new(Box::new(ot)));
        if let Some(item) = self.cache_ot.borrow().get(name) {
            return Ok(item.clone());
        }
        panic!("unreachable")
    }

    // pub fn load_t2(&self, name: &str) -> Fallible<Rc<Box<Terrain>>> {

    // }

    pub fn library(&self) -> &LibStack {
        return &self.library;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn load_via_resource() -> Fallible<()> {
        let lib = LibStack::from_dir_search(Path::new("../../test_data/unpacked/FA"))?;
        let rm = ResourceManager::new_headless(lib)?;
        let res = rm.load("STRIP2.OT")?;
        let ot = res.to_object_type()?;
        assert_eq!(ot.short_name, "Runway 2");
        return Ok(());
    }

    #[test]
    fn load_direct() -> Fallible<()> {
        let lib = LibStack::from_dir_search(Path::new("../../test_data/unpacked/FA"))?;
        let rm = ResourceManager::new_headless(lib)?;
        let ot = rm.load_ot("STRIP2.OT")?;
        assert_eq!(ot.short_name, "Runway 2");
        return Ok(());
    }
}
