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
use absolute_unit::{degrees, meters};
use anyhow::Result;
use atmosphere::AtmosphereBuffer;
use camera::{ArcBallCamera, Camera};
use catalog::DirectoryDrawer;
use chrono::{TimeZone, Utc};
use composite::CompositeRenderPass;
use fullscreen::FullscreenBuffer;
use geodesy::{GeoSurface, Graticule, Target};
use global_data::GlobalParametersBuffer;
use gpu::{make_frame_graph, Gpu};
use input::{InputController, InputSystem};
use legion::world::World;
use lib::{from_dos_string, CatalogBuilder};
use mm::MissionMap;
use nalgebra::convert;
use nitrous::{Interpreter, Value};
use nitrous_injector::{inject_nitrous_module, method, NitrousModule};
use orrery::Orrery;
use pal::Palette;
use parking_lot::RwLock;
use stars::StarsBuffer;
use std::{path::PathBuf, sync::Arc, time::Instant};
use structopt::StructOpt;
use t2::Terrain as T2Terrain;
use t2_tile_set::T2HeightTileSet;
use terrain::{CpuDetailLevel, GpuDetailLevel, TerrainBuffer, TileSet};
use tokio::{runtime::Runtime, sync::RwLock as AsyncRwLock};
use ui::UiRenderPass;
use widget::{Color, Label, PositionH, PositionV, Terminal, WidgetBuffer};
use winit::window::Window;
use world::WorldRenderPass;
use xt::TypeManager;

/// Show the contents of an MM file
#[derive(Debug, StructOpt)]
struct Opt {
    /// Extra directories to treat as libraries
    #[structopt(short, long)]
    libdir: Vec<PathBuf>,

    /// Regenerate instead of loading cached items on startup
    #[structopt(long = "no-cache")]
    no_cache: bool,

    /// The map file to view
    #[structopt(name = "NAME", last = true)]
    map_names: Vec<String>,
}

#[derive(Debug, NitrousModule)]
struct OFA {
    exit: bool,
    pin_camera: bool,
    camera: Camera,
}

#[inject_nitrous_module]
impl OFA {
    pub fn new(interpreter: &mut Interpreter) -> Arc<RwLock<Self>> {
        let ofa = Arc::new(RwLock::new(Self {
            exit: false,
            pin_camera: false,
            camera: Default::default(),
        }));
        interpreter.put_global("system", Value::Module(ofa.clone()));
        ofa
    }

    pub fn add_default_bindings(&mut self, interpreter: &mut Interpreter) -> Result<()> {
        interpreter.interpret_once(
            r#"
                let bindings := mapper.create_bindings("system");
                bindings.bind("quit", "system.exit()");
                bindings.bind("Escape", "system.exit()");
                bindings.bind("q", "system.exit()");
                bindings.bind("p", "system.toggle_pin_camera(pressed)");
                bindings.bind("l", "widget.dump_glyphs(pressed)");
            "#,
        )?;
        Ok(())
    }

    #[method]
    pub fn exit(&mut self) {
        self.exit = true;
    }

    #[method]
    pub fn toggle_pin_camera(&mut self, pressed: bool) {
        if pressed {
            self.pin_camera = !self.pin_camera;
        }
    }

    pub fn get_camera(&mut self, camera: &Camera) -> &Camera {
        if !self.pin_camera {
            self.camera = camera.to_owned();
        }
        &self.camera
    }
}

make_frame_graph!(
    FrameGraph {
        buffers: {
            atmosphere: AtmosphereBuffer,
            fullscreen: FullscreenBuffer,
            globals: GlobalParametersBuffer,
            stars: StarsBuffer,
            terrain: TerrainBuffer,
            widgets: WidgetBuffer,
            world: WorldRenderPass,
            ui: UiRenderPass,
            composite: CompositeRenderPass
        };
        passes: [
            // terrain
            // Update the indices so we have correct height data to tessellate with and normal
            // and color data to accumulate.
            paint_atlas_indices: Any() { terrain() },
            // Apply heights to the terrain mesh.
            tessellate: Compute() { terrain() },
            // Render the terrain mesh's texcoords to an offscreen buffer.
            deferred_texture: Render(terrain, deferred_texture_target) {
                terrain( globals )
            },
            // Accumulate normal and color data.
            accumulate_normal_and_color: Compute() { terrain( globals ) },

            // world: Flatten terrain g-buffer into the final image and mix in stars.
            render_world: Render(world, offscreen_target) {
                world( globals, fullscreen, atmosphere, stars, terrain )
            },

            // ui: Draw our widgets onto a buffer with resolution independent of the world.
            render_ui: Render(ui, offscreen_target) {
                ui( globals, widgets, world )
            },

            // composite: Accumulate offscreen buffers into a final image.
            composite_scene: Render(Screen) {
                composite( fullscreen, globals, world, ui )
            }
        ];
    }
);

