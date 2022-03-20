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
use absolute_unit::{degrees, feet, meters, Feet, Meters};
use anyhow::{anyhow, bail, Result};
use bevy_ecs::prelude::*;
use camera::ArcBallController;
use camera::ScreenCamera;
use geodesy::{Cartesian, GeoCenter, GeoSurface, Graticule};
use geometry::{intersect::sphere_vs_ray, Ray, Sphere};
use gpu::Gpu;
use lib::{from_dos_string, Libs};
use measure::WorldSpaceFrame;
use mmm::{Mission, MissionMap, ObjectInfo};
use nitrous::EntityName;
use nitrous::{inject_nitrous_resource, make_symbol, method, HeapMut, NitrousResource, Value};
use once_cell::sync::Lazy;
use ordered_float::OrderedFloat;
use parking_lot::RwLock;
use runtime::{Extension, Runtime};
use shape::{ShapeBuffer, ShapeId, ShapeMetadata, ShapeScale};
use std::{
    borrow::Borrow,
    collections::{HashMap, HashSet},
    sync::Arc,
};
use t2_terrain::{T2TerrainBuffer, T2TileSet};
use xt::TypeManager;

static SCALE_OVERRIDE: Lazy<HashMap<&'static str, i32>> = Lazy::new(|| {
    let m: HashMap<&str, i32> = HashMap::new();
    m
});

#[derive(Debug, Default, NitrousResource)]
pub struct Game {}

