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
use atmosphere::AtmosphereBuffer;
use camera::ArcBallCamera;
use failure::{bail, Fallible};
use frame_graph::make_frame_graph;
use fullscreen::FullscreenBuffer;
use galaxy::Galaxy;
use global_data::GlobalParametersBuffer;
use gpu::GPU;
use input::{InputBindings, InputSystem};
use log::trace;
use nalgebra::{Point3, UnitQuaternion, Vector3};
use omnilib::{make_opt_struct, OmniLib};
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
        passes: [
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

    let shape_bindings = InputBindings::new("shape")
        .bind("+pan-view", "mouse1")?
        .bind("+move-sun", "mouse2")?
        .bind("+move-view", "mouse3")?
        .bind("exit", "Escape")?
        .bind("exit", "q")?
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
    let mut input = InputSystem::new(vec![shape_bindings])?;
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

    let mut sun_angle = 0.0f64;
    let mut in_sun_move = false;
    let mut camera = ArcBallCamera::new(gpu.aspect_ratio(), 0.001, 3.4e+38);
    camera.set_target(0f64, -10f64, 0f64);

    ///////////////////////////////////////////////////////////
    let atmosphere_buffer = AtmosphereBuffer::new(&mut gpu)?;
    let fullscreen_buffer = FullscreenBuffer::new(gpu.device())?;
    let globals_buffer = GlobalParametersBuffer::new(gpu.device())?;
    let stars_buffer = StarsBuffer::new(gpu.device())?;
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
        .add_screen_text(Font::HUD11, "", gpu.device())?
        .with_color(&[1f32, 0f32, 0f32, 1f32])
        .with_horizontal_position(TextPositionH::Left)
        .with_horizontal_anchor(TextAnchorH::Left)
        .with_vertical_position(TextPositionV::Top)
        .with_vertical_anchor(TextAnchorV::Top);
    let state_handle = text_layout_buffer
        .borrow_mut()
        .add_screen_text(Font::HUD11, "", gpu.device())?
        .with_color(&[1f32, 0.5f32, 0f32, 1f32])
        .with_horizontal_position(TextPositionH::Right)
        .with_horizontal_anchor(TextAnchorH::Right)
        .with_vertical_position(TextPositionV::Bottom)
        .with_vertical_anchor(TextAnchorV::Bottom);

    loop {
        let loop_start = Instant::now();

        for command in input.poll()? {
            match command.name.as_str() {
                "window-resize" => {
                    gpu.note_resize(&input);
                    camera.set_aspect_ratio(gpu.aspect_ratio());
                }
                "window-close" | "window-destroy" | "exit" => return Ok(()),
                "mouse-move" => {
                    if in_sun_move {
                        sun_angle += command.displacement()?.0 / (180.0 * 2.0);
                    } else {
                        camera.on_mousemove(command.displacement()?.0, command.displacement()?.1)
                    }
                }
                "mouse-wheel" => {
                    camera.on_mousescroll(command.displacement()?.0, command.displacement()?.1)
                }
                "+pan-view" => camera.on_mousebutton_down(1),
                "-pan-view" => camera.on_mousebutton_up(1),
                "+move-view" => camera.on_mousebutton_down(3),
                "-move-view" => camera.on_mousebutton_up(3),
                "+move-sun" => in_sun_move = true,
                "-move-sun" => in_sun_move = false,
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

        let sun_direction = Vector3::new(sun_angle.sin() as f32, 0f32, sun_angle.cos() as f32);

        let mut buffers = Vec::new();
        globals_buffer
            .borrow()
            .make_upload_buffer_for_arcball_on_globe(&camera, &gpu, &mut buffers)?;
        atmosphere_buffer
            .borrow()
            .make_upload_buffer(sun_direction, gpu.device(), &mut buffers)?;
        shape_instance_buffer.borrow_mut().make_upload_buffer(
            &galaxy.start_owned(),
            galaxy.world_mut(),
            gpu.device(),
            &mut buffers,
        )?;
        text_layout_buffer
            .borrow()
            .make_upload_buffer(&gpu, &mut buffers)?;
        frame_graph.run(&mut gpu, buffers)?;

        let frame_time = loop_start.elapsed();
        let time_str = format!(
            "{}.{} ms",
            frame_time.as_secs() * 1000 + u64::from(frame_time.subsec_millis()),
            frame_time.subsec_micros()
        );
        fps_handle.set_span(&time_str, gpu.device())?;

        let ds = galaxy
            .world()
            .get_component::<ShapeState>(ent)
            .unwrap()
            .draw_state;
        let params = format!(
            "dist: {}, gear:{}/{:.1}, flaps:{}, brake:{}, hook:{}, bay:{}/{:.1}, aft:{}, swp:{}",
            camera.get_distance(),
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
        state_handle.set_span(&params, gpu.device())?;
    }
}
