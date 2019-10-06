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
use failure::Fallible;
use gpu::GPU;
use input::{InputBindings, InputSystem};
use log::trace;
use nalgebra::{Unit, UnitQuaternion, Vector3};
use skybox_wgpu::SkyboxRenderer;
use std::{f64::consts::PI, time::Instant};

fn main() -> Fallible<()> {
    let mut input = InputSystem::new(vec![InputBindings::new("base")
        .bind("+enter-move-sun", "mouse1")?
        .bind("exit", "Escape")?
        .bind("exit", "q")?])?;
    let mut gpu = GPU::new(&input, Default::default())?;

    let skybox_renderer = SkyboxRenderer::new(&mut gpu)?;

    let poll_start = Instant::now();
    gpu.device().poll(true);
    println!("poll time: {:?}", poll_start.elapsed());

    let mut camera = ArcBallCamera::new(gpu.aspect_ratio(), 0.1, 3.4e+38);
    camera.set_target(6_378.2, 0.0, 0.0);
    camera.set_angle(PI / 2.0, -PI / 2.0);
    camera.set_up(-Vector3::x());
    camera.set_rotation(UnitQuaternion::from_axis_angle(
        &Unit::new_normalize(Vector3::z()),
        PI / 2.0,
    ));
    camera.set_distance(0.1);
    camera.on_mousebutton_down(1);
    let mut sun_angle = 0f64;
    let mut in_sun_move = false;

    loop {
        let frame_start = Instant::now();
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
                        camera.on_mousemove(
                            command.displacement()?.0 / 4.0,
                            command.displacement()?.1 / 4.0,
                        )
                    }
                }
                "window-cursor-move" => {}
                _ => trace!("unhandled command: {}", command.name),
            }
        }

        // Prepare new camera parameters.
        let sun_direction = Vector3::new(sun_angle.sin() as f32, 0f32, sun_angle.cos() as f32);
        let state = skybox_renderer.prepare_upload(&camera, &sun_direction, gpu.device());

        {
            let mut frame = gpu.begin_frame();
            {
                skybox_renderer.upload(&mut frame, state);

                let mut rpass = frame.begin_render_pass();
                skybox_renderer.draw(&mut rpass);
            }
            frame.finish();
        }

        println!("frame time: {:?}", frame_start.elapsed());
    }
}
