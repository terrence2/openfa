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
use anyhow::{bail, Result};
use atmosphere::AtmosphereBuffer;
use camera::ArcBallCamera;
use catalog::DirectoryDrawer;
use chrono::{TimeZone, Utc};
use command::{Bindings, CommandHandler};
use composite::CompositeRenderPass;
use fnt::Fnt;
use font_fnt::FntFont;
use fullscreen::FullscreenBuffer;
use galaxy::Galaxy;
use geodesy::{GeoSurface, Graticule, Target};
use global_data::GlobalParametersBuffer;
use gpu::{make_frame_graph, GPU};
use input::{InputController, InputSystem};
use legion::world::World;
use lib::CatalogBuilder;
use log::trace;
use nalgebra::{convert, Point3, UnitQuaternion};
use orrery::Orrery;
use shape::ShapeRenderPass;
use shape_instance::{DrawSelection, ShapeInstanceBuffer, ShapeState};
use stars::StarsBuffer;
use std::{path::PathBuf, sync::Arc, time::Instant};
use structopt::StructOpt;
use terrain_geo::{CpuDetailLevel, GpuDetailLevel, TerrainGeoBuffer};
use tokio::{runtime::Runtime as TokioRuntime, sync::RwLock as AsyncRwLock};
use ui::UiRenderPass;
use widget::{Color, Label, PositionH, PositionV, Terminal, WidgetBuffer};
use winit::window::Window;
use world::WorldRenderPass;

/// Show the contents of a SH file
#[derive(Debug, StructOpt)]
struct Opt {
    /// Extra directories to treat as libraries
    #[structopt(short, long)]
    libdir: Vec<PathBuf>,

    /// Shapes to show
    inputs: Vec<String>,
}

