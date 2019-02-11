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
use failure::{bail, ensure, Fallible};
use nalgebra::Point3;
use ot::{
    make_type_struct,
    parse::{parse_string, FieldRow, FromRow},
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

impl FromRow for HardpointDefault {
    type Produces = HardpointDefault;
    fn from_row(
        field: &FieldRow,
        _pointers: &HashMap<&str, Vec<&str>>,
    ) -> Fallible<Self::Produces> {
        if field.value().pointer().is_err() {
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
            let name = parse_string(&values[0])?.to_uppercase();
            Ok(HardpointDefault::new(name))
        }
    }
}

make_type_struct![
HardpointType(parent: (), version: HardpointTypeVersion) {
    (Word, [Dec, Hex],            "flags", Unsigned, flags,                        u16, V0, panic!()), // word $8    ; flags
    (Word, [Dec],                  "pos.",     Vec3, pos,                  Point3<f32>, V0, panic!()), // word 0     ; pos.{x,y,z}
    (Word, [Dec],                 "slewH", Unsigned, slewH,                        u16, V0, panic!()), // word 0     ; slewH
    (Word, [Dec],                 "slewP", Unsigned, slewP,                        u16, V0, panic!()), // word 0     ; slewP
    (Word, [Dec],            "slewLimitH", Unsigned, slewLimitH,                   u16, V0, panic!()), // word 0     ; slewLimitH
    (Word, [Dec],            "slewLimitP", Unsigned, slewLimitP,                   u16, V0, panic!()), // word 16380 ; slewLimitP
    (Ptr,  [Dec, Sym], "defaultTypeName0",   Custom, default_loadout, HardpointDefault, V0, panic!()), // ptr defaultTypeName0
    (Byte, [Dec],             "maxWeight", Unsigned, maxWeight,                     u8, V0, panic!()), // byte 0     ; maxWeight
    (Word, [Dec],              "maxItems", Unsigned, maxItems,                     u16, V0, panic!()), // word 32767 ; maxItems
    (Byte, [Dec],                  "name", Unsigned, name,                          u8, V0, panic!())  // byte 0     ; name
}];
