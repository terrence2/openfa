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
use gpu::{Gpu, UploadTracker};
use lib::from_dos_string;
use log::warn;
use measure::WorldSpaceFrame;
use mmm::{Mission, MissionMap};
use nitrous::{inject_nitrous_resource, method, HeapMut, NitrousResource, Value};
use parking_lot::RwLock;
use runtime::{Extension, Runtime};
use shape_instance::{DrawSelection, ShapeInstanceBuffer};
use std::sync::Arc;
use t2_tile_set::{T2Adjustment, T2TileSet};
use terrain::{TerrainBuffer, TileSet};
use xt::TypeManager;

#[derive(Debug, Default, NitrousResource)]
pub struct Game {}

impl Extension for Game {
    fn init(runtime: &mut Runtime) -> Result<()> {
        /*
        let t2_adjustment = T2Adjustment::default();
        let mut t2_tile_set = T2TileSet::new(
            system.read().t2_adjustment(),
            &terrain.read(),
            &globals.read(),
            &gpu.read(),
        )?;
        runtime
            .resource_mut::<TerrainBuffer>()
            .add_tile_set(Box::new(t2_tile_set) as Box<dyn TileSet>);
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
        let catalog = heap.resource::<Arc<RwLock<Catalog>>>();
        let xt = heap
            .resource::<TypeManager>()
            .load(&pt_filename, &catalog.read())?;
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
        // can we print this in a useful way?
        println!("Loading {}...", name);

        let type_manager = heap.resource::<TypeManager>();
        let catalog = heap.resource::<Arc<RwLock<Catalog>>>();
        let cat = catalog.read();
        let raw = cat.read_name_sync(name)?;
        let mm_content = from_dos_string(raw);
        let mm = MissionMap::from_str(&mm_content, type_manager, &cat)?;

        // let mut t2_tile_set = T2TileSet::new(
        //     system.read().t2_adjustment(),
        //     &terrain.read(),
        //     &globals.read(),
        //     &gpu.read(),
        // )?;

        // let terrain = heap.resource::<TerrainBuffer>();

        Ok(())
    }
}
