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
use absolute_unit::{degrees, meters, radians};
use animate::Timeline;
use anyhow::{bail, Result};
use atmosphere::AtmosphereBuffer;
use camera::{ArcBallCamera, Camera};
use catalog::Catalog;
use chrono::{Duration as ChronoDuration, TimeZone, Utc};
use composite::CompositeRenderPass;
use fnt::Fnt;
use font_fnt::FntFont;
use fullscreen::FullscreenBuffer;
use galaxy::Galaxy;
use geodesy::{GeoSurface, Graticule};
use global_data::GlobalParametersBuffer;
use gpu::{
    make_frame_graph,
    size::{AbsSize, LeftBound, Size},
    Gpu,
};
use input::{InputController, InputSystem};
use lib::{from_dos_string, CatalogManager, CatalogOpts};
use mmm::MissionMap;
use nalgebra::convert;
use nitrous::{Interpreter, Value};
use nitrous_injector::{inject_nitrous_module, method, NitrousModule};
use orrery::Orrery;
use pal::Palette;
use parking_lot::RwLock;
use shape_instance::{DrawSelection, ShapeInstanceBuffer};
use stars::StarsBuffer;
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use structopt::StructOpt;
use t2_tile_set::{T2Adjustment, T2TileSet};
use terrain::{CpuDetailLevel, GpuDetailLevel, TerrainBuffer, TileSet};
use tokio::{runtime::Runtime, sync::RwLock as AsyncRwLock};
use ui::UiRenderPass;
use widget::{
    Border, Color, Expander, Extent, Label, Labeled, PositionH, PositionV, VerticalBox,
    WidgetBuffer,
};
use winit::window::Window;
use world::WorldRenderPass;
use xt::TypeManager;

/// Show the contents of an MM file
#[derive(Debug, StructOpt)]
struct Opt {
    /// Run a command after startup
    #[structopt(short, long)]
    run_command: Option<String>,

    /// Run given file after startup
    #[structopt(short = "x", long)]
    execute: Option<PathBuf>,

    /// The map file(s) to view
    #[structopt(short, long, name = "NAME")]
    map_names: Vec<String>,

    #[structopt(flatten)]
    catalog_opts: CatalogOpts,
}

#[derive(Debug)]
struct VisibleWidgets {
    demo_label: Arc<RwLock<Label>>,
    sim_time: Arc<RwLock<Label>>,
    camera_direction: Arc<RwLock<Label>>,
    camera_position: Arc<RwLock<Label>>,
    camera_fov: Arc<RwLock<Label>>,
    fps_label: Arc<RwLock<Label>>,
}

#[derive(Debug, NitrousModule)]
struct System {
    exit: bool,
    pin_camera: bool,
    visibility_camera: Camera,
    maybe_update_view: Option<Graticule<GeoSurface>>,
    adjust: Arc<RwLock<T2Adjustment>>,
    target_offset: isize,
    targets: Vec<(String, Graticule<GeoSurface>)>,
    interpreter: Interpreter,
    widgets: Arc<RwLock<WidgetBuffer>>,
    visible_widgets: VisibleWidgets,
}

