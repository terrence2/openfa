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
use anyhow::Result;
use nitrous::{inject_nitrous_resource, method, NitrousResource};
use runtime::{Extension, Runtime};

#[derive(Debug, Default, NitrousResource)]
pub struct Player;

impl Extension for Player {
    fn init(runtime: &mut Runtime) -> Result<()> {
        let player = Self::new();
        runtime.insert_named_resource("player", player);
        Ok(())
    }
}

#[inject_nitrous_resource]
impl Player {
    fn new() -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {}
}
