// This file is part of Nitrogen.
//
// Nitrogen is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Nitrogen is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with Nitrogen.  If not, see <http://www.gnu.org/licenses/>.
use failure::{bail, Fallible};
use std::path::PathBuf;
use winit::{
    dpi::{LogicalPosition, LogicalSize},
    event::DeviceId,
};

#[derive(Clone, Debug)]
pub enum CommandArg {
    None,
    Boolean(bool),
    Float(f64),
    Path(PathBuf),
    Device(DeviceId),
    Displacement((f64, f64)),
}

impl From<DeviceId> for CommandArg {
    fn from(v: DeviceId) -> Self {
        CommandArg::Device(v)
    }
}

impl From<(f64, f64)> for CommandArg {
    fn from(v: (f64, f64)) -> Self {
        CommandArg::Displacement((v.0, v.1))
    }
}

impl From<(f32, f32)> for CommandArg {
    fn from(v: (f32, f32)) -> Self {
        CommandArg::Displacement((f64::from(v.0), f64::from(v.1)))
    }
}

impl From<f64> for CommandArg {
    fn from(v: f64) -> Self {
        CommandArg::Float(v)
    }
}

impl From<LogicalSize> for CommandArg {
    fn from(v: LogicalSize) -> Self {
        CommandArg::Displacement((v.width, v.height))
    }
}

impl From<LogicalPosition> for CommandArg {
    fn from(v: LogicalPosition) -> Self {
        CommandArg::Displacement((v.x, v.y))
    }
}

impl From<PathBuf> for CommandArg {
    fn from(v: PathBuf) -> Self {
        CommandArg::Path(v)
    }
}

impl From<bool> for CommandArg {
    fn from(v: bool) -> Self {
        CommandArg::Boolean(v)
    }
}

#[derive(Clone, Debug)]
pub struct Command {
    pub name: String,
    pub arg: CommandArg,
}

impl Command {
    pub fn from_string(name: String) -> Self {
        Self {
            name,
            arg: CommandArg::None,
        }
    }

    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            arg: CommandArg::None,
        }
    }

    pub fn with_arg(name: &str, arg: CommandArg) -> Self {
        Self {
            name: name.to_owned(),
            arg,
        }
    }

    pub fn boolean(&self) -> Fallible<bool> {
        match self.arg {
            CommandArg::Boolean(v) => Ok(v),
            _ => bail!("not a boolean argument"),
        }
    }

    pub fn float(&self) -> Fallible<f64> {
        match self.arg {
            CommandArg::Float(v) => Ok(v),
            _ => bail!("not a float argument"),
        }
    }

    pub fn path(&self) -> Fallible<PathBuf> {
        match &self.arg {
            CommandArg::Path(v) => Ok(v.to_path_buf()),
            _ => bail!("not a path argument"),
        }
    }

    pub fn displacement(&self) -> Fallible<(f64, f64)> {
        match self.arg {
            CommandArg::Displacement(v) => Ok(v),
            _ => bail!("not a displacement argument"),
        }
    }

    pub fn device(&self) -> Fallible<DeviceId> {
        match self.arg {
            CommandArg::Device(v) => Ok(v),
            _ => bail!("not a device argument"),
        }
    }
}