impl Extension for Game {
    fn init(runtime: &mut Runtime) -> Result<()> {
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
    fn boresight(&self, pressed: bool, mut heap: HeapMut) {
        if !pressed {
            return;
        }

        heap.resource_scope(|mut heap, camera: Mut<ScreenCamera>| {
            heap.resource_scope(|mut heap, shapes: Mut<ShapeBuffer>| {
                let mut intersects = Vec::new();
                let view_ray = Ray::new(
                    camera.position::<Meters>().point64(),
                    camera.forward().to_owned(),
                );
                for (name, shape_id, frame, scale) in heap
                    .query::<(&EntityName, &ShapeId, &WorldSpaceFrame, &ShapeScale)>()
                    .iter(heap.world())
                {
                    let metadata = shapes.metadata(*shape_id);
                    if let Some(intersect) = metadata.read().extent().intersect_ray(
                        frame.position().point64(),
                        scale.scale() as f64,
                        &view_ray,
                    ) {
                        intersects.push((
                            name.name().to_owned(),
                            OrderedFloat(intersect.coords.magnitude()),
                        ));
                    };
                }
                intersects.sort_by_key(|(_, d)| -*d);
                for (name, d) in intersects {
                    println!("{}: at {:?}", name, d);
                }
            })
        });
    }

    // fn spawn_inner(&self, instances: &[(&str, &str)], mut heap: HeapMut) -> Result<()> {}

    fn frame_for_interactive(heap: &mut HeapMut) -> WorldSpaceFrame {
        let target = if let Some(arcball) = heap.maybe_get_named::<ArcBallController>("player") {
            arcball.target()
        } else if let Some(frame) = heap.maybe_get_named::<WorldSpaceFrame>("player") {
            let p = frame.position().vec64() + (frame.basis().forward * 100.);
            let target = Cartesian::<GeoCenter, Meters>::from(p);
            Graticule::<GeoSurface>::from(Graticule::<GeoCenter>::from(target))
        } else {
            Graticule::<GeoSurface>::new(degrees!(0f32), degrees!(0f32), meters!(10f32))
        };
        WorldSpaceFrame::from_graticule(
            target,
            Graticule::new(degrees!(0), degrees!(0), meters!(1)),
        )
    }

    #[method]
    fn spawn(&self, name: &str, filename: &str, mut heap: HeapMut) -> Result<Value> {
        // Please don't force me to type in all-caps all the time.
        let filename = filename.to_uppercase();

        // Make sure we can load the thing, before creating the entity.
        let xt = {
            let libs = heap.resource::<Libs>();
            heap.resource::<TypeManager>()
                .load(&filename, libs.catalog())
        }?;

        let scale: Option<&i32> = SCALE_OVERRIDE.get(<String as Borrow<str>>::borrow(&filename));
        let scale = *scale.unwrap_or(&1i32) as f32;

        let frame = Self::frame_for_interactive(&mut heap);
        let id = heap
            .spawn_named(name)?
            .insert(xt.clone())
            .insert(frame)
            .insert(ShapeScale::new(scale))
            .id();

        // Instantiate the shape, if it has one.
        if let Some(shape_name) = xt.ot().shape.as_ref() {
            heap.resource_scope(|mut heap, mut shapes: Mut<ShapeBuffer>| {
                heap.resource_scope(|mut heap, gpu: Mut<Gpu>| {
                    heap.resource_scope(|mut heap, libs: Mut<Libs>| {
                        let entity = heap.named_entity_mut(id);
                        shapes.instantiate_one(
                            entity,
                            shape_name,
                            libs.palette(),
                            libs.catalog(),
                            &gpu,
                        )
                    })
                })
            })?;
        }

        // FIXME: we need all the things that are in an mmm ObjectInfo here...

        Ok(Value::True())
    }

    fn load_mmm_common<'a, I>(
        &self,
        map_name: &str,
        mission_map: &MissionMap,
        objects_pass_1: I,
        objects_pass_2: I,
        heap: &mut HeapMut,
    ) -> Result<()>
    where
        I: Iterator<Item = &'a ObjectInfo>,
    {
        let tile_set = heap.resource_scope(|heap, mut t2_terrain: Mut<T2TerrainBuffer>| {
            let libs = heap.resource::<Libs>();
            t2_terrain.add_map(
                libs.palette(),
                &mission_map,
                libs.catalog(),
                heap.resource::<Gpu>(),
            )
        })?;
        let tile_id = heap
            .spawn_named(make_symbol(map_name))?
            .insert_named(tile_set)?
            .id();

        // Pre-load the shapes into as few chunks as possible.
        let mut shape_names = HashSet::new();
        for info in objects_pass_1 {
            if let Some(shape_file) = info.xt().ot().shape.as_ref() {
                shape_names.insert(shape_file.to_owned());
            }
        }
        let shape_names = shape_names.iter().collect::<Vec<_>>();
        let preloaded_shape_ids = heap.resource_scope(|heap, mut shapes: Mut<ShapeBuffer>| {
            let libs = heap.resource::<Libs>();
            shapes.upload_shapes(
                libs.palette(),
                &shape_names,
                libs.catalog(),
                heap.resource::<Gpu>(),
            )
        })?;

        // Create all entities as we go.
        let game_name = heap.resource::<Libs>().catalog().label().to_owned();
        let mut duplicates: HashMap<String, i32> = HashMap::new();
        for info in objects_pass_2 {
            let base_inst_name = info.name().unwrap_or_else(|| {
                let shape_name = info.xt().ot().shape.clone().unwrap_or_default();
                format!(
                    "{}_{}_{}",
                    game_name,
                    make_symbol(&map_name[0..map_name.len() - 3]),
                    make_symbol(&shape_name[0..shape_name.len() - 3]),
                )
                .to_lowercase()
            });
            let (inst_name, next_count) =
                if let Some(current_count) = duplicates.get(&base_inst_name) {
                    (
                        format!("{}_{}", base_inst_name, current_count + 1),
                        current_count + 1,
                    )
                } else {
                    (base_inst_name.clone(), 0)
                };
            duplicates.insert(base_inst_name, next_count);

            let id = heap.spawn_named(&inst_name)?.id();

            // Load the shape if it has one.
            let metadata = if let Some(shape_file) = info.xt().ot().shape.as_ref() {
                let shape_ids = preloaded_shape_ids
                    .get(shape_file)
                    .ok_or_else(|| anyhow!("failed to load shape"))?;
                heap.resource_scope(|mut heap, mut shapes: Mut<ShapeBuffer>| {
                    heap.resource_scope(|mut heap, gpu: Mut<Gpu>| {
                        let entity = heap.named_entity_mut(id);
                        shapes.instantiate(entity, shape_ids.normal(), &gpu)
                    })
                })?;
                heap.resource::<ShapeBuffer>().metadata(shape_ids.normal())
            } else {
                Arc::new(RwLock::new(ShapeMetadata::non_shape()))
            };

            // let scale: Option<&i32> = SCALE_OVERRIDE.get(<String as Borrow<str>>::borrow(
            //     &info.xt().ot().ot_names.file_name,
            // ));
            // let scale = *scale.unwrap_or(&1i32) as f32;
            let scale = 1.0f32;

            let frame = {
                let tile_mapper = heap.get::<T2TileSet>(tile_id).mapper();
                let offset_from_ground = metadata.read().extent().offset_to_ground() * scale;
                // FIXME: figure out the terrain height here
                let position = tile_mapper.fa2grat(info.position(), feet!(offset_from_ground));

                if info.xt().ot().ot_names.file_name.starts_with("STRIP") {
                    println!(
                        "{}: {},{},{} => {}",
                        info.xt().ot().ot_names.file_name,
                        info.angle().yaw(),
                        info.angle().pitch(),
                        info.angle().roll(),
                        info.angle().facing()
                    );
                }
                WorldSpaceFrame::from_graticule(position, info.angle().facing())
            };

            heap.named_entity_mut(id)
                .insert(frame)
                .insert(ShapeScale::new(scale));
        }

        Ok(())
    }

    #[method]
    fn load_map(&self, name: &str, mut heap: HeapMut) -> Result<()> {
        let name = name.to_uppercase();
        if name.starts_with('~') || name.starts_with('$') {
            // FIXME: log message to terminal
            bail!("cannot load {name}; it is a template (note the ~ or $ prefix)");
        }
        let game_name = heap.resource::<Libs>().catalog().label().to_owned();

        let mm = {
            let libs = heap.resource::<Libs>();
            let mm_content = from_dos_string(libs.read_name(&name)?);
            MissionMap::from_str(
                mm_content.as_ref(),
                heap.resource::<TypeManager>(),
                libs.catalog(),
            )?
        };

        self.load_mmm_common(&name, &mm, mm.objects(), mm.objects(), &mut heap)?;

        Ok(())
    }

