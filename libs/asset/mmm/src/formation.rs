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
use absolute_unit::{feet, Feet, Length};
use anyhow::{bail, Result};

#[derive(Copy, Clone, Debug)]
pub enum FormationControl {
    None = 0,
    Loose = 1,
    Medium = 2,
    Tight = 3,
}

impl FormationControl {
    pub fn from_u8(v: u8) -> Result<Self> {
        Ok(match v {
            0 => Self::None,
            1 => Self::Loose,
            2 => Self::Medium,
            3 => Self::Tight,
            _ => bail!("invalid formation control specified"),
        })
    }
}

#[derive(Copy, Clone, Debug)]
pub enum FormationKind {
    Echelon = 0,
    Abreast = 1,
    Astern = 2,
}

impl FormationKind {
    pub fn from_u8(v: u8) -> Result<Self> {
        Ok(match v {
            0 => Self::Echelon,
            1 => Self::Abreast,
            2 => Self::Astern,
            _ => bail!("invalid formation kind specified"),
        })
    }
}

#[derive(Clone, Debug)]
pub struct WingFormation {
    // How aggressively should the AI keep exactly to the formation.
    control: FormationControl,

    // What arrangement this formation takes.
    kind: FormationKind,

    // Usually 512, 2048, etc. Probably feet.
    horizontal_separation: Length<Feet>,

    // Negative for "Low", positive for "High", 0 for "Level" formation.
    vertical_separation: Length<Feet>,
}

impl WingFormation {
    pub(crate) fn from_tokens<'a, 'b, I: Iterator<Item = &'a str>>(
        tokens: &'b mut I,
    ) -> Result<WingFormation>
    where
        'a: 'b,
    {
        let raw_control = str::parse::<u8>(tokens.next().expect("wng control"))?;
        let raw_kind = str::parse::<u8>(tokens.next().expect("wng formation"))?;
        let horizontal_separation = feet!(str::parse::<i32>(
            tokens.next().expect("wng horizontal sep")
        )?);
        let vertical_separation =
            feet!(str::parse::<i32>(tokens.next().expect("wng vertical sep"))?);
        Ok(Self {
            control: FormationControl::from_u8(raw_control)?,
            kind: FormationKind::from_u8(raw_kind)?,
            horizontal_separation,
            vertical_separation,
        })
    }

    pub fn control(&self) -> FormationControl {
        self.control
    }

    pub fn kind(&self) -> FormationKind {
        self.kind
    }

    pub fn horizontal_separation(&self) -> Length<Feet> {
        self.horizontal_separation
    }

    pub fn vertical_separation(&self) -> Length<Feet> {
        self.vertical_separation
    }
}
