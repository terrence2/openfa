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
//pub use legion::{entity::Entity, world::EntityStore};
pub use universe::component::{Rotation, Scale, Transform};

use absolute_unit::{feet, meters};
use anyhow::Result;
use catalog::Catalog;
use geodesy::{GeoSurface, Graticule};
use legion::*;
use nalgebra::UnitQuaternion;
use pal::Palette;
use shape_chunk::{ChunkPart, ShapeId};
use shape_instance::{
    ShapeFlagBuffer, ShapeRef, ShapeSlot, ShapeState, ShapeTransformBuffer, ShapeXformBuffer,
    SlotId,
};
use std::{sync::Arc, time::Instant};

pub struct Galaxy {
    start_time: Instant,

    legion_world: World,

    // Resources
    //lib: Arc<Box<Library>>,
    palette: Arc<Palette>,
}

impl Galaxy {
    pub fn new(catalog: &Catalog) -> Result<Self> {
        let legion_world = World::new(Default::default());

        Ok(Self {
            start_time: Instant::now(),
            legion_world,
            palette: Arc::new(Palette::from_bytes(
                &catalog.read_name_sync("PALETTE.PAL")?,
            )?),
            //lib,
        })
    }

    pub fn world(&self) -> &World {
        &self.legion_world
    }

    pub fn world_mut(&mut self) -> &mut World {
        &mut self.legion_world
    }

    /*
    pub fn library(&self) -> &Library {
        &self.lib
    }

    pub fn library_owned(&self) -> Arc<Box<Library>> {
        self.lib.clone()
    }
     */

    pub fn palette(&self) -> &Palette {
        &self.palette
    }

    pub fn start_time(&self) -> &Instant {
        &self.start_time
    }

    pub fn start_time_owned(&self) -> Instant {
        self.start_time
    }

    pub fn create_building(
        &mut self,
        slot_id: SlotId,
        shape_id: ShapeId,
        part: &ChunkPart,
        scale: f32,
        position: Graticule<GeoSurface>,
        rotation: &UnitQuaternion<f32>,
    ) -> Result<Entity> {
        let widget_ref = part.widgets();
        let widgets = widget_ref.read().unwrap();
        let entity = self.legion_world.push((
            Transform::new(position),
            Rotation::new(*rotation),
            Scale::new(
                /*SHAPE_UNIT_TO_FEET */ scale * feet!(meters!(1.0)).f32(),
            ),
            ShapeRef::new(shape_id),
            ShapeSlot::new(slot_id),
            ShapeState::new(widgets.errata()),
            ShapeTransformBuffer::default(),
            ShapeFlagBuffer::default(),
        ));
        if widgets.errata().has_xform_animation {
            self.legion_world
                .entry(entity)
                .expect("just created")
                .add_component(ShapeXformBuffer::default());
        }
        Ok(entity)
    }

    /*
    pub fn create_ground_mover(
        &mut self,
        slot_id: SlotId,
        shape_id: ShapeId,
        position: Point3<f32>,
    ) -> Result<Entity> {
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
    ) -> Result<Entity> {
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
        self.legion_world.remove(entity)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use lib::CatalogBuilder;

    #[test]
    fn test_it_works() -> Result<()> {
        // Note: rely on uniqueness of PALETTE.PAL to give us every game.
        let (mut catalog, inputs) =
            CatalogBuilder::build_and_select(&["*:PALETTE.PAL".to_owned()])?;
        for &fid in &inputs {
            let label = catalog.file_label(fid)?;
            catalog.set_default_label(&label);
            let game = label.split(':').last().unwrap();
            let meta = catalog.stat_sync(fid)?;
            println!(
                "At: {}:{:13} @ {}",
                game,
                meta.name(),
                meta.path()
                    .map(|v| v.to_string_lossy())
                    .unwrap_or_else(|| "<none>".into())
            );
            //let _universe = Galaxy::new(omni.library("FA"))?;
        }

        Ok(())
    }
}
