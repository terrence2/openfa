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
use absolute_unit::{degrees, meters, Meters};
use anyhow::{bail, Result};
use bevy_ecs::prelude::*;
use camera::ArcBallController;
use catalog::Catalog;
use geodesy::{Cartesian, GeoCenter, GeoSurface, Graticule};
use gpu::Gpu;
use lib::from_dos_string;
use lib::Libs;
use log::warn;
use measure::WorldSpaceFrame;
use mmm::{Mission, MissionMap};
use nitrous::{inject_nitrous_resource, make_symbol, method, HeapMut, NitrousResource, Value};
use pal::Palette;
use parking_lot::RwLock;
use runtime::{Extension, Runtime};
use shape::{DrawSelection, ShapeBuffer};
use std::{collections::HashSet, sync::Arc};
use t2_terrain::{T2Adjustment, T2TerrainBuffer};
use terrain::{TerrainBuffer, TileSet};
use xt::{TypeManager, TypeRef};

#[derive(Debug, Default, NitrousResource)]
pub struct Game {}

impl Extension for Game {
    fn init(runtime: &mut Runtime) -> Result<()> {
        /*
        let t2_adjustment = T2Adjustment::default();
        let mut t2_terrain = T2TileSet::new(
            system.read().t2_adjustment(),
            &terrain.read(),
            &globals.read(),
            &gpu.read(),
        )?;
        runtime
            .resource_mut::<TerrainBuffer>()
            .add_tile_set(Box::new(t2_terrain) as Box<dyn TileSet>);
         */
        let game = Game::new();
        runtime.insert_named_resource("game", game);
        Ok(())
    }
}

#[inject_nitrous_resource]
impl Game {
    fn new() -> Self {
        Game {}
    }

    #[method]
    fn spawn_named_plane(&self, name: &str, pt_filename: &str, mut heap: HeapMut) -> Result<Value> {
        // Sanity checks
        let pt_filename = pt_filename.to_uppercase();
        if !pt_filename.ends_with(".PT") {
            bail!("PT files must end in PT");
        }

        // Find a good place in the world to spawn
        let target = if let Some(arcball) = heap.maybe_get_named::<ArcBallController>("player") {
            arcball.target()
        } else if let Some(frame) = heap.maybe_get_named::<WorldSpaceFrame>("player") {
            let p = frame.position().vec64() + (frame.forward() * 100.);
            let target = Cartesian::<GeoCenter, Meters>::from(p);
            Graticule::<GeoSurface>::from(Graticule::<GeoCenter>::from(target))
        } else {
            Graticule::<GeoSurface>::new(degrees!(0f32), degrees!(0f32), meters!(10f32))
        };

        // Load and parse the PT file
        heap.resource_scope(|heap, mut shapes: Mut<ShapeBuffer>| {
            let catalog = heap.resource::<Arc<RwLock<Catalog>>>();
            let xt = heap
                .resource::<TypeManager>()
                .load(&pt_filename, &catalog.read())?;
            let palette = Palette::from_bytes(&catalog.read().read_name("PALETTE.PAL")?.as_ref())?;
            shapes.upload_shapes(
                &palette,
                &[xt.ot().shape.clone().unwrap()],
                &catalog.read(),
                heap.resource::<Gpu>(),
            )
        })?;
        // let (_shape_id, _slot_id) =
        //     heap.resource_scope(|heap, mut shapes: Mut<ShapeInstanceBuffer>| {
        //         let gpu = heap.resource::<Gpu>();
        //         // shapes.upload_and_allocate_slot(
        //         //     xt.ot().shape.as_ref().expect("a shape file"),
        //         //     DrawSelection::NormalModel,
        //         //     &catalog.read(),
        //         //     gpu,
        //         //     heap.resource::<UploadTracker>(),
        //         // )
        //         bail!("unimplemnted")
        //     })?;

        // Build the entity
        let entity = heap.spawn_named(name)?;

        Ok(Value::True())
    }