#[inject_nitrous_module]
impl System {
    pub fn new(
        catalog: &Catalog,
        interpreter: Interpreter,
        widgets: Arc<RwLock<WidgetBuffer>>,
    ) -> Result<Arc<RwLock<Self>>> {
        let visible_widgets = Self::build_gui(catalog, widgets.clone())?;
        let system = Arc::new(RwLock::new(Self {
            exit: false,
            pin_camera: false,
            maybe_update_view: None,
            visibility_camera: Default::default(),
            adjust: Arc::new(RwLock::new(T2Adjustment::default())),
            target_offset: 0,
            targets: Vec::new(),
            interpreter,
            widgets,
            visible_widgets,
        }));
        let demo = Value::Module(system.read().visible_widgets.demo_label.clone());
        system.write().interpreter.put_global("demo", demo);
        system
            .write()
            .interpreter
            .put_global("system", Value::Module(system.clone()));
        Ok(system)
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
                bindings.bind("d", "system.replay_demo(pressed)");

                bindings.bind("j", "system.terrain_adjust_lon_base(pressed, -1.0)");
                bindings.bind("l", "system.terrain_adjust_lon_base(pressed, 1.0)");
                bindings.bind("i", "system.terrain_adjust_lat_base(pressed, 1.0)");
                bindings.bind("k", "system.terrain_adjust_lat_base(pressed, -1.0)");
                bindings.bind("shift+j", "system.terrain_adjust_lon_base(pressed, -0.1)");
                bindings.bind("shift+l", "system.terrain_adjust_lon_base(pressed, 0.1)");
                bindings.bind("shift+i", "system.terrain_adjust_lat_base(pressed, 0.1)");
                bindings.bind("shift+k", "system.terrain_adjust_lat_base(pressed, -0.1)");
                bindings.bind("control+j", "system.terrain_adjust_lon_base(pressed, -0.01)");
                bindings.bind("control+l", "system.terrain_adjust_lon_base(pressed, 0.01)");
                bindings.bind("control+i", "system.terrain_adjust_lat_base(pressed, 0.01)");
                bindings.bind("control+k", "system.terrain_adjust_lat_base(pressed, -0.01)");

                bindings.bind("n", "system.next_target(pressed)");
                bindings.bind("shift+n", "system.previous_target(pressed)");

                bindings.bind("o", "system.terrain_adjust_lon_scale(pressed, 1000.0)");
                bindings.bind("u", "system.terrain_adjust_lon_scale(pressed, -1000.0)");

                bindings.bind("f", "system.terrain_adjust_toggle_hide(pressed)");
            "#,
        )?;
        Ok(())
    }

    pub fn build_gui(
        catalog: &Catalog,
        widgets: Arc<RwLock<WidgetBuffer>>,
    ) -> Result<VisibleWidgets> {
        let fnt = Fnt::from_bytes(&catalog.read_name_sync("HUD11.FNT")?)?;
        let font = FntFont::from_fnt(&fnt)?;
        widgets.write().add_font("HUD11", font);

        let sim_time = Label::new("").with_color(Color::White).wrapped();
        let camera_direction = Label::new("").with_color(Color::White).wrapped();
        let camera_position = Label::new("").with_color(Color::White).wrapped();
        let camera_fov = Label::new("").with_color(Color::White).wrapped();
        let controls_box = VerticalBox::new_with_children(&[
            sim_time.clone(),
            camera_direction.clone(),
            camera_position.clone(),
            camera_fov.clone(),
        ])
        .with_background_color(Color::Gray.darken(3.).opacity(0.8))
        .with_glass_background()
        .with_padding(Border::new(
            Size::zero(),
            Size::from_px(8.),
            Size::from_px(24.),
            Size::from_px(8.),
        ))
        .wrapped();
        let expander = Expander::new_with_child("☰ OpenFA v0.0", controls_box)
            .with_color(Color::White)
            .with_background_color(Color::Gray.darken(3.).opacity(0.8))
            .with_glass_background()
            .with_border(
                Color::Black,
                Border::new(
                    Size::zero(),
                    Size::from_px(2.),
                    Size::from_px(2.),
                    Size::zero(),
                ),
            )
            .with_padding(Border::new(
                Size::from_px(2.),
                Size::from_px(3.),
                Size::from_px(3.),
                Size::from_px(2.),
            ))
            .wrapped();
        widgets
            .read()
            .root_container()
            .write()
            .add_child("controls", expander)
            .set_float(PositionH::End, PositionV::Top);

        let fps_label = Label::new("")
            .with_font(widgets.read().font_context().font_id_for_name("sans"))
            .with_color(Color::Red)
            .with_size(Size::from_pts(13.0))
            .with_pre_blended_text()
            .wrapped();
        widgets
            .read()
            .root_container()
            .write()
            .add_child("fps", fps_label.clone())
            .set_float(PositionH::Start, PositionV::Bottom);

        let demo_label = Label::new("")
            .with_font(widgets.read().font_context().font_id_for_name("HUD11"))
            .with_color(Color::White)
            .with_size(Size::from_pts(18.0))
            .wrapped();
        let demo_box = VerticalBox::new_with_children(&[demo_label.clone()])
            .with_background_color(Color::Gray.darken(3.).opacity(0.8))
            .with_glass_background()
            .with_border(Color::Black, Border::new_uniform(Size::from_px(2.)))
            .with_padding(Border::new_uniform(Size::from_px(8.)))
            .wrapped();
        widgets
            .read()
            .root_container()
            .write()
            .add_child("demo", demo_box)
            .set_float(PositionH::Start, PositionV::Bottom);
        widgets
            .read()
            .root_container()
            .write()
            .packing_mut("demo")?
            .set_expand(false);

        Ok(VisibleWidgets {
            demo_label,
            sim_time,
            camera_direction,
            camera_position,
            camera_fov,
            fps_label,
        })
    }

    pub fn track_visible_state(
        &mut self,
        frame_time: Duration,
        orrery: &Orrery,
        arcball: &mut ArcBallCamera,
    ) {
        if let Some(grat) = self.maybe_update_view {
            arcball.set_target(grat);
        }
        self.maybe_update_view = None;
        self.visible_widgets
            .sim_time
            .write()
            .set_text(format!("Date: {}", orrery.get_time()));
        self.visible_widgets
            .camera_direction
            .write()
            .set_text(format!("Eye: {}", arcball.eye()));
        self.visible_widgets
            .camera_position
            .write()
            .set_text(format!("Position: {}", arcball.target(),));
        self.visible_widgets
            .camera_fov
            .write()
            .set_text(format!("FoV: {}", degrees!(arcball.camera().fov_y()),));
        self.visible_widgets
            .fps_label
            .write()
            .set_text(format!("fps: {:0.2}", 1. / frame_time.as_secs_f64()));
    }

    pub fn t2_adjustment(&self) -> Arc<RwLock<T2Adjustment>> {
        self.adjust.clone()
    }

    pub fn add_target(&mut self, name: &str, grat: Graticule<GeoSurface>) {
        self.targets.push((name.to_owned(), grat));
    }

    #[method]
    pub fn terrain_adjust_lon_scale(&self, pressed: bool, f: f64) {
        if pressed {
            self.adjust.write().span_offset[1] += meters!(f);
            println!(
                "span offset: {}x{}",
                self.adjust.read().span_offset[0],
                self.adjust.read().span_offset[1]
            );
        }
    }

    #[method]
    pub fn terrain_adjust_lon_base(&self, pressed: bool, f: f64) {
        if pressed {
            self.adjust.write().base_offset[1] += degrees!(f);
            println!(
                "base offset: {}x{}",
                self.adjust.read().base_offset[0],
                self.adjust.read().base_offset[1]
            );
        }
    }

    #[method]
    pub fn terrain_adjust_lat_base(&self, pressed: bool, f: f64) {
        if pressed {
            self.adjust.write().base_offset[0] += degrees!(f);
            println!(
                "base offset: {}x{}",
                self.adjust.read().base_offset[0],
                self.adjust.read().base_offset[1]
            );
        }
    }

    #[method]
    pub fn terrain_adjust_toggle_hide(&self, pressed: bool) {
        if pressed {
            if self.adjust.read().blend_factor < 1.0 {
                self.adjust.write().blend_factor = 1.0;
            } else {
                self.adjust.write().blend_factor = 0.2;
            }
        }
    }

    #[method]
    pub fn next_target(&mut self, pressed: bool) {
        if pressed {
            self.target_offset += 1;
            self.target_offset %= self.targets.len() as isize;

            let (name, pos) = &self.targets[self.target_offset as usize];
            self.maybe_update_view = Some(*pos);
            println!("target: {}, {}", self.target_offset, name);
        }
    }

    #[method]
    pub fn previous_target(&mut self, pressed: bool) {
        if pressed {
            self.target_offset -= 1;
            if self.target_offset < 0 {
                self.target_offset = self.targets.len() as isize - 1;
            }

            let (name, pos) = &self.targets[self.target_offset as usize];
            self.maybe_update_view = Some(*pos);
            println!("target: {}", name);
        }
    }

    #[method]
    pub fn exec_file(&mut self, exec_file: &str) {
        match std::fs::read_to_string(exec_file) {
            Ok(code) => {
                let rv = self.interpreter.interpret_async(code);
                println!("Execution Completed: {:?}", rv);
            }
            Err(e) => {
                println!("Unable to read file '{:?}': {}", exec_file, e);
            }
        }
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

    /// Maybe update visibility computation camera from the current view camera.
    pub fn get_camera(&mut self, view_camera: &Camera) -> &Camera {
        if !self.pin_camera {
            self.visibility_camera = view_camera.to_owned();
        }
        &self.visibility_camera
    }
}

