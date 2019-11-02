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
use chrono::{prelude::*, Duration};
use failure::Fallible;
use input::{InputBindings, InputSystem};
use lib::Library;
use log::trace;
use nalgebra::{convert, Vector3};
use orrery::Orrery;
use simplelog::{Config, LevelFilter, TermLogger};
use skybox::SkyboxRenderer;
use std::{f64::consts::PI, time::Instant};
use text::{Font, TextAnchorH, TextAnchorV, TextPositionH, TextPositionV, TextRenderer};
use vulkano::command_buffer::AutoCommandBufferBuilder;
use window::{GraphicsConfigBuilder, GraphicsWindow};

fn main() -> Fallible<()> {
    TermLogger::init(LevelFilter::Trace, Config::default())?;

    use std::sync::Arc;
    let lib = Arc::new(Box::new(Library::empty()?));

    let mut window = GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?;
    let shape_bindings = InputBindings::new("shape")
        .bind("+enter-move-sun", "mouse1")?
        .bind("zoom-in", "Equals")?
        .bind("zoom-out", "Subtract")?
        .bind("+rotate-right", "c")?
        .bind("+rotate-left", "z")?
        .bind("+move-left", "a")?
        .bind("+move-right", "d")?
        .bind("+move-forward", "w")?
        .bind("+move-backward", "s")?
        .bind("+move-up", "space")?
        .bind("+move-down", "Control")?
        .bind("exit", "Escape")?
        .bind("exit", "q")?;
    let mut input = InputSystem::new(vec![shape_bindings]);

    let mut text_renderer = TextRenderer::new(&lib, &window)?;
    let mut skybox_renderer = SkyboxRenderer::new(&window)?;

    let fps_handle = text_renderer
        .add_screen_text(Font::QUANTICO, "", &window)?
        .with_color(&[1f32, 0f32, 0f32, 1f32])
        .with_horizontal_position(TextPositionH::Left)
        .with_horizontal_anchor(TextAnchorH::Left)
        .with_vertical_position(TextPositionV::Top)
        .with_vertical_anchor(TextAnchorV::Top);

    let mut orrery = Orrery::new();

    let mut camera = UfoCamera::new(f64::from(window.aspect_ratio()?), 0.1f64, 3.4e+38f64);
    camera.set_position(6_378_001.0, 0.0, 0.0);
    camera.set_rotation(&Vector3::new(0.0, 0.0, 1.0), PI / 2.0);
    camera.apply_rotation(&Vector3::new(0.0, 1.0, 0.0), PI);

    let mut in_sun_move = false;
    let mut sim_time = Utc.ymd(2000, 1, 1).and_hms_milli(12, 0, 0, 0);

    loop {
        let loop_start = Instant::now();

        for command in input.poll(&mut window.events_loop) {
            match command.name.as_str() {
                "window-resize" => {
                    window.note_resize();
                    camera.set_aspect_ratio(f64::from(window.aspect_ratio()?));
                }
                "window-close" | "window-destroy" | "exit" => return Ok(()),
                "+enter-move-sun" => in_sun_move = true,
                "-enter-move-sun" => in_sun_move = false,
                "mouse-move" => {
                    if in_sun_move {
                        //sun_angle += command.displacement()?.0 / (180.0 * 2.0);
                        let days = command.displacement()?.0 as i64;
                        println!("ADDING DAYS: {}", days);
                        sim_time = sim_time.checked_add_signed(Duration::days(days)).unwrap();
                    } else {
                        camera.on_mousemove(command.displacement()?.0, command.displacement()?.1)
                    }
                }
                "mouse-wheel" => {
                    if command.displacement()?.1 > 0.0 {
                        camera.speed *= 0.8;
                    } else {
                        camera.speed *= 1.2;
                    }
                }
                "zoom-in" => camera.zoom_in(),
                "zoom-out" => camera.zoom_out(),
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
                "window-cursor-move" => {}
                _ => trace!("unhandled command: {}", command.name),
            }
        }

        //sh_renderer.animate(&loop_start)?;
        camera.think();

        {
            let frame = window.begin_frame()?;
            if !frame.is_valid() {
                continue;
            }

            //let sun_direction = Vector3::new(sun_angle.sin() as f32, 0f32, sun_angle.cos() as f32);
            let sun_direction = convert(orrery.sun_position_at(sim_time).coords.normalize());
            println!("SUN DIRECTION: {:?}", sun_direction);

            skybox_renderer.before_frame(&camera, &sun_direction)?;
            text_renderer.before_frame(&window)?;

            let mut cbb = AutoCommandBufferBuilder::primary_one_time_submit(
                window.device(),
                window.queue().family(),
            )?;

            cbb = cbb.begin_render_pass(
                frame.framebuffer(&window),
                false,
                vec![[0f32, 0f32, 1f32, 1f32].into(), 0f32.into()],
            )?;

            cbb = skybox_renderer.draw(cbb, &window.dynamic_state)?;
            cbb = text_renderer.render(cbb, &window.dynamic_state)?;

            cbb = cbb.end_render_pass()?;

            let cb = cbb.build()?;

            frame.submit(cb, &mut window)?;
        }

        let frame_time = loop_start.elapsed();
        let render_time = frame_time - window.idle_time;
        let ts = format!(
            "Date: {:?} || frame: {}.{}ms / render: {}.{}ms",
            sim_time,
            frame_time.as_secs() * 1000 + u64::from(frame_time.subsec_millis()),
            frame_time.subsec_micros(),
            render_time.as_secs() * 1000 + u64::from(render_time.subsec_millis()),
            render_time.subsec_micros(),
        );
        fps_handle.set_span(&ts, &window)?;
    }
}
