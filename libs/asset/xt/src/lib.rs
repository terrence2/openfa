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
pub use jt::ProjectileType;
pub use nt::{HardpointType, NpcType};
pub use ot::parse;
pub use ot::ObjectType;
pub use pt::{Envelope, PlaneType};

use anyhow::{bail, Result};
use catalog::Catalog;
use lib::from_dos_string;
use log::trace;
use std::{cell::RefCell, collections::HashMap, rc::Rc};

// A generic type.
#[allow(clippy::upper_case_acronyms)]
#[derive(Debug)]
pub enum Type {
    JT(Box<ProjectileType>),
    NT(Box<NpcType>),
    OT(Box<ObjectType>),
    PT(Box<PlaneType>),
}

impl Type {
    pub fn ot(&self) -> &ObjectType {
        match self {
            Type::OT(ref ot) => ot,
            Type::JT(ref jt) => &jt.ot,
            Type::NT(ref nt) => &nt.ot,
            Type::PT(ref pt) => &pt.nt.ot,
        }
    }

    pub fn jt(&self) -> Result<&ProjectileType> {
        Ok(match self {
            Type::JT(ref jt) => jt,
            _ => bail!("Type: not a projectile"),
        })
    }

    pub fn nt(&self) -> Result<&NpcType> {
        Ok(match self {
            Type::NT(ref nt) => nt,
            Type::PT(ref pt) => &pt.nt,
            _ => bail!("Type: not an npc"),
        })
    }

    pub fn pt(&self) -> Result<&PlaneType> {
        Ok(match self {
            Type::PT(ref pt) => pt,
            _ => bail!("Type: not a plane"),
        })
    }
}

// Any single type is likely used by multiple game objects at once so we cache
// type loads aggressively and hand out a Ref to an immutable, shared global
// copy of the Type.
#[derive(Clone, Debug)]
pub struct TypeRef(Rc<Type>);

impl TypeRef {
    fn new(item: Type) -> Self {
        TypeRef(Rc::new(item))
    }

    pub fn ot(&self) -> &ObjectType {
        self.0.ot()
    }

    pub fn jt(&self) -> Result<&ProjectileType> {
        self.0.jt()
    }

    pub fn nt(&self) -> Result<&NpcType> {
        self.0.nt()
    }

    pub fn pt(&self) -> Result<&PlaneType> {
        self.0.pt()
    }

    pub fn is_pt(&self) -> bool {
        self.pt().is_ok()
    }

    pub fn is_nt(&self) -> bool {
        self.nt().is_ok()
    }

    pub fn is_jt(&self) -> bool {
        self.jt().is_ok()
    }
}

// Knows how to load a type from a game library. Keeps a cached copy and hands
// out a pointer to the type, since we frequently need to load the same item
// repeatedly.
pub struct TypeManager {
    // Cache immutable resources. Use interior mutability for ease of use.
    cache: RefCell<HashMap<String, TypeRef>>,
}

impl TypeManager {
    pub fn empty() -> TypeManager {
        trace!("TypeManager::new");
        TypeManager {
            cache: RefCell::new(HashMap::new()),
        }
    }

    pub fn load(&self, name: &str, catalog: &Catalog) -> Result<TypeRef> {
        let cache_key = format!("{}:{}", catalog.default_label(), name);
        if let Some(item) = self.cache.borrow().get(&cache_key) {
            trace!("TypeManager::load({}) -- cached", name);
            return Ok(item.clone());
        };

        trace!("TypeManager::load({})", name);
        let content = from_dos_string(catalog.read_name_sync(name)?);
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
        let xt = TypeRef::new(item);
        self.cache.borrow_mut().insert(cache_key, xt.clone());
        Ok(xt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lib::CatalogBuilder;

    #[test]
    fn can_parse_all_entity_types() -> Result<()> {
        let (mut catalog, inputs) = CatalogBuilder::build_and_select(&["*:*.[OJNP]T".to_owned()])?;
        for &fid in &inputs {
            let label = catalog.file_label(fid)?;
            let game = label.split(':').last().unwrap();
            let meta = catalog.stat_sync(fid)?;
            println!(
                "At: {}:{:13} @ {}",
                game,
                meta.name(),
                meta.path()
                    .map(|v| v.to_string_lossy())
                    .unwrap_or_else(|| "<none>".into())
            );
            let types = TypeManager::empty();
            catalog.set_default_label(&label);
            let ty = types.load(meta.name(), &catalog)?;
            // Only one misspelling in 2500 files.
            assert!(ty.ot().file_name() == meta.name() || meta.name() == "SMALLARM.JT");
            // println!(
            //     "{}:{:13}> {:?} <> {}",
            //     game, name, ot.explosion_type, ot.long_name
            // );
        }
        Ok(())
    }
}
