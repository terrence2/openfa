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
use asset::AssetLoader;
use failure::{bail, ensure, Fallible};
use nalgebra::Point3;
use ot::{
    make_consume_fields, make_storage_type, make_type_struct, make_validate_field_repr,
    make_validate_field_type,
    parse::{FieldRow, FromField},
};
use std::collections::HashMap;

#[derive(Debug, Ord, PartialOrd, Eq, PartialEq)]
enum HardpointTypeVersion {
    V0,
}

impl HardpointTypeVersion {
    fn from_len(_: usize) -> Fallible<Self> {
        Ok(HardpointTypeVersion::V0)
    }
}

pub struct HardpointDefault {
    #[allow(dead_code)]
    name: Option<String>,
}

impl HardpointDefault {
    fn new(name: String) -> Self {
        HardpointDefault { name: Some(name) }
    }

    fn new_empty() -> Self {
        HardpointDefault { name: None }
    }
}

// TODO: do we want to load these here or leave it to a higher level?
//enum Loadout {
//    GAS(Fueltank),
//    SEE(Sensor),
//    ECM(Ecm),
//    JT(ProjectileType),
//}

impl FromField for HardpointDefault {
    type Produces = HardpointDefault;
    fn from_field(
        field: &FieldRow,
        _pointers: &HashMap<&str, Vec<&str>>,
        _assets: &AssetLoader,
    ) -> Fallible<Self::Produces> {
        if !field.value().pointer().is_ok() {
            ensure!(
                field.value().numeric()?.dword()? == 0u32,
                "null pointer must be dword 0"
            );
            Ok(HardpointDefault::new_empty())
        } else {
            let (sym, values) = field.value().pointer()?;
            ensure!(
                sym.starts_with("defaultTypeName"),
                "expected defaultTypeName in ptr name"
            );
            let name = ot::parse::string(&values[0])?.to_uppercase();
            Ok(HardpointDefault::new(name))
        }
    }
}

make_type_struct![
HardpointType(parent: (), version: HardpointTypeVersion) {
    (Word, [Dec, Hex],            "flags", Unsigned,         flags,              u16, V0, panic!()), // word $8    ; flags
    (Word, [Dec],                  "pos.", Vec3,              unk1,      Point3<f32>, V0, panic!()), // word 0     ; pos.x
    (Word, [Dec],                 "slewH", Unsigned,          unk4,              u16, V0, panic!()), // word 0     ; slewH
    (Word, [Dec],                 "slewP", Unsigned,          unk5,              u16, V0, panic!()), // word 0     ; slewP
    (Word, [Dec],            "slewLimitH", Unsigned,          unk6,              u16, V0, panic!()), // word 0     ; slewLimitH
    (Word, [Dec],            "slewLimitP", Unsigned,          unk7,              u16, V0, panic!()), // word 16380 ; slewLimitP
    (Ptr,  [Dec, Sym], "defaultTypeName0", Struct, default_loadout, HardpointDefault, V0, panic!()), // ptr defaultTypeName0
    (Byte, [Dec],             "maxWeight", Unsigned,          unk9,               u8, V0, panic!()), // byte 0     ; maxWeight
    (Word, [Dec],              "maxItems", Unsigned,         unk10,              u16, V0, panic!()), // word 32767 ; maxItems
    (Byte, [Dec],                  "name", Unsigned,         unk11,               u8, V0, panic!())  // byte 0     ; name
}];