    #[method]
    fn load_mission(&self, name: &str, mut heap: HeapMut) -> Result<()> {
        let name = name.to_uppercase();
        if name.starts_with('~') || name.starts_with('$') {
            // FIXME: log message to terminal
            bail!("cannot load {name}; it is a template (note the ~ or $ prefix)");
        }
        let game_name = heap.resource::<Libs>().catalog().label().to_owned();

        // FIXME: can we print this in a useful way?
        println!("Loading {}:{}...", game_name, name);

        let mission = {
            let libs = heap.resource::<Libs>();
            let m_content = from_dos_string(libs.read_name(&name)?);
            Mission::from_str(
                m_content.as_ref(),
                heap.resource::<TypeManager>(),
                libs.catalog(),
            )?
        };

        self.load_mmm_common(
            &name,
            mission.mission_map(),
            mission.all_objects(),
            mission.all_objects(),
            &mut heap,
        )?;

        /*
        let tile_set = heap.resource_scope(|heap, mut t2_terrain: Mut<T2TerrainBuffer>| {
            let libs = heap.resource::<Libs>();
            t2_terrain.add_map(
                libs.palette(),
                mission.mission_map(),
                libs.catalog(),
                heap.resource::<Gpu>(),
            )
        })?;
        let tile_id = heap
            .spawn_named(make_symbol(&name))?
            .insert_named(tile_set)?
            .id();

        // Pre-load the shapes into as few chunks as possible.
        let mut shape_names = HashSet::new();
        for info in mission.all_objects() {
            if let Some(shape_file) = info.xt().ot().shape.as_ref() {
                shape_names.insert(shape_file.to_owned());
            }
        }
        let shape_names = shape_names.iter().collect::<Vec<_>>();
        let preloaded_shape_ids = heap.resource_scope(|heap, mut shapes: Mut<ShapeBuffer>| {
            let libs = heap.resource::<Libs>();
            shapes.upload_shapes(
                libs.palette(),
                &shape_names,
                libs.catalog(),
                heap.resource::<Gpu>(),
            )
        })?;

        // Create all entities as we go.
        let mut duplicates: HashMap<String, i32> = HashMap::new();
        for info in mission.all_objects() {
            let map_name = &name;
            let mut base_inst_name = info.name().unwrap_or_else(|| {
                let shape_name = info.xt().ot().shape.clone().unwrap_or_default();
                format!(
                    "{}_{}_{}",
                    game_name,
                    make_symbol(&map_name[0..map_name.len() - 3]),
                    make_symbol(&shape_name[0..shape_name.len() - 3]),
                )
                .to_lowercase()
            });
            let (inst_name, next_count) =
                if let Some(current_count) = duplicates.get(&base_inst_name) {
                    (
                        format!("{}_{}", base_inst_name, current_count + 1),
                        current_count + 1,
                    )
                } else {
                    (base_inst_name.clone(), 0)
                };
            duplicates.insert(base_inst_name, next_count);

            let id = heap.spawn_named(&inst_name)?.id();

            // Load the shape if it has one.
            let metadata = if let Some(shape_file) = info.xt().ot().shape.as_ref() {
                let shape_ids = preloaded_shape_ids
                    .get(shape_file)
                    .ok_or_else(|| anyhow!("failed to load shape"))?;
                heap.resource_scope(|mut heap, mut shapes: Mut<ShapeBuffer>| {
                    heap.resource_scope(|mut heap, gpu: Mut<Gpu>| {
                        let entity = heap.named_entity_mut(id);
                        shapes.instantiate(entity, shape_ids.normal(), &gpu)
                    })
                })?;
                heap.resource::<ShapeBuffer>().metadata(shape_ids.normal())
            } else {
                Arc::new(RwLock::new(ShapeMetadata::non_shape()))
            };

            // let scale: Option<&i32> = SCALE_OVERRIDE.get(<String as Borrow<str>>::borrow(
            //     &info.xt().ot().ot_names.file_name,
            // ));
            // let scale = *scale.unwrap_or(&1i32) as f32;
            let scale = 1.0f32;

            let frame = {
                let tile_mapper = heap.get::<T2TileSet>(tile_id).mapper();
                let offset_from_ground = metadata.read().extent().offset_to_ground() * scale;
                // FIXME: figure out the terrain height here
                let position = tile_mapper.fa2grat(info.position(), feet!(offset_from_ground));

                if info.xt().ot().ot_names.file_name.starts_with("STRIP") {
                    println!(
                        "{}: {},{},{} => {}",
                        info.xt().ot().ot_names.file_name,
                        info.angle().yaw(),
                        info.angle().pitch(),
                        info.angle().roll(),
                        info.angle().facing()
                    );
                }
                WorldSpaceFrame::from_graticule(position, info.angle().facing())
            };

            heap.named_entity_mut(id)
                .insert(frame)
                .insert(ShapeScale::new(scale));
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