    #[method]
    fn load_map(&self, name: &str, mut heap: HeapMut) -> Result<()> {
        if name.starts_with('~') || name.starts_with('$') {
            // FIXME: log message to terminal
            bail!("cannot load {name}; it is a template (note the ~ or $ prefix)");
        }

        // FIXME: can we print this in a useful way?
        println!("Loading {}...", name);

        // FIXME: do something smarter with system palette!
        let system_palette = {
            let libs = heap.resource::<Libs>();
            Palette::from_bytes(&libs.read_name("PALETTE.PAL")?.as_ref())?
        };

        let mm = {
            let libs = heap.resource::<Libs>();
            let mm_content = from_dos_string(libs.read_name(name)?);
            MissionMap::from_str(
                mm_content.as_ref(),
                heap.resource::<TypeManager>(),
                libs.catalog(),
            )?
        };

        let tile_set = heap.resource_scope(|heap, mut t2_terrain: Mut<T2TerrainBuffer>| {
            let catalog = heap.resource::<Arc<RwLock<Catalog>>>();
            let cat = catalog.read();
            t2_terrain.add_map(&system_palette, &mm, &cat, &heap.resource::<Gpu>())
        })?;
        heap.spawn_named(make_symbol(name))?
            .insert_scriptable(tile_set)?;

        // Accumulate and load all shapes on the GPU before spawning entities with those shapes.
        let mut shape_names = HashSet::new();
        for info in mm.objects() {
            if let Some(shape_file) = info.xt().ot().shape.as_ref() {
                shape_names.insert(shape_file.to_owned());
            }
        }
        let shape_names = shape_names.iter().collect::<Vec<_>>();
        let preloaded_shape_ids = heap.resource_scope(|heap, mut shapes: Mut<ShapeBuffer>| {
            let catalog = heap.resource::<Arc<RwLock<Catalog>>>();
            let cat = catalog.read();
            shapes.upload_shapes(&system_palette, &shape_names, &cat, heap.resource::<Gpu>())
        })?;

        // Re-visit all objects and instantiate instances
        // TODO: only worried about shape bits for now, not the rest of the entity.
        /*
        for info in mm.objects() {
            let (shape_ids, shape_inst, shape_comps) = if let Some(shape_file) = info.xt().ot().shape.as_ref() {
                let shape_ids = preloaded_shape_ids
                    .get(shape_file)
                    .expect("preloaded shape");
                let (inst, comps) = runtime.resource_scope(|heap, mut shapes: Mut<ShapeBuffer>| {
                    shapes.create_instance(shape_ids.normal(), runtime.resource::<Gpu>());
                })?;
            }
        }
         */

        Ok(())
    }

    /*

    pub fn create_building(
        &mut self,
        slot_id: SlotId,
        shape_id: ShapeId,
        part: &ChunkPart,
        scale: f32,
        position: Graticule<GeoSurface>,
        rotation: &UnitQuaternion<f32>,
    ) -> Result<Entity> {
        // For buildings we need to adjust the frame for "up" to be relative
        // to the position when uploading.
        let r_lon =
            UnitQuaternion::from_axis_angle(&Vector3::y_axis(), -position.lon::<Radians>().f32());
        let lat_axis = r_lon * Vector3::x_axis();
        let q_lat = UnitQuaternion::from_axis_angle(
            &lat_axis,
            radians!(degrees!(90) - position.lat::<Degrees>()).f32(),
        );
        let rotation: UnitQuaternion<f32> = q_lat
            * UnitQuaternion::from_axis_angle(
                &Vector3::y_axis(),
                (-position.lon::<Radians>()).f32(),
            )
            * rotation;

        // vec4 r_lon = quat_from_axis_angle(vec3(0, 1, 0), latlon.y);
        // vec3 lat_axis = quat_rotate(r_lon, vec3(1, 0, 0)).xyz;
        // vec4 r_lat = quat_from_axis_angle(lat_axis, PI / 2.0 - latlon.x);
        // vec3 ground_normal_w = quat_rotate(r_lat, quat_rotate(r_lon, ground_normal_local).xyz).xyz;

        let widget_ref = part.widgets();
        let widgets = widget_ref.read();
        let entity = self.legion_world.push((
            Transform::new(position),
            Rotation::new(rotation),
            Scale::new(
                /* SHAPE_UNIT_TO_FEET */ scale * meters!(feet!(1.0)).f32(),
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
     */
}