make_frame_graph!(
    FrameGraph {
        buffers: {
            atmosphere: AtmosphereBuffer,
            fullscreen: FullscreenBuffer,
            globals: GlobalParametersBuffer,
            shapes: ShapeInstanceBuffer,
            stars: StarsBuffer,
            terrain: TerrainBuffer,
            widgets: WidgetBuffer,
            world: WorldRenderPass,
            ui: UiRenderPass,
            composite: CompositeRenderPass
        };
        passes: [
            // widget
            maintain_font_atlas: Any() { widgets() },

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
            render_world: Render(world, offscreen_target_cleared) {
                world( globals, fullscreen, atmosphere, stars, terrain )
            },

            // FIXME: can we get away with doing this before terrain so we don't overdraw?
            draw_shapes: Render(world, offscreen_target_preserved) {
                shapes( globals, atmosphere )
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
    let mut catalogs = CatalogManager::bootstrap(&opt.catalog_opts)?;
    let catalog = catalogs.best();
    let system_palette = Palette::from_bytes(&catalog.read_name_sync("PALETTE.PAL")?)?;

    let mut async_rt = Runtime::new()?;

    let mut interpreter = Interpreter::default();
    let timeline = Timeline::new(&mut interpreter);
    let gpu = Gpu::new(window, Default::default(), &mut interpreter)?;
    let mut galaxy = Galaxy::new()?;

    let orrery = Orrery::new(Utc.ymd(1964, 8, 24).and_hms(0, 0, 0), &mut interpreter);
    let arcball = ArcBallCamera::new(meters!(0.5), &mut gpu.write(), &mut interpreter);

    ///////////////////////////////////////////////////////////
    let atmosphere_buffer = AtmosphereBuffer::new(&mut gpu.write())?;
    let fullscreen_buffer = FullscreenBuffer::new(&gpu.read());
    let globals = GlobalParametersBuffer::new(gpu.read().device(), &mut interpreter);
    let stars_buffer = Arc::new(RwLock::new(StarsBuffer::new(&gpu.read())?));
    let terrain_buffer = TerrainBuffer::new(
        catalog,
        cpu_detail,
        gpu_detail,
        &globals.read(),
        &mut gpu.write(),
        &mut interpreter,
    )?;
    let shapes = ShapeInstanceBuffer::new(&globals.read(), &atmosphere_buffer.read(), &gpu.read())?;
    let world = WorldRenderPass::new(
        &mut gpu.write(),
        &mut interpreter,
        &globals.read(),
        &atmosphere_buffer.read(),
        &stars_buffer.read(),
        &terrain_buffer.read(),
    )?;
    let widgets = WidgetBuffer::new(&mut gpu.write(), &mut interpreter)?;
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
        shapes.clone(),
        stars_buffer,
        terrain_buffer.clone(),
        widgets.clone(),
        world.clone(),
        ui,
        composite,
    )?;

    let system = System::new(catalog, interpreter.clone(), widgets)?;

    ///////////////////////////////////////////////////////////
    // Scene Setup
    let start = Instant::now();
    shapes
        .write()
        .set_shared_palette(&system_palette, &gpu.read());
    let mut tracker = Default::default();
    let mut t2_tile_set = T2TileSet::new(
        system.read().t2_adjustment(),
        &terrain_buffer.read(),
        &globals.read(),
        &gpu.read(),
    )?;
    let type_manager = TypeManager::empty();
    for mm_fid in catalog.find_with_extension("MM")? {
        let name = catalog.stat_sync(mm_fid)?.name().to_owned();
        if name.starts_with('~') || name.starts_with('$') {
            continue;
        }
        println!("Loading {}...", name);
        let raw = catalog.read_sync(mm_fid)?;
        let mm_content = from_dos_string(raw);
        let mm = MissionMap::from_str(&mm_content, &type_manager, catalog)?;
        let t2_mapper = t2_tile_set.add_map(
            &system_palette,
            &mm,
            catalog,
            &mut gpu.write(),
            &async_rt,
            &mut tracker,
        )?;

        // shapes.write().ensure_uploaded(&mut gpu.write())?;

        for info in mm.objects() {
            if info.xt().ot().shape.is_none() {
                // FIXME: this still needs to add the entity.
                // I believe these are only for hidden flak guns in TVIET.
                continue;
            }

            let (shape_id, slot_id) = shapes.write().upload_and_allocate_slot(
                info.xt().ot().shape.as_ref().expect("a shape file"),
                DrawSelection::NormalModel,
                catalog,
                &mut gpu.write(),
                &async_rt,
                &mut tracker,
            )?;

            if let Ok(_pt) = info.xt().pt() {
                //galaxy.create_flyer(pt, shape_id, slot_id)?
                //unimplemented!()
            } else if let Ok(_nt) = info.xt().nt() {
                //galaxy.create_ground_mover(nt)
                //unimplemented!()
                let scale = if info
                    .xt()
                    .ot()
                    .shape
                    .as_ref()
                    .expect("a shape file")
                    .starts_with("BNK")
                {
                    2f32
                } else {
                    4f32
                };
                let grat = t2_mapper.fa2grat(
                    info.position(),
                    shapes
                        .read()
                        .part(shape_id)
                        .widgets()
                        .read()
                        .offset_to_ground()
                        * scale,
                );
                system
                    .write()
                    .add_target(&info.name().unwrap_or_else(|| "<unknown>".to_owned()), grat);
                galaxy.create_building(
                    slot_id,
                    shape_id,
                    shapes.read().part(shape_id),
                    scale,
                    grat,
                    info.angle(),
                )?;
            } else if info.xt().jt().is_ok() {
                bail!("did not expect a projectile in MM objects")
            } else {
                let scale = if info
                    .xt()
                    .ot()
                    .shape
                    .as_ref()
                    .expect("a shape file")
                    .starts_with("BNK")
                {
                    2f32
                } else {
                    4f32
                };
                let grat = t2_mapper.fa2grat(
                    info.position(),
                    shapes
                        .read()
                        .part(shape_id)
                        .widgets()
                        .read()
                        .offset_to_ground()
                        * scale,
                );
                system
                    .write()
                    .add_target(&info.name().unwrap_or_else(|| "<unknown>".to_owned()), grat);
                galaxy.create_building(
                    slot_id,
                    shape_id,
                    shapes.read().part(shape_id),
                    scale,
                    grat,
                    info.angle(),
                )?;
                /*
                   println!("Obj Inst {:?}: {:?}", info.xt().ot().shape, info.position());
                   let sh_name = info
                       .xt()
                       .ot()
                       .shape
                       .as_ref()
                       .expect("a shape file")
                       .to_owned();
                   if let Some(n) = info.name() {
                       names.push(n + " (" + &sh_name + ")");
                   } else {
                       names.push(sh_name);
                   }
                */
            };
        }
    }

    for (offset, (_game, catalog)) in catalogs.all().enumerate() {
        let system_palette = Palette::from_bytes(&catalog.read_name_sync("PALETTE.PAL")?)?;
        shapes
            .write()
            .set_shared_palette(&system_palette, &gpu.read());
        let mut pts = catalog.find_with_extension("PT")?;
        let side_len = (pts.len() as f64).sqrt().ceil() as usize;
        const KEY: &str = "AV8.PT";
        pts.sort_by(|a_fid, b_fid| {
            let a_stat = catalog.stat_sync(*a_fid).unwrap();
            let b_stat = catalog.stat_sync(*b_fid).unwrap();
            let a = a_stat.name();
            let b = b_stat.name();
            if a == KEY {
                std::cmp::Ordering::Less
            } else if b == KEY {
                std::cmp::Ordering::Greater
            } else {
                a.cmp(b)
            }
        });

        let base_lat = 0.16217;
        let base_lon = 1.379419;
        for (i, pt_fid) in pts.iter().enumerate() {
            let xi = i % side_len;
            let yi = i / side_len;
            let pt_stat = catalog.stat_sync(*pt_fid)?;
            let pt_name = pt_stat.name();
            let xt = type_manager.load(pt_name, catalog)?;
            let pt = xt.pt()?;
            let (shape_id, slot_id) = shapes.write().upload_and_allocate_slot(
                pt.nt.ot.shape.as_ref().unwrap(),
                DrawSelection::NormalModel,
                catalog,
                &mut gpu.write(),
                &async_rt,
                &mut tracker,
            )?;
            galaxy.create_building(
                slot_id,
                shape_id,
                shapes.read().part(shape_id),
                2.,
                Graticule::new(
                    degrees!(0.003 * xi as f64) + radians!(base_lat),
                    degrees!(0.003 * yi as f64) + radians!(base_lon),
                    meters!(1500.0 - 150.0 * offset as f64),
                ),
                &nalgebra::UnitQuaternion::identity(),
            )?;
        }
    }
    shapes
        .write()
        .ensure_uploaded(&mut gpu.write(), &async_rt, &mut tracker)?;
    tracker.dispatch_uploads_one_shot(&mut gpu.write());
    terrain_buffer
        .write()
        .add_tile_set(Box::new(t2_tile_set) as Box<dyn TileSet>);
    println!("Loading scene took: {:?}", start.elapsed());

    {
        let interp = &mut interpreter;
        gpu.write().add_default_bindings(interp)?;
        orrery.write().add_default_bindings(interp)?;
        arcball.write().add_default_bindings(interp)?;
        globals.write().add_default_bindings(interp)?;
        world.write().add_default_bindings(interp)?;
        system.write().add_default_bindings(interp)?;
    }

    let catalog = Arc::new(AsyncRwLock::new(catalogs.steal_best()));

    if let Some(command) = opt.run_command.as_ref() {
        let rv = interpreter.interpret_once(command)?;
        println!("{}", rv);
    }

    if let Ok(code) = std::fs::read_to_string("autoexec.n2o") {
        let rv = interpreter.interpret_once(&code);
        println!("Execution Completed: {:?}", rv);
    }

    if let Some(exec_file) = opt.execute {
        match std::fs::read_to_string(&exec_file) {
            Ok(code) => {
                let rv = interpreter.interpret_async(code);
                println!("Execution Completed: {:?}", rv);
            }
            Err(e) => {
                println!("Unable to read file '{:?}': {}", exec_file, e);
            }
        }
    }

    const STEP: Duration = Duration::from_micros(16_666);
    let mut now = Instant::now();
    let system_start = now;
    while !system.read().exit {
        // Catch up to system time.
        let next_now = Instant::now();
        while now + STEP < next_now {
            orrery.write().step_time(ChronoDuration::from_std(STEP)?);
            timeline.write().step_time(&now)?;
            now += STEP;
        }
        now = next_now;

        {
            let logical_extent: Extent<AbsSize> = gpu.read().logical_size().into();
            let scale_factor = { gpu.read().scale_factor() };
            frame_graph.widgets.write().handle_events(
                now,
                &input_controller.poll_events()?,
                interpreter.clone(),
                scale_factor,
                logical_extent,
            )?;
            frame_graph
                .widgets
                .write()
                .layout_for_frame(now, &mut gpu.write())?;
        }

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
            arcball.read_recursive().camera(),
            system.write().get_camera(arcball.read_recursive().camera()),
            catalog.clone(),
            &mut async_rt,
            &mut gpu.write(),
            &mut tracker,
        )?;
        frame_graph.shapes_mut().make_upload_buffer(
            &system_start,
            &now,
            arcball.read().camera(),
            galaxy.world_mut(),
            &gpu.read(),
            &mut tracker,
        )?;
        frame_graph.widgets.write().make_upload_buffer(
            now,
            &mut gpu.write(),
            &async_rt,
            &mut tracker,
        )?;
        if !frame_graph.run(&mut gpu.write(), tracker)? {
            let sz = gpu.read().physical_size();
            gpu.write().on_resize(sz.width as i64, sz.height as i64)?;
        }

        system
            .write()
            .track_visible_state(now.elapsed(), &orrery.read(), &mut arcball.write());
    }

    Ok(())
}
