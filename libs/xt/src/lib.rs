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
use failure::{bail, Fallible};
use jt::ProjectileType;
use lib::Library;
use log::trace;
use nt::NpcType;
pub use ot::parse;
use ot::ObjectType;
use pt::PlaneType;
use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc};

// A generic type.
pub enum Type {
    JT(Box<ProjectileType>),
    NT(Box<NpcType>),
    OT(Box<ObjectType>),
    PT(Box<PlaneType>),
}

impl Type {
    pub fn ot(&self) -> &ObjectType {
        match self {
            Type::OT(ref ot) => &ot,
            Type::JT(ref jt) => &jt.ot,
            Type::NT(ref nt) => &nt.ot,
            Type::PT(ref pt) => &pt.nt.ot,
        }
    }

    pub fn jt(&self) -> Fallible<&ProjectileType> {
        Ok(match self {
            Type::JT(ref jt) => &jt,
            _ => bail!("Type: not a projectile"),
        })
    }

    pub fn nt(&self) -> Fallible<&NpcType> {
        Ok(match self {
            Type::NT(ref nt) => &nt,
            Type::PT(ref pt) => &pt.nt,
            _ => bail!("Type: not an npc"),
        })
    }

    pub fn pt(&self) -> Fallible<&PlaneType> {
        Ok(match self {
            Type::PT(ref pt) => &pt,
            _ => bail!("Type: not a plane"),
        })
    }
}

// Any single type is likely used by multiple game objects at once so we cache
// type loads aggressively and hand out a Ref to an immutable, shared global
// copy of the Type.
#[derive(Clone)]
pub struct TypeRef(Rc<Type>);

impl TypeRef {
    fn new(item: Type) -> Self {
        TypeRef(Rc::new(item))
    }

    pub fn ot(&self) -> &ObjectType {
        self.0.ot()
    }

    pub fn jt(&self) -> Fallible<&ProjectileType> {
        self.0.jt()
    }

    pub fn nt(&self) -> Fallible<&NpcType> {
        self.0.nt()
    }

    pub fn pt(&self) -> Fallible<&PlaneType> {
        self.0.pt()
    }
}

// Knows how to load a type from a game library. Keeps a cached copy and hands
// out a pointer to the type.
pub struct TypeManager {
    // The library to load from.
    library: Arc<Box<Library>>,

    // Cache immutable resources. Use interior mutability for ease of use.
    cache: RefCell<HashMap<String, TypeRef>>,
}

impl TypeManager {
    pub fn new(library: Arc<Box<Library>>) -> TypeManager {
        trace!("TypeManager::new");
        TypeManager {
            library,
            cache: RefCell::new(HashMap::new()),
        }
    }

    pub fn load(&self, name: &str) -> Fallible<TypeRef> {
        if let Some(item) = self.cache.borrow().get(name) {
            trace!("TypeManager::load({}) -- cached", name);
            return Ok(item.clone());
        };

        trace!("TypeManager::load({})", name);
        let content = self.library.load_text(name)?;
        let ext = name.rsplitn(2, '.').collect::<Vec<&str>>();
        let item = match ext[0] {
            "OT" => {
                let ot = ObjectType::from_text(&content)?;
                Type::OT(Box::new(ot))
            }
            "JT" => {
                let jt = ProjectileType::from_text(&content)?;
                Type::JT(Box::new(jt))
            }
            "NT" => {
                let nt = NpcType::from_text(&content)?;
                Type::NT(Box::new(nt))
            }
            "PT" => {
                let pt = PlaneType::from_text(&content)?;
                Type::PT(Box::new(pt))
            }
            _ => bail!("resource: unknown type {}", name),
        };
        self.cache
            .borrow_mut()
            .insert(name.to_owned(), TypeRef::new(item));
        if let Some(item) = self.cache.borrow().get(name) {
            return Ok(item.clone());
        }
        panic!("unreachable")
    }
}

#[cfg(test)]
extern crate omnilib;

#[cfg(test)]
mod tests {
    use super::*;
    use failure::Error;
    use omnilib::OmniLib;

    #[test]
    fn can_parse_all_entity_types() -> Fallible<()> {
        let omni = OmniLib::new_for_test_in_games(&[
            "FA", "ATF", "ATFGOLD", "ATFNATO", "USNF", "MF", "USNF97",
        ])?;
        for (game, name) in omni.find_matching("*.[OJNP]T")?.iter() {
            println!(
                "At: {}:{:13} @ {}",
                game,
                name,
                omni.path(game, name)
                    .or_else::<Error, _>(|_| Ok("<none>".to_string()))?
            );
            let lib = omni.library(game);
            let types = TypeManager::new(lib.clone());
            let ty = types.load(name)?;
            // Only one misspelling in 2500 files.
            assert!(ty.ot().file_name() == *name || *name == "SMALLARM.JT");
            // println!(
            //     "{}:{:13}> {:?} <> {}",
            //     game, name, ot.explosion_type, ot.long_name
            // );
        }
        Ok(())
    }
}
