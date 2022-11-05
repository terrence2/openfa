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
use absolute_unit::{degrees, feet, feet_per_second, kilograms, meters, scalar, Meters};
use anyhow::{anyhow, bail, Result};
use bevy_ecs::prelude::*;
use camera::{ArcBallController, ScreenCamera, ScreenCameraController};
use fa_vehicle::Turbojet;
use flight_dynamics::ClassicFlightModel;
use geodesy::{GeoSurface, Graticule};
use geometry::Ray;
use gpu::Gpu;
use lib::{from_dos_string, Libs};
use marker::EntityMarkers;
use measure::{BodyMotion, WorldSpaceFrame};
use mmm::{Mission, MissionMap, ObjectInfo};
use nitrous::{
    inject_nitrous_resource, make_symbol, method, EntityName, HeapMut, NitrousResource, Value,
};
use once_cell::sync::Lazy;
use ordered_float::OrderedFloat;
use parking_lot::RwLock;
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use runtime::{Extension, PlayerMarker, Runtime};
use shape::{ShapeBuffer, ShapeId, ShapeMetadata, ShapeScale, SlotId};
use std::{
    borrow::Borrow,
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};
use t2_terrain::{T2TerrainBuffer, T2TileSet};
use vehicle::{
    AirbrakeControl, AirbrakeEffector, Airframe, BayControl, BayEffector, FlapsControl,
    FlapsEffector, FuelSystem, FuelTank, FuelTankKind, GearControl, GearEffector, HookControl,
    HookEffector, PitchInceptor, PowerSystem, RollInceptor, ThrottleInceptor, YawInceptor,
};
use xt::TypeManager;

const FEET_TO_METERS: f32 = 1. / 3.28084;

static SCALE_OVERRIDE: Lazy<HashMap<&'static str, i32>> = Lazy::new(|| {
    let mut m: HashMap<&str, i32> = HashMap::new();
    m.insert("STRIP.OT", 4);
    m.insert("STRIP1.OT", 4);
    m.insert("STRIP2.OT", 4);
    m.insert("STRIP3.OT", 4);
    m.insert("STRIP3A.OT", 4);
    m.insert("STRIP4.OT", 4);
    m.insert("STRIP5.OT", 4);
    m.insert("STRIP5A.OT", 4);
    m.insert("STRIP6.OT", 4);
    m.insert("STRIP6A.OT", 4);
    m.insert("STRIP7.OT", 4);
    m.insert("STRIP7A.OT", 4);
    m
});

#[derive(Debug, Default, Component)]
pub struct MissionMarker;

#[derive(Debug, Default, NitrousResource)]
pub struct AssetLoader;

impl Extension for AssetLoader {
    fn init(runtime: &mut Runtime) -> Result<()> {
        let asset_loader = Self::new();
        runtime.insert_named_resource("game", asset_loader);
        Ok(())
    }
}

#[inject_nitrous_resource]
impl AssetLoader {
    fn new() -> Self {
        Self
    }

