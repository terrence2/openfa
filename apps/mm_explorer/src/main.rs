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
use asset::AssetManager;
use failure::{bail, Fallible};
use log::trace;
use mm::MissionMap;
use nalgebra::Isometry3;
use omnilib::{make_opt_struct, OmniLib};
use render::{ArcBallCamera, PalRenderer, T2Renderer};
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
use xt::TypeManager;

make_opt_struct!(#[structopt(
    name = "mm_explorer",
    about = "Show the contents of an MM file"
)]
Opt {});

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

    let assets = Arc::new(Box::new(AssetManager::new(lib.clone())?));
    let types = TypeManager::new(lib.clone());

    let contents = lib.load_text(&name)?;
    let mm = MissionMap::from_str(&contents, &types)?;

    ///////////////////////////////////////////////////////////
    let mut t2_renderer = T2Renderer::new(mm, &assets, &lib, &window)?;
    let mut lay_base = -3;
    let mut e0_off = -1;
    let mut f1_off = -1;
    let mut c2_off = 0;
    let mut d3_off = 0;
    t2_renderer.set_palette_parameters(&window, lay_base, e0_off, f1_off, c2_off, d3_off)?;
    let mut pal_renderer = PalRenderer::new(&window)?;
    pal_renderer.update_pal_data(&t2_renderer.used_palette, &window)?;
    ///////////////////////////////////////////////////////////

    let model = Isometry3::new(nalgebra::zero(), nalgebra::zero());
    let mut camera = ArcBallCamera::new(window.aspect_ratio()?, 0.001f32, 3.4e+38f32);

    let mut need_reset = false;
    loop {
        let loop_start = Instant::now();

        if need_reset {
            need_reset = false;
            t2_renderer
                .set_palette_parameters(&window, lay_base, e0_off, f1_off, c2_off, d3_off)?;
            pal_renderer.update_pal_data(&t2_renderer.used_palette, &window)?;
        }

        t2_renderer.set_projection(camera.projection_for(model));

        window.drive_frame(|command_buffer, dynamic_state| {
            let cb = command_buffer;
            let cb = t2_renderer.render(cb, dynamic_state)?;
            let cb = pal_renderer.render(cb, dynamic_state)?;
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
                VirtualKeyCode::T => lay_base += 1,
                VirtualKeyCode::G => lay_base -= 1,
                VirtualKeyCode::Y => c2_off += 1,
                VirtualKeyCode::H => c2_off -= 1,
                VirtualKeyCode::U => d3_off += 1,
                VirtualKeyCode::J => d3_off -= 1,
                VirtualKeyCode::I => e0_off += 1,
                VirtualKeyCode::K => e0_off -= 1,
                VirtualKeyCode::O => f1_off += 1,
                VirtualKeyCode::L => f1_off -= 1,
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

        let offsets = format!(
            "base: lay:{} c2:{} d3:{} e0:{} f1:{}",
            lay_base, c2_off, d3_off, e0_off, f1_off
        );
        window.debug_text(
            1800f32,
            25f32,
            30f32,
            [1f32, 0.5f32, 0.5f32, 1f32],
            &offsets,
        );

        let ft = loop_start.elapsed();
        let ts = format!(
            "{}.{} ms",
            ft.as_secs() * 1000 + u64::from(ft.subsec_millis()),
            ft.subsec_micros()
        );
        window.debug_text(10f32, 30f32, 15f32, [1f32, 1f32, 1f32, 1f32], &ts);
    }
}
