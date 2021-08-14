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
use chrono::{Duration, TimeZone, Utc};
use composite::CompositeRenderPass;
use fullscreen::FullscreenBuffer;
use galaxy::Galaxy;
use geodesy::{GeoSurface, Graticule, Target};
use global_data::GlobalParametersBuffer;
use gpu::{
    make_frame_graph,
    size::{AbsSize, Size},
    Gpu,
};
use input::{InputController, InputSystem};
use lib::{from_dos_string, CatalogBuilder};
use mm::MissionMap;
use nalgebra::{convert, UnitQuaternion};
use nitrous::{Interpreter, Value};
use nitrous_injector::{inject_nitrous_module, method, NitrousModule};
use orrery::Orrery;
use pal::Palette;
use parking_lot::RwLock;
use shape_instance::{DrawSelection, ShapeInstanceBuffer};
use stars::StarsBuffer;
use std::{path::PathBuf, sync::Arc, time::Instant};
use structopt::StructOpt;
use t2_tile_set::{T2Adjustment, T2TileSet};
use terrain::{CpuDetailLevel, GpuDetailLevel, TerrainBuffer, TileSet};
use tokio::{runtime::Runtime, sync::RwLock as AsyncRwLock};
use ui::UiRenderPass;
use widget::{Color, Extent, Label, Labeled, PositionH, PositionV, WidgetBuffer};
use winit::window::Window;
use world::WorldRenderPass;
use xt::TypeManager;

/// Show the contents of an MM file
#[derive(Debug, StructOpt)]
struct Opt {
    /// Extra directories to treat as libraries
    #[structopt(short, long)]
    libdir: Vec<PathBuf>,

    /// The map file to view
    #[structopt(name = "NAME", last = true)]
    map_names: Vec<String>,
}

#[derive(Debug, NitrousModule)]
struct System {
    exit: bool,
    pin_camera: bool,
    camera: Camera,
    adjust: Arc<RwLock<T2Adjustment>>,
}

#[inject_nitrous_module]
impl System {
    pub fn new(
        interpreter: &mut Interpreter,
        adjust: Arc<RwLock<T2Adjustment>>,
    ) -> Arc<RwLock<Self>> {
        let system = Arc::new(RwLock::new(Self {
            exit: false,
            pin_camera: false,
            camera: Default::default(),
            adjust,
        }));
        interpreter.put_global("system", Value::Module(system.clone()));
        system
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

                bindings.bind("o", "system.terrain_adjust_lon_scale(pressed, 1000.0)");
                bindings.bind("u", "system.terrain_adjust_lon_scale(pressed, -1000.0)");

                bindings.bind("f", "system.terrain_adjust_toggle_hide(pressed)");
            "#,
        )?;
        Ok(())
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
            maintain_font_atlas: Compute() { widgets() },

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

    let mut async_rt = Runtime::new()?;

    let (mut catalog, input_fids) = CatalogBuilder::build_and_select(&opt.map_names)?;
    for (i, d) in opt.libdir.iter().enumerate() {
        catalog.add_labeled_drawer(
            "default",
            DirectoryDrawer::from_directory(100 + i as i64, d)?,
        )?;
    }

    let interpreter = Interpreter::new();
    let gpu = Gpu::new(window, Default::default(), &mut interpreter.write())?;
    let mut galaxy = Galaxy::new(&catalog)?;

    let orrery = Orrery::new(
        Utc.ymd(1964, 8, 24).and_hms(0, 0, 0),
        &mut interpreter.write(),
    );
    let arcball = ArcBallCamera::new(meters!(0.5), &mut gpu.write(), &mut interpreter.write());

    ///////////////////////////////////////////////////////////
    let atmosphere_buffer = AtmosphereBuffer::new(&mut gpu.write())?;
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
    let shapes = ShapeInstanceBuffer::new(&globals.read(), &atmosphere_buffer.read(), &gpu.read())?;
    let world = WorldRenderPass::new(
        &mut gpu.write(),
        &mut interpreter.write(),
        &globals.read(),
        &atmosphere_buffer.read(),
        &stars_buffer.read(),
        &terrain_buffer.read(),
    )?;
    let widgets = WidgetBuffer::new(&mut gpu.write(), &mut interpreter.write())?;
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

    ///////////////////////////////////////////////////////////
    // UI Setup
    let version_label = Label::new("OpenFA v0.1")
        .with_font(widgets.read().font_context().font_id_for_name("fira-sans"))
        .with_color(Color::Green)
        .with_size(Size::from_pts(8.))
        .with_pre_blended_text()
        .wrapped();
    widgets
        .read()
        .root()
        .write()
        .add_child("version", version_label)
        .set_float(PositionH::End, PositionV::Top);

    let fps_label = Label::new("fps")
        .with_font(widgets.read().font_context().font_id_for_name("sans"))
        .with_color(Color::Red)
        .with_size(Size::from_pts(13.))
        .with_pre_blended_text()
        .wrapped();
    widgets
        .read()
        .root()
        .write()
        .add_child("fps", fps_label.clone())
        .set_float(PositionH::Start, PositionV::Bottom);

