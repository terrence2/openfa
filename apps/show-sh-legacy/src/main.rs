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
use camera::ArcBallCamera;
use failure::{bail, Fallible};
use input::{InputBindings, InputSystem};
use legacy_shape::{upload::DrawSelection, ShapeRenderer};
use log::trace;
use nalgebra::{Unit, UnitQuaternion, Vector3};
use omnilib::{make_opt_struct, OmniLib};
use pal::Palette;
use sh::RawShape;
use simplelog::{Config, LevelFilter, TermLogger};
use skybox::SkyboxRenderer;
use std::{f64::consts::PI, rc::Rc, time::Instant};
use structopt::StructOpt;
use text::{Font, TextAnchorH, TextAnchorV, TextPositionH, TextPositionV, TextRenderer};
use vulkano::command_buffer::AutoCommandBufferBuilder;
use window::{GraphicsConfigBuilder, GraphicsWindow};

make_opt_struct!(
    #[structopt(name = "sh_explorer", about = "Show the contents of a SH file")]
    Opt {
        #[structopt(help = "Shapes to load")]
        shapes => Vec<String>
    }
);

fn main() -> Fallible<()> {
    let opt = Opt::from_args();
    TermLogger::init(LevelFilter::Debug, Config::default())?;

    let (omni, inputs) = opt.find_inputs(&opt.shapes)?;
    if inputs.is_empty() {
        bail!("no inputs");
    }
    let (game, _) = inputs.first().unwrap();
    let lib = omni.library(&game);
    let system_palette = Rc::new(Box::new(Palette::from_bytes(&lib.load("PALETTE.PAL")?)?));

    let mut window = GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?;
    let shape_bindings = InputBindings::new("shape")
        .bind("+pan-view", "mouse1")?
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
    let mut input = InputSystem::new(&[&shape_bindings]);

    let mut skybox_renderer = SkyboxRenderer::new(&window)?;

    let mut text_renderer = TextRenderer::new(&lib, &window)?;
    let fps_handle = text_renderer
        .add_screen_text(Font::HUD11, "", &window)?
        .with_color(&[1f32, 0f32, 0f32, 1f32])
        .with_horizontal_position(TextPositionH::Left)
        .with_horizontal_anchor(TextAnchorH::Left)
        .with_vertical_position(TextPositionV::Top)
        .with_vertical_anchor(TextAnchorV::Top);
    let state_handle = text_renderer
        .add_screen_text(Font::HUD11, "", &window)?
        .with_color(&[1f32, 0.5f32, 0f32, 1f32])
        .with_horizontal_position(TextPositionH::Right)
        .with_horizontal_anchor(TextAnchorH::Right)
        .with_vertical_position(TextPositionV::Bottom)
        .with_vertical_anchor(TextAnchorV::Bottom);

    let mut sh_renderer = ShapeRenderer::new(&window)?;

    let mut instances = Vec::new();
    for (game, name) in &inputs {
        let lib = omni.library(&game);
        let sh = RawShape::from_bytes(&lib.load(&name)?)?;
        let instance = sh_renderer.add_shape_to_render(
            name,
            &sh,
            DrawSelection::NormalModel,
            &system_palette,
            &lib,
            &window,
        )?;
        instances.push(instance);
    }
    sh_renderer.finish_loading_phase(&window)?;

    let mut camera = ArcBallCamera::new(window.aspect_ratio_f64()?, 0.1, 3.4e+38);
    camera.set_up(-Vector3::x());
    camera.set_rotation(UnitQuaternion::from_axis_angle(
        &Unit::new_normalize(Vector3::z()),
        PI / 2.0,
    ));
    camera.set_target(6_378_000.0 + 1000.0, 0.0, 0.0);
    camera.set_distance(40.0);

    loop {
        let loop_start = Instant::now();

        for command in input.poll(&mut window.events_loop) {
            match command.name.as_str() {
                "window-resize" => {
                    window.note_resize();
                    camera.set_aspect_ratio(window.aspect_ratio_f64()?);
                }
                "window-close" | "window-destroy" | "exit" => return Ok(()),
                "mouse-move" => {
                    camera.on_mousemove(command.displacement()?.0, command.displacement()?.1)
                }
                "mouse-wheel" => {
                    camera.on_mousescroll(command.displacement()?.0, command.displacement()?.1)
                }
                "+pan-view" => camera.on_mousebutton_down(1),
                "-pan-view" => camera.on_mousebutton_up(1),
                "+move-view" => camera.on_mousebutton_down(3),
                "-move-view" => camera.on_mousebutton_up(3),
                "+rudder-left" => instances[0].draw_state().borrow_mut().move_rudder_left(),
                "-rudder-left" => instances[0].draw_state().borrow_mut().move_rudder_center(),
                "+rudder-right" => instances[0].draw_state().borrow_mut().move_rudder_right(),
                "-rudder-right" => instances[0].draw_state().borrow_mut().move_rudder_center(),
                "+stick-backward" => instances[0].draw_state().borrow_mut().move_stick_backward(),
                "-stick-backward" => instances[0].draw_state().borrow_mut().move_stick_center(),
                "+stick-forward" => instances[0].draw_state().borrow_mut().move_stick_forward(),
                "-stick-forward" => instances[0].draw_state().borrow_mut().move_stick_center(),
                "+stick-left" => instances[0].draw_state().borrow_mut().move_stick_left(),
                "-stick-left" => instances[0].draw_state().borrow_mut().move_stick_center(),
                "+stick-right" => instances[0].draw_state().borrow_mut().move_stick_right(),
                "-stick-right" => instances[0].draw_state().borrow_mut().move_stick_center(),
                "+vector-thrust-backward" => instances[0]
                    .draw_state()
                    .borrow_mut()
                    .vector_thrust_backward(),
                "+vector-thrust-forward" => instances[0]
                    .draw_state()
                    .borrow_mut()
                    .vector_thrust_forward(),
                "-vector-thrust-forward" => {
                    instances[0].draw_state().borrow_mut().vector_thrust_stop()
                }
                "-vector-thrust-backward" => {
                    instances[0].draw_state().borrow_mut().vector_thrust_stop()
                }
                "bump-eject-state" => instances[0].draw_state().borrow_mut().bump_eject_state(),
                "consume-sam" => instances[0].draw_state().borrow_mut().consume_sam(),
                "+decrease-wing-sweep" => {
                    instances[0].draw_state().borrow_mut().decrease_wing_sweep()
                }
                "+increase-wing-sweep" => {
                    instances[0].draw_state().borrow_mut().increase_wing_sweep()
                }
                "-decrease-wing-sweep" => instances[0].draw_state().borrow_mut().stop_wing_sweep(),
                "-increase-wing-sweep" => instances[0].draw_state().borrow_mut().stop_wing_sweep(),
                "disable-afterburner" => {
                    instances[0].draw_state().borrow_mut().disable_afterburner()
                }
                "enable-afterburner" => instances[0].draw_state().borrow_mut().enable_afterburner(),
                "toggle-airbrake" => instances[0].draw_state().borrow_mut().toggle_airbrake(),
                "toggle-bay" => instances[0]
                    .draw_state()
                    .borrow_mut()
                    .toggle_bay(&loop_start),
                "toggle-flaps" => {
                    instances[0].draw_state().borrow_mut().toggle_flaps();
                    instances[0].draw_state().borrow_mut().toggle_slats();
                }
                "toggle-gear" => instances[0]
                    .draw_state()
                    .borrow_mut()
                    .toggle_gear(&loop_start),
                "toggle-hook" => instances[0].draw_state().borrow_mut().toggle_hook(),
                "toggle-player-dead" => instances[0].draw_state().borrow_mut().toggle_player_dead(),
                "window-cursor-move" => {}
                _ => trace!("unhandled command: {}", command.name),
            }
        }

        sh_renderer.animate(&loop_start)?;

        {
            let frame = window.begin_frame()?;
            if !frame.is_valid() {
                continue;
            }

            let sun_angle = 0.0f64;
            let sun_direction = Vector3::new(sun_angle.sin() as f32, 0f32, sun_angle.cos() as f32);
            skybox_renderer.before_frame(&camera, &sun_direction)?;
            text_renderer.before_frame(&window)?;

            let mut cbb = AutoCommandBufferBuilder::primary_one_time_submit(
                window.device(),
                window.queue().family(),
            )?;

            cbb = sh_renderer.before_render(cbb)?;

            cbb = cbb.begin_render_pass(
                frame.framebuffer(&window),
                false,
                vec![[0f32, 0f32, 1f32, 1f32].into(), 0f32.into()],
            )?;

            cbb = skybox_renderer.draw(cbb, &window.dynamic_state)?;
            cbb = sh_renderer.render(&camera, cbb, &window.dynamic_state, &window)?;
            cbb = text_renderer.render(cbb, &window.dynamic_state)?;

            cbb = cbb.end_render_pass()?;

            let cb = cbb.build()?;

            frame.submit(cb, &mut window)?;
        }

        let frame_time = loop_start.elapsed();
        let render_time = frame_time - window.idle_time;
        let ts = format!(
            "frame: {}.{}ms / render: {}.{}ms",
            frame_time.as_secs() * 1000 + u64::from(frame_time.subsec_millis()),
            frame_time.subsec_micros(),
            render_time.as_secs() * 1000 + u64::from(render_time.subsec_millis()),
            render_time.subsec_micros(),
        );
        fps_handle.set_span(&ts, &window)?;

        let params = format!(
            "dist: {}, gear:{}/{:.1}, flaps:{}, brake:{}, hook:{}, bay:{}/{:.1}, aft:{}, swp:{}",
            camera.get_distance(),
            !instances[0].draw_state().borrow().gear_retracted(),
            instances[0].draw_state().borrow().gear_position(),
            instances[0].draw_state().borrow().flaps_down(),
            instances[0].draw_state().borrow().airbrake_extended(),
            instances[0].draw_state().borrow().hook_extended(),
            !instances[0].draw_state().borrow().bay_closed(),
            instances[0].draw_state().borrow().bay_position(),
            instances[0].draw_state().borrow().afterburner_enabled(),
            instances[0].draw_state().borrow().wing_sweep_angle(),
        );
        state_handle.set_span(&params, &window)?;
    }
}
