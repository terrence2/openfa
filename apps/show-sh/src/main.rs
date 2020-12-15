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
use atmosphere::AtmosphereBuffer;
use camera::ArcBallCamera;
use command::Bindings;
use failure::{bail, Fallible};
use fnt::{Fnt, Font};
use font_fnt::FntFont;
use fullscreen::FullscreenBuffer;
use galaxy::Galaxy;
use geodesy::{GeoSurface, Graticule};
use global_data::GlobalParametersBuffer;
use gpu::{make_frame_graph, UploadTracker, GPU};
use input::{InputController, InputSystem};
use legion::prelude::*;
use lib::CatalogBuilder;
use log::trace;
use nalgebra::{convert, Point3, UnitQuaternion};
use orrery::Orrery;
use screen_text::ScreenTextRenderPass;
use shape::ShapeRenderPass;
use shape_instance::{DrawSelection, DrawState, ShapeInstanceBuffer, ShapeState};
use stars::StarsBuffer;
use std::time::Instant;
use structopt::StructOpt;
use text_layout::{TextAnchorH, TextAnchorV, TextLayoutBuffer, TextPositionH, TextPositionV};
use winit::window::Window;

/// Show the contents of a SH file
#[derive(Debug, StructOpt)]
struct Opt {
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
            stars: StarsBuffer,
            text_layout: TextLayoutBuffer
        };
        renderers: [
            // terrain: TerrainRenderPass { globals, atmosphere, stars, terrain_geo },
            shape: ShapeRenderPass { globals, atmosphere, shape_instance_buffer },
            screen_text: ScreenTextRenderPass { globals, text_layout }
        ];
        passes: [
            draw: Render(Screen) {
                shape( globals, atmosphere, shape_instance_buffer ),
                screen_text( globals, text_layout )
            }
        ];
    }
);

macro_rules! update {
    ($galaxy:ident, $shape_entity:ident, $func:ident) => {{
        $galaxy
            .world_mut()
            .get_component_mut::<ShapeState>($shape_entity)
            .map(|mut shape| {
                let ds: &mut DrawState = &mut shape.draw_state;
                ds.$func();
            });
    }};

    ($galaxy:ident, $shape_entity:ident, $func:ident, $arg:expr) => {{
        $galaxy
            .world_mut()
            .get_component_mut::<ShapeState>($shape_entity)
            .map(|mut shape| {
                let ds: &mut DrawState = &mut shape.draw_state;
                ds.$func($arg);
            });
    }};
}

fn main() -> Fallible<()> {
    env_logger::init();

    let system_bindings = Bindings::new("system")
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

fn window_main(window: Window, input_controller: &InputController) -> Fallible<()> {
    let opt = Opt::from_args();

    let (mut catalog, inputs) = CatalogBuilder::build_and_select(&opt.inputs)?;
    if inputs.is_empty() {
        bail!("no inputs");
    }
    let fid = *inputs.first().unwrap();

    //let mut async_rt = Runtime::new()?;
    let mut legion = World::default();

    let label = catalog.file_label(fid)?;
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

    let mut orrery = Orrery::now();
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
    let stars_buffer = StarsBuffer::new(&gpu)?;
    let mut text_layout_buffer = TextLayoutBuffer::new(&mut gpu)?;
    let fnt = Fnt::from_bytes(&catalog.read_name_sync("HUD11.FNT")?)?;
    let font = FntFont::from_fnt(&fnt, &mut gpu)?;
    text_layout_buffer.add_font(Font::HUD11.name().into(), font, &gpu);
    // let bgl = *text_layout_buffer.borrow().layout_bind_group_layout();
    // text_layout_buffer.add_font(Font::HUD11.name(), FntFont::new(&mut gpu)?)?;

    let mut frame_graph = FrameGraph::new(
        &mut legion,
        &mut gpu,
        atmosphere_buffer,
        fullscreen_buffer,
        globals_buffer,
        shape_instance_buffer,
        stars_buffer,
        text_layout_buffer,
    )?;
    ///////////////////////////////////////////////////////////

    let fps_handle = frame_graph
        .text_layout
        .add_screen_text(Font::HUD11.name(), "", &gpu)?
        .with_color(&[1f32, 0f32, 0f32, 1f32])
        .with_horizontal_position(TextPositionH::Left)
        .with_horizontal_anchor(TextAnchorH::Left)
        .with_vertical_position(TextPositionV::Top)
        .with_vertical_anchor(TextAnchorV::Top)
        .handle();
    let state_handle = frame_graph
        .text_layout
        .add_screen_text(Font::HUD11.name(), "", &gpu)?
        .with_color(&[1f32, 0.5f32, 0f32, 1f32])
        .with_horizontal_position(TextPositionH::Right)
        .with_horizontal_anchor(TextAnchorH::Right)
        .with_vertical_position(TextPositionV::Bottom)
        .with_vertical_anchor(TextAnchorV::Bottom)
        .handle();

    loop {
        let loop_start = Instant::now();

        for command in input_controller.poll()? {
            arcball.handle_command(&command)?;
            orrery.handle_command(&command)?;
            match command.command() {
                "window-close" | "window-destroy" | "exit" => return Ok(()),
                "window-resize" => {
                    gpu.note_resize(&window);
                    arcball.camera_mut().set_aspect_ratio(gpu.aspect_ratio());
                }
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
                _ => trace!("unhandled command: {}", command.full()),
            }
        }

        let mut tracker = Default::default();

        arcball.think();
        frame_graph
            .globals
            .make_upload_buffer(arcball.camera(), 2.2, &gpu, &mut tracker)?;
        //.make_upload_buffer_for_arcball_on_globe(&camera, &gpu, &mut buffers)?;
        frame_graph.atmosphere.make_upload_buffer(
            convert(orrery.sun_direction()),
            &gpu,
            &mut tracker,
        )?;
        frame_graph.shape_instance_buffer.make_upload_buffer(
            &galaxy.start_time_owned(),
            galaxy.world_mut(),
            &gpu,
            &mut tracker,
        )?;
        frame_graph
            .text_layout
            .make_upload_buffer(&gpu, &mut tracker)?;
        frame_graph.run(&mut gpu, tracker)?;

        let frame_time = loop_start.elapsed();
        let time_str = format!(
            "{}.{} ms",
            frame_time.as_secs() * 1000 + u64::from(frame_time.subsec_millis()),
            frame_time.subsec_micros()
        );
        fps_handle
            .grab(&mut frame_graph.text_layout)
            .set_span(&time_str);

        let ds = galaxy
            .world()
            .get_component::<ShapeState>(ent)
            .unwrap()
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
        state_handle
            .grab(&mut frame_graph.text_layout)
            .set_span(&params);
    }
}
