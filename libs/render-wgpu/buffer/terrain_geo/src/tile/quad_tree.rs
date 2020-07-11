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
use crate::tile::{DataSetCoordinates, DataSetDataKind};
use failure::Fallible;
use geodesy::{GeoCenter, Graticule};
use json::JsonValue;
use std::path::Path;

struct QuadTreeNode {
    #[allow(unused)]
    // Number of 1" samples per sample in this level.
    scale: i32,

    #[allow(unused)]
    // References to children.
    children: [usize; 4],
}

pub(crate) struct QuadTree {
    #[allow(unused)]
    // Root is implicitly 0
    nodes: Vec<QuadTreeNode>,

    #[allow(unused)]
    kind: DataSetDataKind,

    #[allow(unused)]
    coordinate_system: DataSetCoordinates,
}

impl QuadTree {
    pub(crate) fn from_json(_base_directory: &Path, json: &JsonValue) -> Fallible<Self> {
        let kind = DataSetDataKind::from_name(json["kind"].as_str().expect("string"))?;
        let coordinate_system =
            DataSetCoordinates::from_name(json["coordinates"].as_str().expect("string"))?;

        let root_json = &json["root"];
        // let path = base_directory.to_owned() + Path::new(root_json["path"].as_str().expect("a string"));
        let scale = root_json["scale"].as_i32().expect("an integer");
        let root = QuadTreeNode {
            scale,
            children: [0; 4],
        };

        Ok(Self {
            nodes: vec![root],
            kind,
            coordinate_system,
        })
    }

    pub(crate) fn note_required(&mut self, _grat: &Graticule<GeoCenter>) {}
}
