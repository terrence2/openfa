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
mod game;

use crate::game::Game;

use absolute_unit::{degrees, meters, radians};
use animate::{TimeStep, Timeline};
use anyhow::{anyhow, bail, Result};
use atmosphere::AtmosphereBuffer;
use bevy_ecs::prelude::*;
use camera::{
    ArcBallController, ArcBallSystem, CameraSystem, ScreenCamera, ScreenCameraController,
};
use catalog::{Catalog, CatalogOpts};
use composite::CompositeRenderPass;
use event_mapper::EventMapper;
use fnt::Fnt;
use font_fnt::FntFont;
use fullscreen::FullscreenBuffer;
use geodesy::{GeoSurface, Graticule};
use global_data::GlobalParametersBuffer;
use gpu::{DetailLevelOpts, Gpu};
use input::{DemoFocus, InputSystem};
use lib::{from_dos_string, Libs, LibsOpts};
use log::warn;
use measure::WorldSpaceFrame;
use mmm::MissionMap;
use nitrous::{inject_nitrous_resource, method, HeapMut, NitrousResource};
use orrery::Orrery;
use parking_lot::RwLock;
use platform_dirs::AppDirs;
use runtime::{ExitRequest, Extension, FrameStage, Runtime, StartupOpts};
use shape::{DrawSelection, ShapeBuffer};
use stars::StarsBuffer;
use std::{f32::consts::PI, fs::create_dir_all, sync::Arc, time::Instant};
use structopt::StructOpt;
use t2_terrain::{T2Adjustment, T2TerrainBuffer};
use terminal_size::{terminal_size, Width};
use terrain::{TerrainBuffer, TileSet};
use ui::UiRenderPass;
use widget::{
    Border, Color, Expander, Label, Labeled, PositionH, PositionV, VerticalBox, WidgetBuffer,
};
use window::{
    size::{LeftBound, Size},
    DisplayOpts, Window, WindowBuilder,
};
use world::WorldRenderPass;
use xt::TypeManager;

/// Show resources from Jane's Fighters Anthology engine LIB files.
#[derive(Clone, Debug, StructOpt)]
#[structopt(set_term_width = if let Some((Width(w), _)) = terminal_size() { w as usize } else { 80 })]
struct Opt {
    #[structopt(flatten)]
    libs_opts: LibsOpts,

    #[structopt(flatten)]
    detail_opts: DetailLevelOpts,

    #[structopt(flatten)]
    display_opts: DisplayOpts,

    #[structopt(flatten)]
    startup_opts: StartupOpts,
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

#[derive(Debug, NitrousResource)]
struct System {
    maybe_update_view: Option<Graticule<GeoSurface>>,
    target_offset: isize,
    targets: Vec<(String, Graticule<GeoSurface>)>,
    visible_widgets: VisibleWidgets,
}

impl Extension for System {
    fn init(runtime: &mut Runtime) -> Result<()> {
        let system =
            runtime.resource_scope(|heap, mut widgets: Mut<WidgetBuffer<DemoFocus>>| {
                System::new(heap.resource::<Libs>(), &mut widgets)
            })?;
        runtime.insert_named_resource("system", system);
        runtime
            .frame_stage_mut(FrameStage::FrameEnd)
            .add_system(Self::sys_track_visible_state);
        runtime.run_string(
            r#"
                bindings.bind("Escape", "exit()");
                bindings.bind("q", "exit()");
            "#,
        )?;
        Ok(())
    }
}

#[inject_nitrous_resource]
impl System {
    pub fn new(libs: &Libs, widgets: &mut WidgetBuffer<DemoFocus>) -> Result<Self> {
        let visible_widgets = Self::build_gui(libs, widgets)?;
        let system = Self {
            maybe_update_view: None,
            target_offset: 0,
            targets: Vec::new(),
            visible_widgets,
        };
        Ok(system)
    }

    /*
    pub fn add_default_bindings(&mut self, interpreter: &mut Interpreter) -> Result<()> {
        interpreter.interpret_once(
            r#"
                bindings.bind("Escape", "exit()");
                bindings.bind("q", "exit()");

                // let bindings := mapper.create_bindings("system");
                bindings.bind("quit", "system.exit()");
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
     */

