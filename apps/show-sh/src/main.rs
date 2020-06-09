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
use frame_graph::make_frame_graph;
use fullscreen::FullscreenBuffer;
use galaxy::Galaxy;
use geodesy::{GeoSurface, Graticule};
use global_data::GlobalParametersBuffer;
use gpu::GPU;
use input::InputSystem;
use log::trace;
use nalgebra::{convert, Point3, UnitQuaternion};
use omnilib::{make_opt_struct, OmniLib};
use orrery::Orrery;
use screen_text::ScreenTextRenderPass;
use shape::ShapeRenderPass;
use shape_instance::{DrawSelection, DrawState, ShapeInstanceBuffer, ShapeState};
use simplelog::{Config, LevelFilter, TermLogger};
use skybox::SkyboxRenderPass;
use stars::StarsBuffer;
use std::time::Instant;
use structopt::StructOpt;
use text_layout::{Font, LayoutBuffer, TextAnchorH, TextAnchorV, TextPositionH, TextPositionV};

make_opt_struct!(
    #[structopt(name = "show-sh", about = "Show the contents of a SH file")]
    Opt {
        #[structopt(help = "Shapes to load")]
        shapes => Vec<String>
    }
);

make_frame_graph!(
    FrameGraph {
        buffers: {
            atmosphere: AtmosphereBuffer,
            fullscreen: FullscreenBuffer,
            globals: GlobalParametersBuffer,
            shape_instance_buffer: ShapeInstanceBuffer,
            stars: StarsBuffer,
            text_layout: LayoutBuffer
        };
        renderers: [
            skybox: SkyboxRenderPass { globals, fullscreen, stars, atmosphere },
            shape: ShapeRenderPass { globals, shape_instance_buffer },
            screen_text: ScreenTextRenderPass { globals, text_layout }
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
    let opt = Opt::from_args();
    TermLogger::init(LevelFilter::Debug, Config::default())?;

    let (omni, inputs) = opt.find_inputs(&opt.shapes)?;
    if inputs.is_empty() {
        bail!("no inputs");
    }
    let (game, shape_name) = inputs.first().unwrap();
    let lib = omni.library(&game);
    let mut galaxy = Galaxy::new(lib)?;

    let system_bindings = Bindings::new("system")
        .bind("exit", "Escape")?
        .bind("exit", "q")?;
    let shape_bindings = Bindings::new("shape")
        .bind("consume-sam", "PageUp")?
        .bind("toggle-gear", "g")?
        .bind("toggle-flaps", "f")?
        .bind("toggle-airbrake", "b")?
        .bind("toggle-hook", "h")?
        .bind("toggle-bay", "o")?
        .bind("toggle-player-dead", "k")?
        .bind("bump-eject-state", "e")?
        .bind("+stick-left", "a")?
        .bind("+stick-right", "d")?
        .bind("+rudder-left", "z")?
        .bind("+rudder-right", "c")?
        .bind("+stick-forward", "w")?
        .bind("+stick-backward", "s")?
        .bind("+vector-thrust-forward", "shift+w")?
        .bind("+vector-thrust-backward", "shift+s")?
        .bind("+increase-wing-sweep", "Period")?
        .bind("+decrease-wing-sweep", "Comma")?
        .bind("enable-afterburner", "key6")?
        .bind("disable-afterburner", "key5")?
        .bind("disable-afterburner", "key4")?
        .bind("disable-afterburner", "key3")?
        .bind("disable-afterburner", "key2")?
        .bind("disable-afterburner", "key1")?;
    let mut input = InputSystem::new(vec![
        Orrery::debug_bindings()?,
        ArcBallCamera::default_bindings()?,
        shape_bindings,
        system_bindings,
    ])?;
    let mut gpu = GPU::new(&input, Default::default())?;

    let shape_instance_buffer = ShapeInstanceBuffer::new(gpu.device())?;

    let (fuel_shape_id, fuel_slot_id) = shape_instance_buffer
        .borrow_mut()
        .upload_and_allocate_slot(
            "FUEL.SH",
            DrawSelection::NormalModel,
            galaxy.palette(),
            galaxy.library(),
            &mut gpu,
        )?;
    galaxy.create_building(
        fuel_slot_id,
        fuel_shape_id,
        shape_instance_buffer.borrow().part(fuel_shape_id),
        4f32,
        Point3::new(0f32, 0f32, 0f32),
        &UnitQuaternion::identity(),
    )?;

    let (f18_shape_id, f18_slot_id) = shape_instance_buffer
        .borrow_mut()
        .upload_and_allocate_slot(
            &shape_name,
            DrawSelection::NormalModel,
            galaxy.palette(),
            galaxy.library(),
            &mut gpu,
        )?;
    let ent = galaxy.create_building(
        f18_slot_id,
        f18_shape_id,
        shape_instance_buffer.borrow().part(f18_shape_id),
        4f32,
        Point3::new(0f32, -10f32, 0f32),
        &UnitQuaternion::identity(),
    )?;

    let (shape_id, slot_id) = shape_instance_buffer
        .borrow_mut()
        .upload_and_allocate_slot(
            &shape_name,
            DrawSelection::NormalModel,
            galaxy.palette(),
            galaxy.library(),
            &mut gpu,
        )?;
    galaxy.create_building(
        slot_id,
        shape_id,
        shape_instance_buffer.borrow().part(shape_id),
        4f32,
        Point3::new(3f32, -10f32, 3f32),
        &UnitQuaternion::identity(),
    )?;

    let (shape_id, slot_id) = shape_instance_buffer
        .borrow_mut()
        .upload_and_allocate_slot(
            &shape_name,
            DrawSelection::NormalModel,
            galaxy.palette(),
            galaxy.library(),
            &mut gpu,
        )?;
    galaxy.create_building(
        slot_id,
        shape_id,
        shape_instance_buffer.borrow().part(shape_id),
        4f32,
        Point3::new(-3f32, -10f32, 3f32),
        &UnitQuaternion::identity(),
    )?;

    shape_instance_buffer
        .borrow_mut()
        .ensure_uploaded(&mut gpu)?;

    let mut orrery = Orrery::now();
    let mut arcball = ArcBallCamera::new(gpu.aspect_ratio(), meters!(0.001), meters!(3.4e+38));
    arcball.set_target(Graticule::<GeoSurface>::new(
        degrees!(0),
        degrees!(0),
        meters!(10),
    ));

    ///////////////////////////////////////////////////////////
    let atmosphere_buffer = AtmosphereBuffer::new(&mut gpu)?;
    let fullscreen_buffer = FullscreenBuffer::new(&gpu)?;
    let globals_buffer = GlobalParametersBuffer::new(gpu.device())?;
    let stars_buffer = StarsBuffer::new(&gpu)?;
    let text_layout_buffer = LayoutBuffer::new(galaxy.library(), &mut gpu)?;

    let frame_graph = FrameGraph::new(
        &mut gpu,
        &atmosphere_buffer,
        &fullscreen_buffer,
        &globals_buffer,
        &shape_instance_buffer,
        &stars_buffer,
        &text_layout_buffer,
    )?;
    ///////////////////////////////////////////////////////////

    let fps_handle = text_layout_buffer
        .borrow_mut()
        .add_screen_text(Font::HUD11, "", &gpu)?
        .with_color(&[1f32, 0f32, 0f32, 1f32])
        .with_horizontal_position(TextPositionH::Left)
        .with_horizontal_anchor(TextAnchorH::Left)
        .with_vertical_position(TextPositionV::Top)
        .with_vertical_anchor(TextAnchorV::Top)
        .handle();
    let state_handle = text_layout_buffer
        .borrow_mut()
        .add_screen_text(Font::HUD11, "", &gpu)?
        .with_color(&[1f32, 0.5f32, 0f32, 1f32])
        .with_horizontal_position(TextPositionH::Right)
        .with_horizontal_anchor(TextAnchorH::Right)
        .with_vertical_position(TextPositionV::Bottom)
        .with_vertical_anchor(TextAnchorV::Bottom)
        .handle();

    loop {
        let loop_start = Instant::now();

        for command in input.poll()? {
            arcball.handle_command(&command)?;
            orrery.handle_command(&command)?;
            match command.name.as_str() {
                "window-close" | "window-destroy" | "exit" => return Ok(()),
                "window-resize" => {
                    gpu.note_resize(&input);
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
                _ => trace!("unhandled command: {}", command.name),
            }
        }

        let mut buffers = Vec::new();
        globals_buffer
            .borrow()
            .make_upload_buffer(arcball.camera(), &gpu, &mut buffers)?;
        //.make_upload_buffer_for_arcball_on_globe(&camera, &gpu, &mut buffers)?;
        atmosphere_buffer.borrow().make_upload_buffer(
            convert(orrery.sun_direction()),
            &gpu,
            &mut buffers,
        )?;
        shape_instance_buffer.borrow_mut().make_upload_buffer(
            &galaxy.start_owned(),
            galaxy.world_mut(),
            &gpu,
            &mut buffers,
        )?;
        text_layout_buffer
            .borrow_mut()
            .make_upload_buffer(&gpu, &mut buffers)?;
        frame_graph.run(&mut gpu, buffers)?;

        let frame_time = loop_start.elapsed();
        let time_str = format!(
            "{}.{} ms",
            frame_time.as_secs() * 1000 + u64::from(frame_time.subsec_millis()),
            frame_time.subsec_micros()
        );
        fps_handle
            .grab(&mut text_layout_buffer.borrow_mut())
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
            .grab(&mut text_layout_buffer.borrow_mut())
            .set_span(&params);
    }
}
