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
pub mod system;

pub use crate::component::{flight_dynamics::FlightDynamics, wheeled_dynamics::WheeledDynamics};
pub use specs::Entity;
pub use universe_base::{component::Transform, FEET_TO_DAM, FEET_TO_HM, FEET_TO_KM, FEET_TO_M};

use failure::Fallible;
use lib::Library;
use nalgebra::{convert, Point3};
use pal::Palette;
use shape_chunk::{ChunkPart, ShapeId};
use shape_instance::{
    ShapeComponent, ShapeFlagBuffer, ShapeInstanceBuffer, ShapeTransformBuffer, ShapeXformBuffer,
    SlotId,
};
use specs::{Builder, Dispatcher, World, WorldExt};
use std::sync::Arc;

pub struct Universe {
    pub ecs: World,

    // Resources
    lib: Arc<Box<Library>>,
    palette: Arc<Palette>,
}

impl Universe {
    pub fn new(lib: Arc<Box<Library>>) -> Fallible<Self> {
        let mut ecs = World::new();
        ecs.register::<FlightDynamics>();
        ecs.register::<WheeledDynamics>();
        ecs.register::<Transform>();
        ShapeInstanceBuffer::register_components(&mut ecs);

        Ok(Self {
            ecs,
            palette: Arc::new(Palette::from_bytes(&lib.load("PALETTE.PAL")?)?),
            lib,
        })
    }

    pub fn run(&mut self, dispatcher: &mut Dispatcher) {
        dispatcher.dispatch(&self.ecs);
        self.ecs.maintain();
    }

    pub fn library(&self) -> &Library {
        &self.lib
    }

    pub fn library_owned(&self) -> Arc<Box<Library>> {
        self.lib.clone()
    }

    pub fn palette(&self) -> &Palette {
        &self.palette
    }

    pub fn create_building(
        &mut self,
        slot_id: SlotId,
        shape_id: ShapeId,
        part: &ChunkPart,
        position: Point3<f32>,
    ) -> Fallible<Entity> {
        let widget_ref = part.widgets();
        let widgets = widget_ref.read().unwrap();
        Ok(self
            .ecs
            .create_entity()
            .with(Transform::new(convert(position)))
            .with(ShapeComponent::new(slot_id, shape_id))
            .with(ShapeTransformBuffer::new())
            .with(ShapeFlagBuffer::new(widgets.errata()))
            .with(ShapeXformBuffer::new(shape_id, part.widgets()))
            .build())
    }

    pub fn create_ground_mover(
        &mut self,
        slot_id: SlotId,
        shape_id: ShapeId,
        position: Point3<f64>,
    ) -> Fallible<Entity> {
        Ok(self
            .ecs
            .create_entity()
            .with(Transform::new(position))
            .with(WheeledDynamics::new())
            .with(ShapeComponent::new(slot_id, shape_id))
            .build())
    }

    /*
    pub fn create_flyer(
        &mut self,
        shape_id: ShapeId,
        position: Point3<f64>,
        part: &ChunkPart,
    ) -> Fallible<Entity> {
        //let slot_id = self.shape_renderer.chunk_manager().reserve_slot()?;

        let widget_ref = part.widgets();
        let widgets = widget_ref.read().unwrap();
        Ok(self
            .ecs
            .create_entity()
            .with(Transform::new(position))
            .with(WheeledDynamics::new())
            .with(FlightDynamics::new())
            .with(ShapeMesh::new(shape_id))
            .with(ShapeMeshTransformBuffer::new())
            .with(ShapeMeshFlagBuffer::new(widgets.errata()))
            .with(ShapeMeshXformBuffer::new(shape_id, part.widgets()))
            .build())
    }
    */

    pub fn destroy_entity(&mut self, entity: Entity) -> Fallible<()> {
        Ok(self.ecs.delete_entity(entity)?)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use omnilib::OmniLib;
    //use shape_chunk::{DrawSelection, OpenChunk};
    use window::{GraphicsConfigBuilder, GraphicsWindow};

    #[test]
    fn test_it_works() -> Fallible<()> {
        let omni = OmniLib::new_for_test_in_games(&["FA"])?;
        let mut world = Universe::new(omni.library("FA"))?;
        /*
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
        */
        Ok(())
    }
}
