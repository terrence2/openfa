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
use failure::Fallible;
use lib::Library;
use nalgebra::{Point3, UnitQuaternion, Vector3};
use pal::Palette;
use specs::{Builder, Component, VecStorage, World as SpecsWorld, WorldExt};

pub use specs::Entity;

// Components
struct Position(Point3<f64>);
impl Component for Position {
    type Storage = VecStorage<Self>;
}

struct Rotation(UnitQuaternion<f64>);
impl Component for Rotation {
    type Storage = VecStorage<Self>;
}

struct Velocity(Vector3<f64>);
impl Component for Velocity {
    type Storage = VecStorage<Self>;
}

// Entities / World
pub struct World {
    ecs: SpecsWorld,
    lib: Library,
    system_palette: Palette,
}

impl World {
    pub fn new(lib: Library) -> Fallible<Self> {
        let mut ecs = SpecsWorld::new();
        ecs.register::<Position>();
        ecs.register::<Rotation>();
        ecs.register::<Velocity>();

        Ok(Self {
            ecs,
            system_palette: Palette::from_bytes(&lib.load("PALETTE.PAL")?)?,
            lib,
        })
    }

    pub fn create_ground_mover(&mut self, position: Point3<f64>) -> Entity {
        self.ecs
            .create_entity()
            .with(Position(position))
            .with(Rotation(UnitQuaternion::identity()))
            .with(Velocity(Vector3::zeros()))
            .build()
    }

    pub fn destroy_entity(&mut self, entity: Entity) -> Fallible<()> {
        Ok(self.ecs.delete_entity(entity)?)
    }
}
