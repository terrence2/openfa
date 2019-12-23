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
use camera::{ArcBallCamera, UfoCamera};
use failure::Fallible;
use frame_graph::make_frame_graph;
use fullscreen::FullscreenBuffer;
use global_data::GlobalParametersBuffer;
use gpu::GPU;
use input::{InputBindings, InputSystem};
use lib::Library;
use log::trace;
use nalgebra::Vector3;
use screen_text::ScreenTextRenderPass;
use simplelog::{Config, LevelFilter, TermLogger};
use skybox::SkyboxRenderPass;
use stars::StarsBuffer;
use std::{f64::consts::PI, time::Instant};
use text_layout::{Font, LayoutBuffer, TextAnchorH, TextAnchorV, TextPositionH, TextPositionV};

make_frame_graph!(
    FrameGraph {
        buffers: {
            atmosphere: AtmosphereBuffer,
            fullscreen: FullscreenBuffer,
            globals: GlobalParametersBuffer,
            stars: StarsBuffer,
            text_layout: LayoutBuffer
        };
        passes: [
            skybox: SkyboxRenderPass { globals, fullscreen, stars, atmosphere },
            screen_text: ScreenTextRenderPass { globals, text_layout }
        ];
    }
);

fn main() -> Fallible<()> {
    TermLogger::init(LevelFilter::Warn, Config::default())?;

    use std::sync::Arc;
    let lib = Arc::new(Box::new(Library::empty()?));

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
    let mut input = InputSystem::new(vec![shape_bindings])?;
    let mut gpu = GPU::new(&input, Default::default())?;

    ///////////////////////////////////////////////////////////
    let atmosphere_buffer = AtmosphereBuffer::new(&mut gpu)?;
    let fullscreen_buffer = FullscreenBuffer::new(gpu.device())?;
    let globals_buffer = GlobalParametersBuffer::new(gpu.device())?;
    let stars_buffer = StarsBuffer::new(gpu.device())?;
    let text_layout_buffer = LayoutBuffer::new(&lib, &mut gpu)?;

    let frame_graph = FrameGraph::new(
        &mut gpu,
        &atmosphere_buffer,
        &fullscreen_buffer,
        &globals_buffer,
        &stars_buffer,
        &text_layout_buffer,
    )?;
    ///////////////////////////////////////////////////////////

    let fps_handle = text_layout_buffer
        .borrow_mut()
        .add_screen_text(Font::QUANTICO, "", gpu.device())?
        .with_color(&[1f32, 0f32, 0f32, 1f32])
        .with_horizontal_position(TextPositionH::Left)
        .with_horizontal_anchor(TextAnchorH::Left)
        .with_vertical_position(TextPositionV::Top)
        .with_vertical_anchor(TextAnchorV::Top);

    let mut in_sun_move = false;
    let mut sun_angle = 0.0;

    let mut camera = UfoCamera::new(f64::from(gpu.aspect_ratio()), 0.1f64, 3.4e+38f64);
    camera.set_position(6_378.0, 0.0, 0.0);
    camera.set_rotation(&Vector3::new(0.0, 0.0, 1.0), PI / 2.0);
    camera.apply_rotation(&Vector3::new(0.0, 1.0, 0.0), PI);

    // let mut camera = ArcBallCamera::new(gpu.aspect_ratio(), 0.001, 3.4e+38);
    // camera.set_target_point(&nalgebra::convert(positions[position_index]));

    loop {
        let loop_start = Instant::now();

        for command in input.poll()? {
            match command.name.as_str() {
                "window-resize" => {
                    gpu.note_resize(&input);
                    camera.set_aspect_ratio(gpu.aspect_ratio());
                }
                "window-close" | "window-destroy" | "exit" => return Ok(()),
                "+enter-move-sun" => in_sun_move = true,
                "-enter-move-sun" => in_sun_move = false,
                "mouse-move" => {
                    if in_sun_move {
                        sun_angle += command.displacement()?.0 / (180.0 * 2.0);
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

        camera.think();
        let sun_direction = Vector3::new(sun_angle.sin() as f32, 0f32, sun_angle.cos() as f32);

        let mut buffers = Vec::new();
        globals_buffer
            .borrow()
            .make_upload_buffer_for_ufo_on_globe(&camera, &gpu, &mut buffers)?;
        atmosphere_buffer
            .borrow()
            .make_upload_buffer(sun_direction, gpu.device(), &mut buffers)?;
        text_layout_buffer
            .borrow()
            .make_upload_buffer(&gpu, &mut buffers)?;
        frame_graph.run(&mut gpu, buffers)?;

        let frame_time = loop_start.elapsed();
        let ts = format!(
            "frame: {}.{}ms",
            frame_time.as_secs() * 1000 + u64::from(frame_time.subsec_millis()),
            frame_time.subsec_micros(),
        );
        fps_handle.set_span(&ts, gpu.device())?;
    }
}