make_frame_graph!(
    FrameGraph {
        buffers: {
            atmosphere: AtmosphereBuffer,
            fullscreen: FullscreenBuffer,
            globals: GlobalParametersBuffer,
            shape_instance_buffer: ShapeInstanceBuffer,
            shape: ShapeRenderPass,
            stars: StarsBuffer,
            terrain_geo: TerrainGeoBuffer,
            widgets: WidgetBuffer,
            world: WorldRenderPass,
            ui: UiRenderPass,
            composite: CompositeRenderPass
        };
        passes: [
            // terrain_geo
            // Update the indices so we have correct height data to tessellate with and normal
            // and color data to accumulate.
            paint_atlas_indices: Any() { terrain_geo() },
            // Apply heights to the terrain mesh.
            tessellate: Compute() { terrain_geo() },
            // Render the terrain mesh's texcoords to an offscreen buffer.
            deferred_texture: Render(terrain_geo, deferred_texture_target) {
                terrain_geo( globals )
            },
            // Accumulate normal and color data.
            accumulate_normal_and_color: Compute() { terrain_geo( globals ) },

            // world: Flatten terrain g-buffer into the final image and mix in stars.
            render_world: Render(world, offscreen_target) {
                world( globals, fullscreen, atmosphere, stars, terrain_geo )
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

macro_rules! update {
    ($galaxy:ident, $shape_entity:ident, $func:ident) => {{
        $galaxy.world_mut().entry($shape_entity).map(|entry| {
            if let Ok(state) = entry.into_component_mut::<ShapeState>() {
                state.draw_state.$func();
            }
        });
    }};

    ($galaxy:ident, $shape_entity:ident, $func:ident, $arg:expr) => {{
        $galaxy.world_mut().entry($shape_entity).map(|entry| {
            if let Ok(state) = entry.into_component_mut::<ShapeState>() {
                state.draw_state.$func($arg);
            }
        });
    }};
}

fn main() -> Result<()> {
    env_logger::init();

    let system_bindings = Bindings::new("system")
        .bind("world.toggle_wireframe", "w")?
        .bind("world.toggle_debug_mode", "r")?
        .bind("system.exit", "Escape")?
        .bind("system.exit", "q")?;
    let shape_bindings = Bindings::new("shape")
        .bind("shape.consume-sam", "PageUp")?
        .bind("shape.toggle-gear", "g")?
        .bind("shape.toggle-flaps", "f")?
        .bind("shape.toggle-airbrake", "b")?
        .bind("shape.toggle-hook", "h")?
        .bind("shape.toggle-bay", "o")?
        .bind("shape.toggle-player-dead", "k")?
        .bind("shape.bump-eject-state", "e")?
        .bind("shape.+stick-left", "a")?
        .bind("shape.+stick-right", "d")?
        .bind("shape.+rudder-left", "z")?
        .bind("shape.+rudder-right", "c")?
        .bind("shape.+stick-forward", "w")?
        .bind("shape.+stick-backward", "s")?
        .bind("shape.+vector-thrust-forward", "shift+w")?
        .bind("shape.+vector-thrust-backward", "shift+s")?
        .bind("shape.+increase-wing-sweep", "Period")?
        .bind("shape.+decrease-wing-sweep", "Comma")?
        .bind("shape.enable-afterburner", "key6")?
        .bind("shape.disable-afterburner", "key5")?
        .bind("shape.disable-afterburner", "key4")?
        .bind("shape.disable-afterburner", "key3")?
        .bind("shape.disable-afterburner", "key2")?
        .bind("shape.disable-afterburner", "key1")?;
    InputSystem::run_forever(
        vec![
            Orrery::debug_bindings()?,
            ArcBallCamera::default_bindings()?,
            shape_bindings,
            system_bindings,
        ],
        window_main,
    )
}

fn window_main(window: Window, input_controller: &InputController) -> Result<()> {
    let opt = Opt::from_args();

    let (mut catalog, inputs) = CatalogBuilder::build_and_select(&opt.inputs)?;
    for (i, d) in opt.libdir.iter().enumerate() {
        catalog.add_drawer(DirectoryDrawer::from_directory(100 + i as i64, d)?)?;
    }
    if inputs.is_empty() {
        bail!("no inputs");
    }
    let fid = *inputs.first().unwrap();

    let (cpu_detail, gpu_detail) = if cfg!(debug_assertions) {
        (CpuDetailLevel::Low, GpuDetailLevel::Low)
    } else {
        (CpuDetailLevel::Medium, GpuDetailLevel::High)
    };

    let mut async_rt = TokioRuntime::new()?;
    let mut legion = World::default();

    let label = catalog.file_label(fid)?;
    println!("Default label: {}", label);
    catalog.set_default_label(&label);
    let meta = catalog.stat_sync(fid)?;
    let shape_name = meta.name;
    let mut galaxy = Galaxy::new(&catalog)?;

    let mut gpu = GPU::new(&window, Default::default())?;

    let mut shape_instance_buffer = ShapeInstanceBuffer::new(gpu.device())?;

    let (fuel_shape_id, fuel_slot_id) = shape_instance_buffer.upload_and_allocate_slot(
        "FUEL.SH",
        DrawSelection::NormalModel,
        galaxy.palette(),
        &catalog,
        &mut gpu,
    )?;
    galaxy.create_building(
        fuel_slot_id,
        fuel_shape_id,
        shape_instance_buffer.part(fuel_shape_id),
        4f32,
        Point3::new(0f32, 0f32, 0f32),
        &UnitQuaternion::identity(),
    )?;

    let (f18_shape_id, f18_slot_id) = shape_instance_buffer.upload_and_allocate_slot(
        &shape_name,
        DrawSelection::NormalModel,
        galaxy.palette(),
        &catalog,
        &mut gpu,
    )?;
    let ent = galaxy.create_building(
        f18_slot_id,
        f18_shape_id,
        shape_instance_buffer.part(f18_shape_id),
        4f32,
        Point3::new(0f32, -10f32, 0f32),
        &UnitQuaternion::identity(),
    )?;

    let (shape_id, slot_id) = shape_instance_buffer.upload_and_allocate_slot(
        &shape_name,
        DrawSelection::NormalModel,
        galaxy.palette(),
        &catalog,
        &mut gpu,
    )?;
    galaxy.create_building(
        slot_id,
        shape_id,
        shape_instance_buffer.part(shape_id),
        4f32,
        Point3::new(3f32, -10f32, 3f32),
        &UnitQuaternion::identity(),
    )?;

    let (shape_id, slot_id) = shape_instance_buffer.upload_and_allocate_slot(
        &shape_name,
        DrawSelection::NormalModel,
        galaxy.palette(),
        &catalog,
        &mut gpu,
    )?;
    galaxy.create_building(
        slot_id,
        shape_id,
        shape_instance_buffer.part(shape_id),
        4f32,
        Point3::new(-3f32, -10f32, 3f32),
        &UnitQuaternion::identity(),
    )?;

    shape_instance_buffer.ensure_uploaded(&mut gpu)?;

    let mut orrery = Orrery::new(Utc.ymd(1964, 2, 24).and_hms(12, 0, 0));
    let mut arcball = ArcBallCamera::new(gpu.aspect_ratio(), meters!(0.001));
    arcball.set_target(Graticule::<GeoSurface>::new(
        degrees!(0),
        degrees!(0),
        meters!(10),
    ));

    ///////////////////////////////////////////////////////////
    let atmosphere_buffer = AtmosphereBuffer::new(false, &mut gpu)?;
    let fullscreen_buffer = FullscreenBuffer::new(&gpu)?;
    let globals_buffer = GlobalParametersBuffer::new(gpu.device())?;
    let shape_render_pass = ShapeRenderPass::new(
        &gpu,
        &globals_buffer,
        &atmosphere_buffer,
        &shape_instance_buffer,
    )?;
    let stars_buffer = StarsBuffer::new(&gpu)?;
    let widget_buffer = WidgetBuffer::new(&mut gpu)?;
    let terrain_geo_buffer =
        TerrainGeoBuffer::new(&catalog, cpu_detail, gpu_detail, &globals_buffer, &mut gpu)?;
    let world_render_pass = WorldRenderPass::new(
        &mut gpu,
        &globals_buffer,
        &atmosphere_buffer,
        &stars_buffer,
        &terrain_geo_buffer,
    )?;
    let ui_render_pass = UiRenderPass::new(
        &mut gpu,
        &globals_buffer,
        &widget_buffer,
        &world_render_pass,
    )?;
    let composite_render_pass = CompositeRenderPass::new(
        &mut gpu,
        &globals_buffer,
        &world_render_pass,
        &ui_render_pass,
    )?;

    let mut frame_graph = FrameGraph::new(
        &mut legion,
        &mut gpu,
        atmosphere_buffer,
        fullscreen_buffer,
        globals_buffer,
        shape_instance_buffer,
        shape_render_pass,
        stars_buffer,
        terrain_geo_buffer,
        widget_buffer,
        world_render_pass,
        ui_render_pass,
        composite_render_pass,
    )?;
    ///////////////////////////////////////////////////////////

    let fnt = Fnt::from_bytes(&catalog.read_name_sync("HUD11.FNT")?)?;
    let font = FntFont::from_fnt(&fnt)?;
    frame_graph.widgets.add_font("HUD11", font);

    let catalog = Arc::new(AsyncRwLock::new(catalog));

    // let version_label = Label::new("OpenFA show-sh v0.0")
    //     .with_color(Color::Green)
    //     .with_size(8.0)
    //     .wrapped();
    // frame_graph
    //     .widgets
    //     .root()
    //     .write()
    //     .add_child(version_label)
    //     .set_float(PositionH::End, PositionV::Bottom);

    // let fps_label = Label::new("fps")
    //     .with_color(Color::Red)
    //     .with_size(13.0)
    //     .wrapped();
    // frame_graph
    //     .widgets
    //     .root()
    //     .write()
    //     .add_child(fps_label.clone())
    //     .set_float(PositionH::Start, PositionV::Top);

    let state_label = Label::new("state")
        .with_font(frame_graph.widgets.font_context().font_id_for_name("HUD11"))
        .with_color(Color::Orange)
        .wrapped();
    frame_graph
        .widgets
        .root()
        .write()
        .add_child(state_label.clone())
        .set_float(PositionH::Start, PositionV::Center);

    // let terminal = Terminal::new(frame_graph.widgets.font_context())
    //     .with_visible(false)
    //     .wrapped();
    // frame_graph
    //     .widgets
    //     .root()
    //     .write()
    //     .add_child(terminal)
    //     .set_float(PositionH::Start, PositionV::Top);

    // everest: 27.9880704,86.9245623
    arcball.set_target(Graticule::<GeoSurface>::new(
        degrees!(27.9880704),
        degrees!(-86.9245623), // FIXME: wat?
        meters!(8000.),
    ));
    arcball.set_eye_relative(Graticule::<Target>::new(
        degrees!(11.5),
        degrees!(869.5),
        meters!(67668.5053),
    ))?;

    let tone_gamma = 2.2f32;
    let _is_camera_pinned = false;
    let _camera_double = arcball.camera().to_owned();
    let _target_vec = meters!(0f64);
    let _show_terminal = false;
    loop {
        let loop_start = Instant::now();

        frame_graph
            .widgets
            .handle_keyboard(&input_controller.poll_keyboard()?)?;
        for command in input_controller.poll_commands()? {
            if InputSystem::is_close_command(&command) || command.full() == "system.exit" {
                return Ok(());
            }
            frame_graph.handle_command(&command);
            arcball.handle_command(&command)?;
            orrery.handle_command(&command)?;
            match command.command() {
                "+rudder-left" => update!(galaxy, ent, move_rudder_left),
                "-rudder-left" => update!(galaxy, ent, move_rudder_center),
                "+rudder-right" => update!(galaxy, ent, move_rudder_right),
                "-rudder-right" => update!(galaxy, ent, move_rudder_center),
                "+stick-backward" => update!(galaxy, ent, move_stick_backward),
                "-stick-backward" => update!(galaxy, ent, move_stick_center),
                "+stick-forward" => update!(galaxy, ent, move_stick_forward),
                "-stick-forward" => update!(galaxy, ent, move_stick_center),
                "+stick-left" => update!(galaxy, ent, move_stick_left),
                "-stick-left" => update!(galaxy, ent, move_stick_center),
                "+stick-right" => update!(galaxy, ent, move_stick_right),
                "-stick-right" => update!(galaxy, ent, move_stick_center),
                "+vector-thrust-backward" => update!(galaxy, ent, vector_thrust_backward),
                "+vector-thrust-forward" => update!(galaxy, ent, vector_thrust_forward),
                "-vector-thrust-forward" => update!(galaxy, ent, vector_thrust_stop),
                "-vector-thrust-backward" => update!(galaxy, ent, vector_thrust_stop),
                "bump-eject-state" => update!(galaxy, ent, bump_eject_state),
                "consume-sam" => update!(galaxy, ent, consume_sam),
                "+decrease-wing-sweep" => update!(galaxy, ent, decrease_wing_sweep),
                "+increase-wing-sweep" => update!(galaxy, ent, increase_wing_sweep),
                "-decrease-wing-sweep" => update!(galaxy, ent, stop_wing_sweep),
                "-increase-wing-sweep" => update!(galaxy, ent, stop_wing_sweep),
                "disable-afterburner" => update!(galaxy, ent, disable_afterburner),
                "enable-afterburner" => update!(galaxy, ent, enable_afterburner),
                "toggle-airbrake" => update!(galaxy, ent, toggle_airbrake),
                "toggle-bay" => update!(galaxy, ent, toggle_bay, &loop_start),
                "toggle-flaps" => {
                    update!(galaxy, ent, toggle_flaps);
                    update!(galaxy, ent, toggle_slats);
                }
                "toggle-gear" => update!(galaxy, ent, toggle_gear, &loop_start),
                "toggle-hook" => update!(galaxy, ent, toggle_hook),
                "toggle-player-dead" => update!(galaxy, ent, toggle_player_dead),
                "window-cursor-move" => {}
                // system bindings
                "window.resize" => {
                    gpu.note_resize(None, &window);
                    frame_graph.terrain_geo.note_resize(&gpu);
                    frame_graph.world.note_resize(&gpu);
                    frame_graph.ui.note_resize(&gpu);
                    arcball.camera_mut().set_aspect_ratio(gpu.aspect_ratio());
                }
                "window.dpi-change" => {
                    gpu.note_resize(Some(command.float(0)?), &window);
                    frame_graph.terrain_geo.note_resize(&gpu);
                    frame_graph.world.note_resize(&gpu);
                    frame_graph.ui.note_resize(&gpu);
                    arcball.camera_mut().set_aspect_ratio(gpu.aspect_ratio());
                }
                _ => trace!("unhandled command: {}", command.full()),
            }
        }

        let mut tracker = Default::default();
        arcball.think();
        frame_graph.globals().make_upload_buffer(
            arcball.camera(),
            tone_gamma,
            &gpu,
            &mut tracker,
        )?;
        frame_graph.atmosphere().make_upload_buffer(
            convert(orrery.sun_direction()),
            &gpu,
            &mut tracker,
        )?;
        frame_graph.terrain_geo().make_upload_buffer(
            arcball.camera(),
            &arcball.camera(),
            catalog.clone(),
            &mut async_rt,
            &mut gpu,
            &mut tracker,
        )?;
        frame_graph
            .widgets()
            .make_upload_buffer(&gpu, &mut tracker)?;
        frame_graph.shape_instance_buffer().make_upload_buffer(
            &galaxy.start_time_owned(),
            galaxy.world_mut(),
            &gpu,
            &mut tracker,
        )?;
        if !frame_graph.run(&mut gpu, tracker)? {
            gpu.note_resize(None, &window);
            frame_graph.terrain_geo.note_resize(&gpu);
            frame_graph.world.note_resize(&gpu);
            frame_graph.ui.note_resize(&gpu);
            arcball.camera_mut().set_aspect_ratio(gpu.aspect_ratio());
        }

        // let frame_time = loop_start.elapsed();
        // let time_str = format!(
        //     "{}.{} ms",
        //     frame_time.as_secs() * 1000 + u64::from(frame_time.subsec_millis()),
        //     frame_time.subsec_micros()
        // );
        // fps_label.write().set_text(&time_str);

        let ds = galaxy
            .world_mut()
            .entry(ent)
            .unwrap()
            .into_component::<ShapeState>()?
            .draw_state;
        let params = format!(
            "dist: {}, gear:{}/{:.1}, flaps:{}, brake:{}, hook:{}, bay:{}/{:.1}, aft:{}, swp:{}",
            arcball.get_distance(),
            !ds.gear_retracted(),
            ds.gear_position(),
            ds.flaps_down(),
            ds.airbrake_extended(),
            ds.hook_extended(),
            !ds.bay_closed(),
            ds.bay_position(),
            ds.afterburner_enabled(),
            ds.wing_sweep_angle(),
        );
        state_label.write().set_text(&params);
    }
}
