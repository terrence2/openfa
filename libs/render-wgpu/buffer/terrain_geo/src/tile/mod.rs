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
mod manager;
mod quad_tree;

pub(crate) use manager::TileManager;
pub(crate) use quad_tree::QuadTree;

use failure::{bail, Fallible};

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum DataSetCoordinates {
    Spherical,
    CartesianPolar,
}

impl DataSetCoordinates {
    pub fn name(&self) -> String {
        match self {
            Self::Spherical => "spherical",
            Self::CartesianPolar => "cartesian_polar",
        }
        .to_owned()
    }

    pub fn from_name(name: &str) -> Fallible<Self> {
        Ok(match name {
            "spherical" => Self::Spherical,
            "cartesian_polar" => Self::CartesianPolar,
            _ => bail!("not a valid data set kind"),
        })
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum DataSetDataKind {
    Color,
    Normal,
    Height,
}

impl DataSetDataKind {
    pub fn name(&self) -> String {
        match self {
            Self::Color => "color",
            Self::Normal => "normal",
            Self::Height => "height",
        }
        .to_owned()
    }

    pub fn from_name(name: &str) -> Fallible<Self> {
        Ok(match name {
            "color" => Self::Color,
            "normal" => Self::Normal,
            "height" => Self::Height,
            _ => bail!("not a valid data set kind"),
        })
    }
}