    #[method]
    fn boresight(&self, mut heap: HeapMut) {
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

    /// The ScreenCameraController is normally attached to an entity named 'camera', which is a
    /// PlayerCameraController which always tracks to entity called 'Player' (and related entities
    /// off of Player, like the target). This method rips the ScreenCameraController off 'camera'
    /// and puts it on 'fallback_camera', which is a free-move ArcBallController with no inherent
    /// attachment to other entities.
    #[method]
    fn detach_camera(&self, mut heap: HeapMut) {
        let fallback_id = heap.entity_by_name("fallback_camera");
        let camera_id = heap.entity_by_name("camera");

        heap.named_entity_mut(camera_id)
            .remove::<ScreenCameraController>();
        heap.named_entity_mut(fallback_id)
            .insert::<ScreenCameraController>(ScreenCameraController);

        let camera_frame = heap.get::<WorldSpaceFrame>(camera_id).to_owned();
        // *heap.get_mut::<WorldSpaceFrame>(fallback_id) = camera_frame;
        heap.get_mut::<ArcBallController>(fallback_id)
            .set_target(camera_frame.position_graticule())
    }

    #[method]
    fn attach_camera(&self, mut heap: HeapMut) {
        let fallback_id = heap.entity_by_name("fallback_camera");
        let camera_id = heap.entity_by_name("camera");

        heap.named_entity_mut(fallback_id)
            .remove::<ScreenCameraController>();
        heap.named_entity_mut(camera_id)
            .insert::<ScreenCameraController>(ScreenCameraController);
    }

    #[method]
    fn take_control(&self, name: &str, mut heap: HeapMut) {
        let player_id = heap.entity_by_name("Player");
        if let Some(target_id) = heap.maybe_entity_by_name(name) {
            heap.named_entity_mut(player_id)
                .remove::<PlayerMarker>()
                .rename_numbered("Player_prior_");
            heap.named_entity_mut(target_id)
                .insert(PlayerMarker)
                .rename("Player");
        }
    }

    fn frame_for_interactive(heap: &mut HeapMut) -> WorldSpaceFrame {
        let fallback_id = heap.entity_by_name("fallback_camera");
        let camera_id = heap.entity_by_name("camera");

        let target = if heap
            .maybe_get::<ScreenCameraController>(fallback_id)
            .is_some()
        {
            heap.get_named::<ArcBallController>("fallback_camera")
                .target()
        } else if heap
            .maybe_get::<ScreenCameraController>(camera_id)
            .is_some()
        {
            let mut grat = heap
                .get_named::<WorldSpaceFrame>("camera")
                .position_graticule();
            grat.latitude += degrees!(1_f64 / 60_f64);
            grat
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

        // Fake up an info.
        let info = ObjectInfo::from_xt(xt.clone(), name);

        // Preload the one shape
        heap.resource_scope(|heap, mut shapes: Mut<ShapeBuffer>| {
            let libs = heap.resource::<Libs>();
            shapes.upload_shapes(
                libs.palette(),
                &[&xt.ot().shape.as_ref().ok_or_else(|| anyhow!("no shape"))?],
                libs.catalog(),
                heap.resource::<Gpu>(),
            )
        })?;

        // Use the spawn routing shared by mission loading
        let id = Self::spawn_entity_common(name, &info, None, heap.as_mut())?;
        if name == "Player" {
            heap.named_entity_mut(id).insert(PlayerMarker);
        }

        Ok(Value::True())
    }

    /// Spawn with a temp name, then game.take_control of the new entity.
    #[method]
    fn spawn_in(&self, filename: &str, mut heap: HeapMut) -> Result<Value> {
        let name: String = thread_rng()
            .sample_iter(&Alphanumeric)
            .take(6)
            .map(char::from)
            .collect();
        self.spawn(&name, filename, heap.as_mut())?;
        self.take_control(&name, heap);
        Ok(Value::True())
    }

    fn spawn_entity_common<S: AsRef<str>>(
        inst_name: S,
        info: &ObjectInfo,
        tile_id: Option<Entity>,
        mut heap: HeapMut,
    ) -> Result<Entity> {
        let id = heap
            .spawn_named(inst_name.as_ref())?
            .insert(MissionMarker)
            .id();

        // Load the shape if it has one.
        let metadata = if let Some(shape_file_name) = info.xt().ot().shape.as_ref() {
            let normal_shape_id = heap
                .resource::<ShapeBuffer>()
                .shape_ids_for_preloaded_shape(shape_file_name)?
                .normal();
            heap.resource_scope(|mut heap, mut shapes: Mut<ShapeBuffer>| {
                heap.resource_scope(|mut heap, gpu: Mut<Gpu>| {
                    let entity = heap.named_entity_mut(id);
                    shapes.instantiate(entity, normal_shape_id, &gpu)
                })
            })?;
            heap.resource::<ShapeBuffer>().metadata(normal_shape_id)
        } else {
            Arc::new(RwLock::new(ShapeMetadata::non_shape()))
        };

        // FIXME: seriously, figure out what's going on with scale in SH files
        let scale: Option<&i32> = SCALE_OVERRIDE.get(<String as Borrow<str>>::borrow(
            &info.xt().ot().ot_names.file_name,
        ));
        let scale = *scale.unwrap_or(&1i32) as f32;
        let scale = scalar!(scale * FEET_TO_METERS); // Internal units are meters

        let frame = if let Some(tile_id) = tile_id {
            let tile_mapper = heap.get::<T2TileSet>(tile_id).mapper();
            let offset_from_ground =
                (metadata.read().extent().offset_to_ground() + feet!(info.position().y)) * scale;
            // FIXME: figure out the terrain height here and raise to at least that level
            let position = tile_mapper.fa2grat(info.position(), feet!(offset_from_ground));
            // FIXME: manually re-align strip halfs
            WorldSpaceFrame::from_graticule(position, info.angle().facing())
        } else {
            Self::frame_for_interactive(&mut heap)
        };

        let facing = *frame.facing();
        heap.named_entity_mut(id)
            .insert_named(frame)?
            .insert_named(ShapeScale::new(scale.into_inner()))?
            .insert(info.xt());

        if info.xt().is_jt() || info.xt().is_nt() || info.xt().is_pt() {
            heap.named_entity_mut(id)
                .insert_named(BodyMotion::new_forward(feet_per_second!(info.speed())))?;

            // TODO: use audio effect time for timing the animation and effector deployment times
            // heap.named_entity_mut(id)
            // TODO: fuel overrides
            // if let Some(fuel_override) = info.fuel_override() {
            //     heap.get_mut::<VehicleState>(id)
            //         .set_internal_fuel_lbs(fuel_override);
            // }
            // TODO: hardpoint overrides
        }

        // If the type is a plane, install a flight model
        if let Some(pt) = info.xt().pt() {
            let on_ground = info.position().y == 0;

            let fuel = FuelSystem::default().with_internal_tank(FuelTank::new(
                FuelTankKind::Center,
                kilograms!(pt.internal_fuel),
            ))?;

            let power = PowerSystem::default().with_engine(Turbojet::new_min_power(info.xt())?);

            heap.named_entity_mut(id)
                .insert_named(Airframe::new(kilograms!(info.xt().ot().empty_weight)))?
                .insert_named(fuel)?
                .insert_named(power)?
                .insert_named(PitchInceptor::default())?
                .insert_named(RollInceptor::default())?
                .insert_named(YawInceptor::default())?
                .insert_named(ThrottleInceptor::new_min_power())?
                .insert_named(AirbrakeControl::default())?
                .insert_named(AirbrakeEffector::new(0., Duration::from_millis(1)))?
                .insert_named(BayControl::default())?
                .insert_named(BayEffector::new(0., Duration::from_secs(2)))?
                .insert_named(FlapsControl::default())?
                .insert_named(FlapsEffector::new(0., Duration::from_millis(1)))?
                .insert_named(GearControl::new(on_ground))?
                .insert_named(GearEffector::new(
                    if on_ground { 1. } else { 0. },
                    Duration::from_secs(4),
                ))?
                .insert_named(HookControl::default())?
                .insert_named(HookEffector::new(0., Duration::from_millis(1)))?
                .insert_named(ClassicFlightModel::new(id, facing))?;
        }

        if let Some(name) = info.name() {
            if name == "Player" {
                heap.named_entity_mut(id)
                    .insert_named(EntityMarkers::default())?;
            }
        }

        Ok(id)
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
                mission_map,
                libs.catalog(),
                heap.resource::<Gpu>(),
            )
        })?;
        let tile_id = heap
            .spawn_named(make_symbol(map_name))?
            .insert_named(tile_set)?
            .insert(MissionMarker)
            .id();

