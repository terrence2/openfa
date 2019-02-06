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
use failure::Fallible;
use log::trace;
use omnilib::OmniLib;
use pal::Palette;
use render::{ArcBallCamera, ShRenderer};
use sh::CpuShape;
use simplelog::{Config, LevelFilter, TermLogger};
use std::{sync::Arc, time::Instant};
use structopt::StructOpt;
use window::{GraphicsConfigBuilder, GraphicsWindow};
use winit::{
    DeviceEvent::{Button, Key, MouseMotion, MouseWheel},
    ElementState,
    Event::{DeviceEvent, WindowEvent},
    KeyboardInput, MouseScrollDelta, VirtualKeyCode,
    WindowEvent::{CloseRequested, Destroyed, Resized},
};

#[derive(Debug, StructOpt)]
#[structopt(name = "mm_explorer", about = "Show the contents of an mm file")]
struct Opt {
    #[structopt(
        short = "g",
        long = "game",
        default_value = "FA",
        help = "The game libraries to load."
    )]
    game: String,

    #[structopt(help = "Will load it from game, or look at last component of path")]
    input: String,
}

fn main() -> Fallible<()> {
    let opt = Opt::from_args();
    TermLogger::init(LevelFilter::Trace, Config::default())?;

    let omnilib = OmniLib::new_for_test()?;
    let lib = omnilib.library(&opt.game);

    let mut window = GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?;

    let system_palette = Arc::new(Palette::from_bytes(&lib.load("PALETTE.PAL")?)?);
    let mut sh_renderer = ShRenderer::new(system_palette.clone(), &window)?;

    let sh = CpuShape::from_data(&lib.load(&opt.input)?)?;
    sh_renderer.add_shape_to_render("foo", &sh, &lib, &window)?;

    //let model = Isometry3::new(nalgebra::zero(), nalgebra::zero());
    let mut camera = ArcBallCamera::new(window.aspect_ratio()?, 0.1f32, 3.40282347e+38f32);
    camera.set_distance(40f32);

    let mut need_reset = false;
    loop {
        let loop_start = Instant::now();

        if need_reset == true {
            need_reset = false;
            // t2_renderer.set_palette_parameters(&window, lay_base, e0_off, f1_off, c2_off, d3_off)?;
            // pal_renderer.update_pal_data(&t2_renderer.used_palette, &window)?;
        }

        sh_renderer.set_view(camera.view_matrix());
        sh_renderer.set_projection(camera.projection_matrix());

        window.drive_frame(|command_buffer, dynamic_state| {
            let cb = command_buffer;
            let cb = sh_renderer.render(cb, dynamic_state)?;
            Ok(cb)
        })?;

        let mut done = false;
        let mut resized = false;
        window.events_loop.poll_events(|ev| match ev {
            WindowEvent {
                event: CloseRequested,
                ..
            } => done = true,
            WindowEvent {
                event: Destroyed, ..
            } => done = true,
            WindowEvent {
                event: Resized(_), ..
            } => resized = true,

            // Mouse motion
            DeviceEvent {
                event: MouseMotion { delta: (x, y) },
                ..
            } => {
                camera.on_mousemove(x as f32, y as f32);
            }
            DeviceEvent {
                event:
                    MouseWheel {
                        delta: MouseScrollDelta::LineDelta(x, y),
                    },
                ..
            } => camera.on_mousescroll(x, y),
            DeviceEvent {
                event:
                    Button {
                        button: id,
                        state: ElementState::Pressed,
                    },
                ..
            } => camera.on_mousebutton_down(id),
            DeviceEvent {
                event:
                    Button {
                        button: id,
                        state: ElementState::Released,
                    },
                ..
            } => camera.on_mousebutton_up(id),

            // Keyboard Press
            DeviceEvent {
                event:
                    Key(KeyboardInput {
                        virtual_keycode: Some(keycode),
                        state: ElementState::Pressed,
                        ..
                    }),
                ..
            } => match keycode {
                VirtualKeyCode::Escape => done = true,
                VirtualKeyCode::Q => done = true,
                VirtualKeyCode::R => need_reset = true,
                _ => trace!("unknown keycode: {:?}", keycode),
            },

            _ => (),
        });
        if done {
            return Ok(());
        }
        if resized {
            window.note_resize()
        }

        let ft = loop_start.elapsed();
        let ts = format!(
            "{}.{} ms",
            ft.as_secs() * 1000 + ft.subsec_millis() as u64,
            ft.subsec_micros()
        );
        window.debug_text(10f32, 30f32, 15f32, [1f32, 1f32, 1f32, 1f32], &ts);
    }
}