    ///////////////////////////////////////////////////////////
    // Scene Setup
    let t2_adjustment = Arc::new(RwLock::new(T2Adjustment::default()));
    let mut tracker = Default::default();
    let mut t2_tile_set = T2TileSet::new(
        t2_adjustment.clone(),
        &terrain_buffer.read(),
        &globals.read(),
        &gpu.read(),
    )?;
    let start = Instant::now();
    let type_manager = TypeManager::empty();
    for mm_fid in &input_fids {
        let name = catalog.stat_sync(*mm_fid)?.name().to_owned();
        if name.starts_with('~') || name.starts_with('$') {
            continue;
        }
        println!("Loading {}...", name);
        catalog.set_default_label(&catalog.file_label(*mm_fid)?);
        let system_palette = Palette::from_bytes(&catalog.read_name_sync("PALETTE.PAL")?)?;
        let raw = catalog.read_sync(*mm_fid)?;
        let mm_content = from_dos_string(raw);
        let mm = MissionMap::from_str(&mm_content, &type_manager, &catalog)?;
        let _t2_mapper = t2_tile_set.add_map(
            &system_palette,
            &mm,
            &catalog,
            &mut gpu.write(),
            &async_rt,
            &mut tracker,
        )?;

        let (shape_id, slot_id) = shapes.write().upload_and_allocate_slot(
            "BNK2.SH",
            DrawSelection::NormalModel,
            &system_palette,
            &catalog,
            &mut gpu.write(),
        )?;
        shapes.write().ensure_uploaded(&mut gpu.write())?;
        galaxy.create_building(
            slot_id,
            shape_id,
            shapes.read().part(shape_id),
            4.,
            Graticule::new(degrees!(0), degrees!(0), meters!(0)),
            &UnitQuaternion::identity(),
        )?;

        /*
        for info in mm.objects() {
            if info.xt().ot().shape.is_none() {
                // FIXME: this still needs to add the entity.
                // I believe these are only for hidden flak guns in TVIET.
                continue;
            }

            let (shape_id, slot_id) = shapes.write().upload_and_allocate_slot(
                info.xt().ot().shape.as_ref().expect("a shape file"),
                DrawSelection::NormalModel,
                &system_palette,
                &catalog,
                &mut gpu.write(),
            )?;
            let aabb = *shapes
                .read()
                .part(shape_id)
                .widgets()
                .read()
                .unwrap()
                .aabb();

            if let Ok(_pt) = info.xt().pt() {
                //galaxy.create_flyer(pt, shape_id, slot_id)?
                //unimplemented!()
            } else if let Ok(_nt) = info.xt().nt() {
                //galaxy.create_ground_mover(nt)
                //unimplemented!()
            } else if info.xt().jt().is_ok() {
                bail!("did not expect a projectile in MM objects")
            } else {
                let grat = t2_mapper.fa2grat(info.position());
                println!("{:?}: {}", info.name(), grat);
                let scale = 4f32;
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
                   let mut p = info.position();
                   let ns_ft = t2_buffer.t2().extent_north_south_in_ft();
                   p.coords[2] = ns_ft - p.coords[2]; // flip z for vulkan
                   p *= FEET_TO_HM_32;
                   p.coords[1] = /*t2_buffer.borrow().ground_height_at_tile(&p)*/
                       -aabb[1][1] * scale * FEET_TO_HM_32;
                   positions.push(p);
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
                   galaxy.create_building(
                       slot_id,
                       shape_id,
                       shape_instance_buffer.part(shape_id),
                       scale,
                       p,
                       info.angle(),
                   )?;
                */
            };
        }
         */
    }
    tracker.dispatch_uploads_one_shot(&mut gpu.write());
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

    arcball.write().set_target(Graticule::<GeoSurface>::new(
        degrees!(0),
        degrees!(0),
        meters!(2),
    ));
    arcball.write().set_eye_relative(Graticule::<Target>::new(
        degrees!(10),
        degrees!(0),
        meters!(15),
    ))?;
    // London: 51.5,-0.1
    // arcball.write().set_target(Graticule::<GeoSurface>::new(
    //     degrees!(51.5),
    //     degrees!(-0.1),
    //     meters!(8000.),
    // ));
    // arcball.write().set_eye_relative(Graticule::<Target>::new(
    //     degrees!(11.5),
    //     degrees!(869.5),
    //     meters!(67668.5053),
    // ))?;
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

    let system = System::new(&mut interpreter.write(), t2_adjustment);

    {
        let interp = &mut interpreter.write();
        gpu.write().add_default_bindings(interp)?;
        orrery.write().add_default_bindings(interp)?;
        arcball.write().add_default_bindings(interp)?;
        globals.write().add_default_bindings(interp)?;
        world.write().add_default_bindings(interp)?;
        system.write().add_default_bindings(interp)?;
    }

    let catalog = Arc::new(AsyncRwLock::new(catalog));

    let mut now = Instant::now();
    let system_start = now;
    while !system.read().exit {
        orrery
            .write()
            .adjust_time(Duration::from_std(now.elapsed())?);
        now = Instant::now();

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
            arcball.read().camera(),
            system.write().get_camera(arcball.read().camera()),
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

        let frame_time = now.elapsed();
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
