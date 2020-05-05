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
pub use legion::entity::Entity;
pub use universe::component::{Rotation, Scale, Transform};

use failure::Fallible;
use legion::prelude::*;
use lib::Library;
use nalgebra::{Point3, UnitQuaternion};
use pal::Palette;
use physical_constants::{FEET_TO_DAM, FEET_TO_HM_32, FEET_TO_KM, FEET_TO_M};
use shape_chunk::{ChunkPart, ShapeId};
use shape_instance::{
    ShapeFlagBuffer, ShapeRef, ShapeSlot, ShapeState, ShapeTransformBuffer, ShapeXformBuffer,
    SlotId,
};
use std::{sync::Arc, time::Instant};

pub struct Galaxy {
    start: Instant,

    _legion_universe: Universe,
    legion_world: World,

    // Resources
    lib: Arc<Box<Library>>,
    palette: Arc<Palette>,
}

impl Galaxy {
    pub fn new(lib: Arc<Box<Library>>) -> Fallible<Self> {
        let legion_universe = Universe::new();
        let legion_world = legion_universe.create_world();

        Ok(Self {
            start: Instant::now(),
            _legion_universe: legion_universe,
            legion_world,
            palette: Arc::new(Palette::from_bytes(&lib.load("PALETTE.PAL")?)?),
            lib,
        })
    }

    pub fn world(&self) -> &World {
        &self.legion_world
    }

    pub fn world_mut(&mut self) -> &mut World {
        &mut self.legion_world
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

    pub fn start(&self) -> &Instant {
        &self.start
    }

    pub fn start_owned(&self) -> Instant {
        self.start
    }

    pub fn create_building(
        &mut self,
        slot_id: SlotId,
        shape_id: ShapeId,
        part: &ChunkPart,
        scale: f32,
        position: Point3<f32>,
        rotation: &UnitQuaternion<f32>,
    ) -> Fallible<Entity> {
        let widget_ref = part.widgets();
        let widgets = widget_ref.read().unwrap();
        let entities = self.legion_world.insert(
            (),
            vec![(
                Transform::new(position.coords),
                Rotation::new(*rotation),
                Scale::new(/*SHAPE_UNIT_TO_FEET */ scale * FEET_TO_HM_32),
                ShapeRef::new(shape_id),
                ShapeSlot::new(slot_id),
                ShapeState::new(widgets.errata()),
                ShapeTransformBuffer::default(),
                ShapeFlagBuffer::default(),
            )],
        );
        let entity = entities[0];
        if widgets.errata().has_xform_animation {
            self.legion_world
                .add_component(entity, ShapeXformBuffer::default());
        }
        Ok(entity)
    }

    /*
    pub fn create_ground_mover(
        &mut self,
        slot_id: SlotId,
        shape_id: ShapeId,
        position: Point3<f32>,
    ) -> Fallible<Entity> {
        Ok(self
            .ecs
            .create_entity()
            .with(Transform::new(position, UnitQuaternion::identity()))
            .with(WheeledDynamics::new())
            .with(ShapeSlot::new(slot_id, shape_id))
            .build())
    }

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

    pub fn destroy_entity(&mut self, entity: Entity) -> bool {
        self.legion_world.delete(entity)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use omnilib::OmniLib;

    #[test]
    fn test_it_works() -> Fallible<()> {
        let omni = OmniLib::new_for_test_in_games(&["FA"])?;
        let _universe = Galaxy::new(omni.library("FA"))?;
        Ok(())
    }
}
