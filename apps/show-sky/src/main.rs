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
use camera::UfoCamera;
use failure::{bail, Fallible};
use input::{InputBindings, InputSystem};
use log::trace;
use omnilib::{make_opt_struct, OmniLib};
use pal::Palette;
use sh::RawShape;
use shape::{DrawSelection, ShRenderer};
use simplelog::{Config, LevelFilter, TermLogger};
use sky::SkyRenderer;
use starbox::StarboxRenderer;
use std::{rc::Rc, time::Instant};
use structopt::StructOpt;
use subocean::SubOceanRenderer;
use text::{Font, TextAnchorH, TextAnchorV, TextPositionH, TextPositionV, TextRenderer};
use vulkano::command_buffer::AutoCommandBufferBuilder;
use window::{GraphicsConfigBuilder, GraphicsWindow};

make_opt_struct!(
    #[structopt(name = "sh_explorer", about = "Show the contents of a SH file")]
    Opt {}
);

fn main() -> Fallible<()> {
    let opt = Opt::from_args();
    TermLogger::init(LevelFilter::Debug, Config::default())?;

    let (omni, inputs) = opt.find_inputs()?;
    if inputs.is_empty() {
        bail!("no inputs");
    }
    let (game, name) = inputs.first().unwrap();
    let lib = omni.library(&game);
    let system_palette = Rc::new(Box::new(Palette::from_bytes(&lib.load("PALETTE.PAL")?)?));

    let mut window = GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?;
    let shape_bindings = InputBindings::new("shape")
        .bind("+rotate-right", "c")?
        .bind("+rotate-left", "z")?
        .bind("+move-left", "a")?
        .bind("+move-right", "d")?
        .bind("+move-forward", "w")?
        .bind("+move-backward", "s")?
        .bind("+move-up", "space")?
        .bind("+move-down", "Control")?
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

    let mut sh_renderer = ShRenderer::new(&window)?;
    let mut text_renderer = TextRenderer::new(system_palette.clone(), &lib, &window)?;
    let mut starbox_renderer = StarboxRenderer::new(&window)?;
    let mut sky_renderer = SkyRenderer::new(&window)?;
    let mut subocean_renderer = SubOceanRenderer::new(&window)?;

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

    let sh = RawShape::from_bytes(&lib.load(&name)?)?;
    let instance = sh_renderer.add_shape_to_render(
        name,
        &sh,
        DrawSelection::NormalModel,
        &system_palette,
        &lib,
        &window,
    )?;

    let mut camera = UfoCamera::new(window.aspect_ratio()? as f64, 0.1f64, 3.4e+38f64);
    //camera.set_position(6_378.0001, 0.0, 0.0);

    loop {
        let loop_start = Instant::now();

        for command in input.poll(&mut window.events_loop) {
            match command.name.as_str() {
                "window-resize" => {
                    window.note_resize();
                    camera.set_aspect_ratio(window.aspect_ratio()? as f64);
                }
                "window-close" | "window-destroy" | "exit" => return Ok(()),
                "mouse-move" => {
                    camera.on_mousemove(command.displacement()?.0, command.displacement()?.1)
                }
                "mouse-wheel" => {
                    if command.displacement()?.1 > 0.0 {
                        camera.speed *= 0.8;
                    } else {
                        camera.speed *= 1.2;
                    }
                }
                "+rotate-right" => camera.plus_rotate_right(),
                "-rotate-right" => camera.minus_rotate_right(),
                "+rotate-left" => camera.plus_rotate_left(),
                "-rotate-left" => camera.minus_rotate_left(),
                "+move-up" => camera.plus_move_up(),
                "-move-up" => camera.minus_move_up(),
                "+move-down" => camera.plus_move_down(),
                "-move-down" => camera.minus_move_down(),
                "+move-left" => camera.plus_move_left(),
                "-move-left" => camera.minus_move_left(),
                "+move-right" => camera.plus_move_right(),
                "-move-right" => camera.minus_move_right(),
                "+move-backward" => camera.plus_move_backward(),
                "-move-backward" => camera.minus_move_backward(),
                "+move-forward" => camera.plus_move_forward(),
                "-move-forward" => camera.minus_move_forward(),
                "+rudder-left" => instance.draw_state().borrow_mut().move_rudder_left(),
                "-rudder-left" => instance.draw_state().borrow_mut().move_rudder_center(),
                "+rudder-right" => instance.draw_state().borrow_mut().move_rudder_right(),
                "-rudder-right" => instance.draw_state().borrow_mut().move_rudder_center(),
                "+stick-backward" => instance.draw_state().borrow_mut().move_stick_backward(),
                "-stick-backward" => instance.draw_state().borrow_mut().move_stick_center(),
                "+stick-forward" => instance.draw_state().borrow_mut().move_stick_forward(),
                "-stick-forward" => instance.draw_state().borrow_mut().move_stick_center(),
                "+stick-left" => instance.draw_state().borrow_mut().move_stick_left(),
                "-stick-left" => instance.draw_state().borrow_mut().move_stick_center(),
                "+stick-right" => instance.draw_state().borrow_mut().move_stick_right(),
                "-stick-right" => instance.draw_state().borrow_mut().move_stick_center(),
                "+vector-thrust-backward" => {
                    instance.draw_state().borrow_mut().vector_thrust_backward()
                }
                "+vector-thrust-forward" => {
                    instance.draw_state().borrow_mut().vector_thrust_forward()
                }
                "-vector-thrust-forward" => instance.draw_state().borrow_mut().vector_thrust_stop(),
                "-vector-thrust-backward" => {
                    instance.draw_state().borrow_mut().vector_thrust_stop()
                }
                "bump-eject-state" => instance.draw_state().borrow_mut().bump_eject_state(),
                "consume-sam" => instance.draw_state().borrow_mut().consume_sam(),
                "+decrease-wing-sweep" => instance.draw_state().borrow_mut().decrease_wing_sweep(),
                "+increase-wing-sweep" => instance.draw_state().borrow_mut().increase_wing_sweep(),
                "-decrease-wing-sweep" => instance.draw_state().borrow_mut().stop_wing_sweep(),
                "-increase-wing-sweep" => instance.draw_state().borrow_mut().stop_wing_sweep(),
                "disable-afterburner" => instance.draw_state().borrow_mut().disable_afterburner(),
                "enable-afterburner" => instance.draw_state().borrow_mut().enable_afterburner(),
                "toggle-airbrake" => instance.draw_state().borrow_mut().toggle_airbrake(),
                "toggle-bay" => instance.draw_state().borrow_mut().toggle_bay(&loop_start),
                "toggle-flaps" => {
                    instance.draw_state().borrow_mut().toggle_flaps();
                    instance.draw_state().borrow_mut().toggle_slats();
                }
                "toggle-gear" => instance.draw_state().borrow_mut().toggle_gear(&loop_start),
                "toggle-hook" => instance.draw_state().borrow_mut().toggle_hook(),
                "toggle-player-dead" => instance.draw_state().borrow_mut().toggle_player_dead(),
                "window-cursor-move" => {}
                _ => trace!("unhandled command: {}", command.name),
            }
        }

        sh_renderer.animate(&loop_start)?;
        camera.think();

        {
            let frame = window.begin_frame()?;
            if !frame.is_valid() {
                continue;
            }

            subocean_renderer.before_frame(&camera)?;
            starbox_renderer.before_frame(&camera)?;
            sky_renderer.before_frame(&camera)?;
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

            cbb = starbox_renderer.render(cbb, &window.dynamic_state)?;
            //cbb = sky_renderer.render(cbb, &window.dynamic_state)?;
            cbb = sh_renderer.render(&camera, cbb, &window.dynamic_state)?;
            cbb = subocean_renderer.render(cbb, &window.dynamic_state)?;
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
            "speed: {:.3}, gear:{}/{:.1}, flaps:{}, brake:{}, hook:{}, bay:{}/{:.1}, aft:{}, swp:{}",
            camera.speed,
            !instance.draw_state().borrow().gear_retracted(),
            instance.draw_state().borrow().gear_position(),
            instance.draw_state().borrow().flaps_down(),
            instance.draw_state().borrow().airbrake_extended(),
            instance.draw_state().borrow().hook_extended(),
            !instance.draw_state().borrow().bay_closed(),
            instance.draw_state().borrow().bay_position(),
            instance.draw_state().borrow().afterburner_enabled(),
            instance.draw_state().borrow().wing_sweep_angle(),
        );
        state_handle.set_span(&params, &window)?;
    }
}