    pub fn build_gui(libs: &Libs, widgets: &mut WidgetBuffer<DemoFocus>) -> Result<VisibleWidgets> {
        let fnt = Fnt::from_bytes(libs.read_name("HUD11.FNT")?.as_ref())?;
        let font = FntFont::from_fnt(&fnt)?;
        widgets.add_font("HUD11", font);

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
            .root_container()
            .write()
            .add_child("controls", expander)
            .set_float(PositionH::End, PositionV::Top);

        let fps_label = Label::new("")
            .with_font(widgets.font_context().font_id_for_name("sans"))
            .with_color(Color::Red)
            .with_size(Size::from_pts(13.0))
            .with_pre_blended_text()
            .wrapped();
        widgets
            .root_container()
            .write()
            .add_child("fps", fps_label.clone())
            .set_float(PositionH::Start, PositionV::Bottom);

        let demo_label = Label::new("")
            .with_font(widgets.font_context().font_id_for_name("HUD11"))
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
            .root_container()
            .write()
            .add_child("demo", demo_box)
            .set_float(PositionH::Start, PositionV::Bottom);
        widgets
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

    fn sys_track_visible_state(
        query: Query<&ArcBallController>,
        camera: Res<ScreenCamera>,
        timestep: Res<TimeStep>,
        orrery: Res<Orrery>,
        mut system: ResMut<System>,
    ) {
        for arcball in query.iter() {
            system.track_visible_state(*timestep.now(), &orrery, arcball, &camera);
        }
    }

    pub fn track_visible_state(
        &mut self,
        now: Instant,
        orrery: &Orrery,
        arcball: &ArcBallController,
        camera: &ScreenCamera,
    ) {
        // if let Some(grat) = self.maybe_update_view {
        //     arcball.set_target(grat);
        // }
        // self.maybe_update_view = None;
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
            .set_text(format!("FoV: {}", degrees!(camera.fov_y()),));
        let frame_time = now.elapsed();
        let ts = format!(
            "frame: {}.{}ms",
            frame_time.as_secs() * 1000 + u64::from(frame_time.subsec_millis()),
            frame_time.subsec_micros(),
        );
        self.visible_widgets.fps_label.write().set_text(ts);
    }

    /*
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
     */

    /*
    /// FIXME: should be in platform
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
     */
}

/*
make_frame_graph!(
    FrameGraph {
        buffers: {
            // Note: lock order
            // catalog
            // system
            // game
            composite: CompositeRenderPass,
            ui: UiRenderPass,
            widgets: WidgetBuffer,
            world: WorldRenderPass,
            shapes: ShapeInstanceBuffer,
            terrain: TerrainBuffer,
            atmosphere: AtmosphereBuffer,
            stars: StarsBuffer,
            fullscreen: FullscreenBuffer,
            globals: GlobalParametersBuffer
            // gpu
            // window
            // arcball
            // orrery
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

fn build_frame_graph(
    cpu_detail: CpuDetailLevel,
    gpu_detail: GpuDetailLevel,
    app_dirs: &AppDirs,
    catalog: &Catalog,
    mapper: Arc<RwLock<EventMapper>>,
    window: &mut Window,
    interpreter: &mut Interpreter,
) -> Result<(Arc<RwLock<Gpu>>, FrameGraph)> {
    let gpu = Gpu::new(window, Default::default(), interpreter)?;
    let globals = GlobalParametersBuffer::new(gpu.read().device(), interpreter);
    let fullscreen = FullscreenBuffer::new(&gpu.read());
    let stars = Arc::new(RwLock::new(StarsBuffer::new(&gpu.read())?));
    let atmosphere = AtmosphereBuffer::new(&mut gpu.write())?;
    let terrain = TerrainBuffer::new(
        catalog,
        cpu_detail,
        gpu_detail,
        &globals.read(),
        &mut gpu.write(),
        interpreter,
    )?;
    let shapes = ShapeInstanceBuffer::new(&globals.read(), &atmosphere.read(), &gpu.read())?;
    let world = WorldRenderPass::new(
        &terrain.read(),
        &atmosphere.read(),
        &stars.read(),
        &globals.read(),
        &mut gpu.write(),
        interpreter,
    )?;
    let widgets = WidgetBuffer::new(mapper, &mut gpu.write(), interpreter, &app_dirs.state_dir)?;
    let ui = UiRenderPass::new(
        &widgets.read(),
        &world.read(),
        &globals.read(),
        &mut gpu.write(),
    )?;
    let composite = Arc::new(RwLock::new(CompositeRenderPass::new(
        &ui.read(),
        &world.read(),
        &globals.read(),
        &mut gpu.write(),
    )?));

    let frame_graph = FrameGraph::new(
        composite, ui, widgets, world, shapes, terrain, atmosphere, stars, fullscreen, globals,
    )?;
    Ok((gpu, frame_graph))
}
 */

fn main() -> Result<()> {
    let opt = Opt::from_args();
    env_logger::init();
    InputSystem::run_forever(
        opt,
        WindowBuilder::new().with_title("OpenFA"),
        simulation_main,
    )
}

fn simulation_main(mut runtime: Runtime) -> Result<()> {
    let opt = runtime.resource::<Opt>().to_owned();

    let app_dirs = AppDirs::new(Some("openfa"), true)
        .ok_or_else(|| anyhow!("unable to find app directories"))?;
    create_dir_all(&app_dirs.config_dir)?;
    create_dir_all(&app_dirs.state_dir)?;

    runtime
        .insert_resource(opt.libs_opts)
        .insert_resource(opt.display_opts)
        .insert_resource(opt.startup_opts)
        .insert_resource(opt.detail_opts.cpu_detail())
        .insert_resource(opt.detail_opts.gpu_detail())
        .insert_resource(app_dirs)
        .insert_resource(DemoFocus::Demo)
        .load_extension::<StartupOpts>()?
        .load_extension::<Libs>()?
        .load_extension::<EventMapper<DemoFocus>>()?
        .load_extension::<Window>()?
        .load_extension::<Gpu>()?
        .load_extension::<AtmosphereBuffer>()?
        .load_extension::<FullscreenBuffer>()?
        .load_extension::<GlobalParametersBuffer>()?
        .load_extension::<StarsBuffer>()?
        .load_extension::<TerrainBuffer>()?
        .load_extension::<T2TerrainBuffer>()?
        .load_extension::<WorldRenderPass>()?
        .load_extension::<WidgetBuffer<DemoFocus>>()?
        .load_extension::<UiRenderPass<DemoFocus>>()?
        .load_extension::<CompositeRenderPass<DemoFocus>>()?
        .load_extension::<System>()?
        .load_extension::<Orrery>()?
        .load_extension::<Timeline>()?
        .load_extension::<TimeStep>()?
        .load_extension::<CameraSystem>()?
        .load_extension::<ArcBallSystem>()?
        .load_extension::<TypeManager>()?
        .load_extension::<ShapeBuffer>()?
        .load_extension::<Game>()?;

    ///////////////////////////////////////////////////////////
    // let globals = frame_graph.globals.clone();
    // let widgets = frame_graph.widgets.clone();
    // let shapes = frame_graph.shapes.clone();
    // let world = frame_graph.world.clone();
    // let terrain = frame_graph.terrain.clone();

    // let system = System::new(&catalog.read(), interpreter.clone(), widgets)?;

    ///////////////////////////////////////////////////////////
    // Scene Setup
    /*
    let start = Instant::now();
    let system_palette = Palette::from_bytes(&catalog.read().read_name_sync("PALETTE.PAL")?)?;
    shapes
        .write()
        .set_shared_palette(&system_palette, &gpu.read());
    let mut tracker = Default::default();
    let mut t2_terrain = T2TileSet::new(
        system.read().t2_adjustment(),
        &terrain.read(),
        &globals.read(),
        &gpu.read(),
    )?;
    let type_manager = TypeManager::empty();
    for mm_fid in catalog.read().find_with_extension("MM")? {
        let catalog = catalog.read();
        let name = catalog.stat_sync(mm_fid)?.name().to_owned();
        if name.starts_with('~') || name.starts_with('$') {
            continue;
        }
        println!("Loading {}...", name);
        let raw = catalog.read_sync(mm_fid)?;
        let mm_content = from_dos_string(raw);
        let mm = MissionMap::from_str(&mm_content, &type_manager, &catalog)?;
        let t2_mapper = t2_terrain.add_map(
            &system_palette,
            &mm,
            &catalog,
            &mut gpu.write(),
            &mut tracker,
        )?;

        // shapes.write().finish_open_chunks(&mut gpu.write())?;

        for info in mm.objects() {
            if info.xt().ot().shape.is_none() {
                // FIXME: this still needs to add the entity.
                // I believe these are only for hidden flak guns in TVIET.
                continue;
            }

            let (shape_id, slot_id) = shapes.write().upload_and_allocate_slot(
                info.xt().ot().shape.as_ref().expect("a shape file"),
                DrawSelection::NormalModel,
                &catalog,
                &mut gpu.write(),
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
            let xt = type_manager.load(pt_name, &catalog)?;
            let pt = xt.pt()?;
            let (shape_id, slot_id) = shapes.write().upload_and_allocate_slot(
                pt.nt.ot.shape.as_ref().unwrap(),
                DrawSelection::NormalModel,
                &catalog,
                &mut gpu.write(),
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
        .finish_open_chunks(&mut gpu.write(), &mut tracker)?;
    tracker.dispatch_uploads_one_shot(&mut gpu.write());
    terrain
        .write()
        .add_tile_set(Box::new(t2_terrain) as Box<dyn TileSet>);
    println!("Loading scene took: {:?}", start.elapsed());

    {
        let interp = &mut interpreter;
        system.write().add_default_bindings(interp)?;
    }
     */

    // But we need at least a camera and controller before the sim is ready to run.
    let _player_ent = runtime
        .spawn_named("player")?
        .insert(WorldSpaceFrame::default())
        .insert_scriptable(ArcBallController::default())?
        .insert(ScreenCameraController::default())
        .id();

    runtime.run_startup();
    while runtime.resource::<ExitRequest>().still_running() {
        // Catch monotonic sim time up to system time.
        let frame_start = Instant::now();
        while runtime.resource::<TimeStep>().next_now() < frame_start {
            runtime.run_sim_once();
        }

        // Display a frame
        runtime.run_frame_once();
    }

    /*
    while !system.read().exit {
        {
            let events = input_controller.poll_events()?;
            frame_graph.widgets_mut().track_state_changes(
                now,
                &events,
                &window.read(),
                interpreter.clone(),
            )?;
            frame_graph.globals_mut().track_state_changes(
                arcball.read().camera(),
                &orrery.read(),
                &window.read(),
            );
            let mut sys_lock = system.write();
            let vis_camera = sys_lock.current_camera(arcball.read_recursive().camera());
            frame_graph.shapes_mut().track_state_changes(
                &system_start,
                &now,
                arcball.read().camera(),
                galaxy.world_mut(),
            );
            frame_graph.terrain_mut().track_state_changes(
                arcball.read_recursive().camera(),
                vis_camera,
                catalog.clone(),
            )?;
            arcball.write().track_state_changes();
        }

        /*
        let mut tracker = Default::default();
        frame_graph
            .globals_mut()
            .ensure_uploaded(&gpu.read(), &mut tracker)?;
        frame_graph
            .terrain_mut()
            .ensure_uploaded(&mut gpu.write(), &mut tracker)?;
        frame_graph
            .shapes_mut()
            .ensure_uploaded(&gpu.read(), &mut tracker)?;
        frame_graph.widgets_mut().ensure_uploaded(
            now,
            &mut gpu.write(),
            &window.read(),
            &mut tracker,
        )?;
        if !frame_graph.run(gpu.clone(), tracker)? {
            gpu.write()
                .on_display_config_changed(window.read().config())?;
        }
         */

        system
            .write()
            .track_visible_state(now.elapsed(), &orrery.read(), &mut arcball.write());
    }

    window.write().closing = true;
    render_handle.join().ok();
     */

    Ok(())
}

/*
fn render_main(
    window: Arc<RwLock<Window>>,
    gpu: Arc<RwLock<Gpu>>,
    mut frame_graph: FrameGraph,
) -> Result<()> {
    while !window.read().closing {
        let now = Instant::now();
        let mut tracker = Default::default();
        frame_graph.widgets_mut().ensure_uploaded(
            now,
            &mut gpu.write(),
            &window.read(),
            &mut tracker,
        )?;
        frame_graph
            .shapes_mut()
            .ensure_uploaded(&gpu.read(), &mut tracker)?;
        frame_graph
            .terrain_mut()
            .ensure_uploaded(&mut gpu.write(), &mut tracker)?;
        frame_graph
            .globals_mut()
            .ensure_uploaded(&gpu.read(), &mut tracker)?;
        if !frame_graph.run(gpu.clone(), tracker)? {
            gpu.write()
                .on_display_config_changed(window.read().config())?;
        }
    }

    Ok(())
}
*/
