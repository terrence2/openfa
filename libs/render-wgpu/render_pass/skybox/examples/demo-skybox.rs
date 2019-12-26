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
use absolute_unit::meters;
use atmosphere::AtmosphereBuffer;
use camera::ArcBallCamera;
use failure::Fallible;
use fullscreen::FullscreenBuffer;
use global_data::GlobalParametersBuffer;
use gpu::GPU;
use input::{InputBindings, InputSystem};
use nalgebra::Vector3;
use skybox::SkyboxRenderPass;
use stars::StarsBuffer;
use std::time::Instant;

fn main() -> Fallible<()> {
    let system_bindings = InputBindings::new("system")
        .bind("exit", "Escape")?
        .bind("exit", "q")?;
    let mut input = InputSystem::new(vec![ArcBallCamera::default_bindings()?, system_bindings])?;
    let mut gpu = GPU::new(&input, Default::default())?;

    let globals_buffer = GlobalParametersBuffer::new(gpu.device())?;
    let fullscreen_buffer = FullscreenBuffer::new(gpu.device())?;
    let atmosphere_buffer = AtmosphereBuffer::new(&mut gpu)?;
    let stars_buffer = StarsBuffer::new(gpu.device())?;
    let skybox_renderer = SkyboxRenderPass::new(
        &mut gpu,
        &globals_buffer.borrow(),
        &fullscreen_buffer.borrow(),
        &stars_buffer.borrow(),
        &atmosphere_buffer.borrow(),
    )?;

    let poll_start = Instant::now();
    gpu.device().poll(true);
    println!("poll time: {:?}", poll_start.elapsed());

    let mut camera = ArcBallCamera::new(gpu.aspect_ratio(), meters!(0.1), meters!(3.4e+38));
    camera.set_target(0.0, -10.0, 0.0);

    loop {
        let frame_start = Instant::now();
        for command in input.poll()? {
            camera.handle_command(&command)?;
            match command.name.as_str() {
                "window-close" | "window-destroy" | "exit" => return Ok(()),
                "window-resize" => {
                    gpu.note_resize(&input);
                    camera.set_aspect_ratio(gpu.aspect_ratio());
                }
                "window-cursor-move" => {}
                _ => {}
            }
        }

        // Prepare new camera parameters.
        let sun_direction = Vector3::new(
            camera.sun_angle.sin() as f32,
            0f32,
            camera.sun_angle.cos() as f32,
        );

        let mut upload_buffers = Vec::new();
        globals_buffer
            .borrow()
            .make_upload_buffer_for_arcball_on_globe(&camera, &gpu, &mut upload_buffers)?;
        atmosphere_buffer.borrow().make_upload_buffer(
            sun_direction,
            gpu.device(),
            &mut upload_buffers,
        )?;

        {
            let mut frame = gpu.begin_frame()?;
            {
                for desc in upload_buffers.drain(..) {
                    frame.copy_buffer_to_buffer(
                        &desc.source,
                        desc.source_offset,
                        &desc.destination,
                        desc.destination_offset,
                        desc.copy_size,
                    );
                }

                let mut rpass = frame.begin_render_pass();
                skybox_renderer.draw(
                    &mut rpass,
                    &globals_buffer.borrow(),
                    &fullscreen_buffer.borrow(),
                    &stars_buffer.borrow(),
                    &atmosphere_buffer.borrow(),
                );
            }
            frame.finish();
        }

        println!("frame time: {:?}", frame_start.elapsed());
    }
}