fn main() -> Result<()> {
    env_logger::init();
    InputSystem::run_forever(window_main)
}

fn window_main(window: Window, input_controller: &InputController) -> Result<()> {
    let opt = Opt::from_args();
    let (cpu_detail, gpu_detail) = if cfg!(debug_assertions) {
        (CpuDetailLevel::Low, GpuDetailLevel::Low)
    } else {
        (CpuDetailLevel::Medium, GpuDetailLevel::High)
    };

    let mut async_rt = Runtime::new()?;
    let mut _legion = World::default();

    let (mut catalog, input_fids) = CatalogBuilder::build_and_select(&opt.map_names)?;
    for (i, d) in opt.libdir.iter().enumerate() {
        catalog.add_labeled_drawer(
            "default",
            DirectoryDrawer::from_directory(100 + i as i64, d)?,
        )?;
    }

    let interpreter = Interpreter::new();
    let gpu = Gpu::new(&window, Default::default(), &mut interpreter.write())?;

    let orrery = Orrery::new(
        Utc.ymd(1964, 8, 24).and_hms(0, 0, 0),
        &mut interpreter.write(),
    );
    let arcball = ArcBallCamera::new(meters!(0.5), &mut gpu.write(), &mut interpreter.write());

    ///////////////////////////////////////////////////////////
    let atmosphere_buffer = Arc::new(RwLock::new(AtmosphereBuffer::new(
        opt.no_cache,
        &mut gpu.write(),
    )?));
    let fullscreen_buffer = FullscreenBuffer::new(&gpu.read());
    let globals = GlobalParametersBuffer::new(gpu.read().device(), &mut interpreter.write());
    let stars_buffer = Arc::new(RwLock::new(StarsBuffer::new(&gpu.read())?));
    let terrain_buffer = TerrainBuffer::new(
        &catalog,
        cpu_detail,
        gpu_detail,
        &globals.read(),
        &mut gpu.write(),
        &mut interpreter.write(),
    )?;
    let widgets = WidgetBuffer::new(&mut gpu.write(), &mut interpreter.write())?;
    let world = WorldRenderPass::new(
        &mut gpu.write(),
        &mut interpreter.write(),
        &globals.read(),
        &atmosphere_buffer.read(),
        &stars_buffer.read(),
        &terrain_buffer.read(),
    )?;
    let ui = UiRenderPass::new(
        &mut gpu.write(),
        &globals.read(),
        &widgets.read(),
        &world.read(),
    )?;
    let composite = Arc::new(RwLock::new(CompositeRenderPass::new(
        &mut gpu.write(),
        &globals.read(),
        &world.read(),
        &ui.read(),
    )?));

    let mut frame_graph = FrameGraph::new(
        atmosphere_buffer,
        fullscreen_buffer,
        globals.clone(),
        stars_buffer,
        terrain_buffer.clone(),
        widgets.clone(),
        world.clone(),
        ui,
        composite,
    )?;

    ///////////////////////////////////////////////////////////
    // UI Setup
    let version_label = Label::new("OpenFA v0.1")
        .with_color(Color::Green)
        .with_size(8.0)
        .with_pre_blended_text()
        .wrapped();
    widgets
        .read()
        .root()
        .write()
        .add_child("version", version_label)
        .set_float(PositionH::End, PositionV::Top);

    let fps_label = Label::new("fps")
        .with_color(Color::Red)
        .with_size(13.0)
        .with_pre_blended_text()
        .wrapped();
    widgets
        .read()
        .root()
        .write()
        .add_child("fps", fps_label.clone())
        .set_float(PositionH::Start, PositionV::Bottom);

    let terminal = Terminal::new(frame_graph.widgets.read().font_context())
        .with_visible(false)
        .wrapped();
    widgets
        .read()
        .root()
        .write()
        .add_child("terminal", terminal)
        .set_float(PositionH::Start, PositionV::Top);

    ///////////////////////////////////////////////////////////
    // Scene Setup
    let mut tracker = Default::default();
    let mut t2_tile_set =
        T2HeightTileSet::new(&terrain_buffer.read(), &globals.read(), &gpu.read())?;
    let start = Instant::now();
    let type_manager = TypeManager::empty();
    for mm_fid in &input_fids {
        let system_palette = Palette::from_bytes(&catalog.read_name_sync("PALETTE.PAL")?)?;
        let raw = catalog.read_sync(*mm_fid)?;
        let mm_content = from_dos_string(raw);
        let mm = MissionMap::from_str(&mm_content, &type_manager, &catalog)?;
        let t2_data = catalog.read_name_sync(mm.t2_name())?;
        let t2 = T2Terrain::from_bytes(&t2_data)?;
        t2_tile_set.add_map(
            &system_palette,
            &mm,
            &t2,
            &catalog,
            &mut gpu.write(),
            &async_rt,
            &mut tracker,
        )?;
    }
    terrain_buffer
        .write()
        .add_tile_set(Box::new(t2_tile_set) as Box<dyn TileSet>);
    println!("Loading scene took: {:?}", start.elapsed());

    /*
    let mut camera = UfoCamera::new(gpu.read().aspect_ratio(), 0.1f64, 3.4e+38f64);
    camera.set_position(6_378.0, 0.0, 0.0);
    camera.set_rotation(&Vector3::new(0.0, 0.0, 1.0), PI / 2.0);
    camera.apply_rotation(&Vector3::new(0.0, 1.0, 0.0), PI);
    */

    // London: 51.5,-0.1
    arcball.write().set_target(Graticule::<GeoSurface>::new(
        degrees!(51.5),
        degrees!(-0.1),
        meters!(8000.),
    ));
    arcball.write().set_eye_relative(Graticule::<Target>::new(
        degrees!(11.5),
        degrees!(869.5),
        meters!(67668.5053),
    ))?;
    // everest: 27.9880704,86.9245623
    // arcball.write().set_target(Graticule::<GeoSurface>::new(
    //     degrees!(27.9880704),
    //     degrees!(-86.9245623), // FIXME: wat?
    //     meters!(8000.),
    // ));
    // arcball.write().set_eye_relative(Graticule::<Target>::new(
    //     degrees!(11.5),
    //     degrees!(869.5),
    //     meters!(67668.5053),
    // ))?;
    // ISS: 408km up
    // arcball.write().set_target(Graticule::<GeoSurface>::new(
    //     degrees!(27.9880704),
    //     degrees!(-86.9245623), // FIXME: wat?
    //     meters!(408_000.),
    // ));
    // arcball.write().set_eye_relative(Graticule::<Target>::new(
    //     degrees!(58),
    //     degrees!(668.0),
    //     meters!(1308.7262),
    // ))?;

    let ofa = OFA::new(&mut interpreter.write());

    {
        let interp = &mut interpreter.write();
        gpu.write().add_default_bindings(interp)?;
        orrery.write().add_default_bindings(interp)?;
        arcball.write().add_default_bindings(interp)?;
        globals.write().add_default_bindings(interp)?;
        world.write().add_default_bindings(interp)?;
        ofa.write().add_default_bindings(interp)?;
    }

    let catalog = Arc::new(AsyncRwLock::new(catalog));

    while !ofa.read().exit {
        let loop_start = Instant::now();

        widgets
            .write()
            .handle_events(&input_controller.poll_events()?, interpreter.clone())?;

        arcball.write().think();

        let mut tracker = Default::default();
        frame_graph.globals().make_upload_buffer(
            arcball.read().camera(),
            &gpu.read(),
            &mut tracker,
        )?;
        frame_graph.atmosphere().make_upload_buffer(
            convert(orrery.read().sun_direction()),
            &gpu.read(),
            &mut tracker,
        )?;
        frame_graph.terrain_mut().make_upload_buffer(
            arcball.read().camera(),
            ofa.write().get_camera(arcball.read().camera()),
            catalog.clone(),
            &mut async_rt,
            &mut gpu.write(),
            &mut tracker,
        )?;
        frame_graph.widgets.write().make_upload_buffer(
            &mut gpu.write(),
            &async_rt,
            &mut tracker,
        )?;
        if !frame_graph.run(&mut gpu.write(), tracker)? {
            let sz = gpu.read().physical_size();
            gpu.write().on_resize(sz.width as i64, sz.height as i64)?;
        }

        let frame_time = loop_start.elapsed();
        let ts = format!(
            "eye_rel: {} | tgt: {} | asl: {}, fov: {} || Date: {:?} || frame: {}.{}ms",
            arcball.read().get_eye_relative(),
            arcball.read().get_target(),
            arcball.read().get_target().distance,
            degrees!(arcball.read().camera().fov_y()),
            orrery.read().get_time(),
            frame_time.as_secs() * 1000 + u64::from(frame_time.subsec_millis()),
            frame_time.subsec_micros(),
        );
        fps_label.write().set_text(ts);
    }

    Ok(())
}
