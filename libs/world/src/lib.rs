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
pub mod component;

use crate::component::*;
use failure::Fallible;
use lib::Library;
use nalgebra::Point3;
use pal::Palette;
use shape_chunk::{ChunkPart, ShapeId};
use specs::{Builder, Dispatcher, World as SpecsWorld, WorldExt};
use std::sync::{Arc, RwLock};

pub use specs::Entity;

pub struct World {
    ecs: RwLock<SpecsWorld>,

    // Resources
    lib: Arc<Box<Library>>,
    palette: Arc<Palette>,
}

impl World {
    pub fn new(lib: Arc<Box<Library>>) -> Fallible<Self> {
        let mut ecs = SpecsWorld::new();
        ecs.register::<FlightDynamics>();
        ecs.register::<WheeledDynamics>();
        ecs.register::<ShapeMesh>();
        ecs.register::<ShapeMeshTransformBuffer>();
        ecs.register::<ShapeMeshFlagBuffer>();
        ecs.register::<Transform>();

        Ok(Self {
            ecs: RwLock::new(ecs),
            palette: Arc::new(Palette::from_bytes(&lib.load("PALETTE.PAL")?)?),
            lib,
        })
    }

    pub fn run(&self, dispatcher: &mut Dispatcher) {
        let mut ecs = self.ecs.write().unwrap();
        dispatcher.dispatch(&mut ecs);
        ecs.maintain();
    }

    pub fn library(&self) -> &Library {
        &self.lib
    }

    pub fn system_palette(&self) -> &Palette {
        &self.palette
    }

    pub fn create_ground_mover(
        &self,
        shape_id: ShapeId,
        position: Point3<f64>,
    ) -> Fallible<Entity> {
        Ok(self
            .ecs
            .write()
            .unwrap()
            .create_entity()
            .with(Transform::new(position))
            .with(WheeledDynamics::new())
            .with(ShapeMesh::new(shape_id))
            .build())
    }

    pub fn create_flyer(
        &self,
        shape_id: ShapeId,
        position: Point3<f64>,
        part: &ChunkPart,
    ) -> Fallible<Entity> {
        let errata = part.widgets().errata();
        Ok(self
            .ecs
            .write()
            .unwrap()
            .create_entity()
            .with(Transform::new(position))
            .with(WheeledDynamics::new())
            .with(FlightDynamics::new())
            .with(ShapeMesh::new(shape_id))
            .with(ShapeMeshTransformBuffer::new())
            .with(ShapeMeshFlagBuffer::new(&errata))
            .build())
    }

    pub fn destroy_entity(&self, entity: Entity) -> Fallible<()> {
        Ok(self.ecs.write().unwrap().delete_entity(entity)?)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use omnilib::OmniLib;
    use shape_chunk::{ClosedChunk, DrawSelection, OpenChunk};
    use window::{GraphicsConfigBuilder, GraphicsWindow};

    #[test]
    fn test_it_works() -> Fallible<()> {
        let omni = OmniLib::new_for_test_in_games(&["FA"])?;
        let mut world = World::new(omni.library("FA"))?;
        let window = GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?;
        let mut upload = OpenChunk::new(&window)?;
        let shape_id = upload.upload_shape(
            "T80.SH",
            DrawSelection::NormalModel,
            world.system_palette(),
            world.library(),
            &window,
        )?;
        let ent = world.create_ground_mover(shape_id, Point3::new(0f64, 0f64, 0f64))?;
        world.destroy_entity(ent)?;
        Ok(())
    }
}
