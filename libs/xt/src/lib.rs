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
pub use ot::parse;
use ot::ObjectType;
use pt::PlaneType;
use std::{cell::RefCell, collections::HashMap, rc::Rc};

// A generic type.
pub enum Type {
    JT(ProjectileType),
    NT(NpcType),
    OT(ObjectType),
    PT(PlaneType),
}

impl Type {
    pub fn ot(&self) -> &ObjectType {
        return match self {
            Type::OT(ref ot) => &ot,
            Type::JT(ref jt) => &jt.obj,
            Type::NT(ref nt) => &nt.obj,
            Type::PT(ref pt) => &pt.npc.obj,
        };
    }

    pub fn jt(&self) -> Fallible<&ProjectileType> {
        return Ok(match self {
            Type::JT(ref jt) => &jt,
            _ => bail!("Type: not a projectile"),
        });
    }

    pub fn nt(&self) -> Fallible<&NpcType> {
        return Ok(match self {
            Type::NT(ref nt) => &nt,
            Type::PT(ref pt) => &pt.npc,
            _ => bail!("Type: not an npc"),
        });
    }

    pub fn pt(&self) -> Fallible<&PlaneType> {
        return Ok(match self {
            Type::PT(ref pt) => &pt,
            _ => bail!("Type: not a plane"),
        });
    }
}

// Any single type is likely used by multiple game entities at once so we cache
// type loads aggresively and hand out a Ref to an immutable, shared global
// copy of the Type.
#[derive(Clone)]
pub struct TypeRef(Rc<Box<Type>>);

impl TypeRef {
    fn new(item: Type) -> Self {
        TypeRef(Rc::new(Box::new(item)))
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
pub struct TypeManager<'a> {
    // The library to load from.
    library: &'a LibStack,

    // Cache immutable resources. Use interior mutability for ease of use.
    cache: RefCell<HashMap<String, TypeRef>>,
}

impl<'a> TypeManager<'a> {
    pub fn new(library: &'a LibStack) -> Fallible<TypeManager> {
        return Ok(TypeManager {
            library,
            cache: RefCell::new(HashMap::new()),
        });
    }

    pub fn load(&self, name: &str) -> Fallible<TypeRef> {
        if let Some(item) = self.cache.borrow().get(name) {
            return Ok(item.clone());
        };

        let content = self.library.load_text(name)?;
        let ext = name.rsplitn(2, ".").collect::<Vec<&str>>();
        let item = match ext[0] {
            "OT" => {
                let ot = ObjectType::from_str(&content)?;
                Type::OT(ot)
            }
            "JT" => {
                let ot = ProjectileType::from_str(&content)?;
                Type::JT(ot)
            }
            "NT" => {
                let ot = NpcType::from_str(&content)?;
                Type::NT(ot)
            }
            "PT" => {
                let ot = PlaneType::from_str(&content)?;
                Type::PT(ot)
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

    pub fn library(&self) -> &LibStack {
        return self.library;
    }
}