        // Pre-load the shapes into as few chunks as possible.
        let mut shape_names = HashSet::new();
        for info in objects_pass_1 {
            if let Some(shape_file) = info.xt().ot().shape.as_ref() {
                shape_names.insert(shape_file.to_owned());
            }
        }
        let shape_names = shape_names.iter().collect::<Vec<_>>();
        heap.resource_scope(|heap, mut shapes: Mut<ShapeBuffer>| {
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
                    make_symbol(if !shape_name.is_empty() {
                        &shape_name[0..shape_name.len() - 3]
                    } else {
                        "none"
                    }),
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

            let id = Self::spawn_entity_common(&inst_name, info, Some(tile_id), heap.as_mut())?;
            if inst_name == "Player" {
                heap.named_entity_mut(id).insert(PlayerMarker);
            }
        }

        Ok(())
    }

    #[method]
    fn unload_mission(&self, mut heap: HeapMut) -> Result<()> {
        let mut work = Vec::new();
        for (entity, _) in heap.query::<(Entity, &MissionMarker)>().iter(heap.world()) {
            work.push(entity);
        }
        for entity in work.drain(..) {
            if let Some(slot_id) = heap.maybe_get::<SlotId>(entity) {
                let slot_id = *slot_id;
                heap.resource_mut::<ShapeBuffer>().free_slot(slot_id);
            }
            heap.despawn(entity);
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
        // let game_name = heap.resource::<Libs>().catalog().label().to_owned();

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
        // Unload prior mission
        self.unload_mission(heap.as_mut())?;

        // Reattach camera in case it has gone wandering
        self.attach_camera(heap.as_mut());

        let name = name.to_uppercase();
        // if name.starts_with('~') || name.starts_with('$') {
        //     // FIXME: log message to terminal
        //     bail!("cannot load {name}; it is a template (note the ~ or $ prefix)");
        // }
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

#[cfg(test)]
mod tests {
    use super::*;
    use atmosphere::AtmosphereBuffer;
    use camera::CameraSystem;
    use global_data::GlobalParametersBuffer;
    use player::PlayerCameraController;
    use terrain::TerrainBuffer;

    #[test]
    fn it_works() -> Result<()> {
        let mut runtime = Gpu::for_test()?;
        runtime
            .load_extension::<AssetLoader>()?
            .load_extension::<Libs>()?
            .load_extension::<TypeManager>()?
            .load_extension::<GlobalParametersBuffer>()?
            .load_extension::<AtmosphereBuffer>()?
            .load_extension::<TerrainBuffer>()?
            .load_extension::<T2TerrainBuffer>()?
            .load_extension::<ShapeBuffer>()?
            .load_extension::<CameraSystem>()?
            .load_extension::<PlayerCameraController>()?;

        let _fallback_camera_ent = runtime
            .spawn_named("fallback_camera")?
            .insert_named(WorldSpaceFrame::default())?
            .insert_named(ArcBallController::default())?
            .id();

        runtime.resource_scope(|heap, assets: Mut<AssetLoader>| {
            assets.load_mission("UKR01.M", heap)
        })?;

        Ok(())
    }
}
