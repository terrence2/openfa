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
use dlg::Dialog;
use failure::{bail, Fallible};
use log::trace;
use omnilib::{make_opt_struct, OmniLib};
use render::DialogRenderer;
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

make_opt_struct!(#[structopt(
    name = "dlg_explorer",
    about = "Show the contents of a DLG file"
)]
Opt {
    #[structopt(short = "b", long = "background", help = "The background for this dialog")]
    background => Option<String>
});

pub fn main() -> Fallible<()> {
    let opt = Opt::from_args();
    TermLogger::init(LevelFilter::Trace, Config::default())?;

    let (omni, inputs) = opt.find_inputs()?;
    if inputs.is_empty() {
        bail!("no inputs");
    }
    let (game, name) = inputs.first().unwrap();
    let lib = omni.library(&game);

    let mut window = GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?;

    let background = if let Some(s) = opt.background {
        s
    } else {
        "CHOOSEAC.PIC".to_owned()
    };
    let dlg = Arc::new(Box::new(Dialog::from_bytes(&lib.load(name)?)?));

    ///////////////////////////////////////////////////////////
    let mut dlg_renderer = DialogRenderer::new(dlg, &background, &lib, &window)?;
    //dlg_renderer.set_palette_parameters(&window, lay_base, e0_off, f1_off, c2_off, d3_off)?;
    ///////////////////////////////////////////////////////////

    loop {
        let loop_start = Instant::now();

        dlg_renderer.set_projection(&window)?;

        window.drive_frame(|command_buffer, dynamic_state| {
            let cb = command_buffer;
            let cb = dlg_renderer.render(cb, dynamic_state)?;
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
            /*
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
            */
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
            ft.as_secs() * 1000 + u64::from(ft.subsec_millis()),
            ft.subsec_micros()
        );
        window.debug_text(10f32, 30f32, 15f32, [1f32, 1f32, 1f32, 1f32], &ts);
    }
}
