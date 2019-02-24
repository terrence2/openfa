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
use render::{ArcBallCamera, DrawMode, ShRenderer};
use sh::CpuShape;
use simplelog::{Config, LevelFilter, TermLogger};
use std::{sync::Arc, time::Instant};
use structopt::StructOpt;
use window::{GraphicsConfigBuilder, GraphicsWindow};
use winit::{
    DeviceEvent::{Button, MouseMotion},
    ElementState,
    Event::{DeviceEvent, WindowEvent},
    KeyboardInput, MouseButton, MouseScrollDelta, VirtualKeyCode,
    WindowEvent::{CloseRequested, Destroyed, MouseInput, MouseWheel, Resized},
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

    #[structopt(
        short = "s",
        long = "stop",
        default_value = "100000",
        help = "Stop at this instruction."
    )]
    stop_at_offset: usize,

    #[structopt(
        short = "r",
        long = "range",
        help = "Show only this range."
    )]
    ranged: Option<String>,

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

    let sh = CpuShape::from_bytes(&lib.load(&opt.input)?)?;
    let mut stop_at_offset = opt.stop_at_offset;
    let mut draw_mode = DrawMode {
        range: opt.ranged.map(|s| {
            let mut parts = s.split(",");
            [usize::from_str_radix(parts.next().unwrap(), 16).unwrap(),
             usize::from_str_radix(parts.next().unwrap(), 16).unwrap()]
        }),
        damaged: false,
        closeness: 200,
        frame_number: 0,
        detail: 4,
    };
    sh_renderer.add_shape_to_render("foo", &sh, stop_at_offset, &draw_mode, &lib, &window)?;

    //let model = Isometry3::new(nalgebra::zero(), nalgebra::zero());
    let mut camera = ArcBallCamera::new(window.aspect_ratio()?, 0.1f32, 3.4e+38f32);
    camera.set_distance(40f32);

    let mut need_reset = false;
    loop {
        let loop_start = Instant::now();

        if need_reset {
            need_reset = false;
            // t2_renderer.set_palette_parameters(&window, lay_base, e0_off, f1_off, c2_off, d3_off)?;
            // pal_renderer.update_pal_data(&t2_renderer.used_palette, &window)?;
            sh_renderer.add_shape_to_render(
                "foo",
                &sh,
                stop_at_offset,
                &draw_mode,
                &lib,
                &window,
            )?;
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
            //    Use device events instead of window events for motion, so that we can move the
            //    mouse without worrying about leaving the window. Also for mouse-up, so
            //    interaction ends even if we moved off window.
            DeviceEvent {
                event: MouseMotion { delta: (x, y) },
                ..
            } => {
                camera.on_mousemove(x as f32, y as f32);
            }
            WindowEvent {
                event:
                    MouseInput {
                        button: id,
                        state: ElementState::Pressed,
                        ..
                    },
                ..
            } => {
                let id = match id {
                    MouseButton::Left => 1,
                    MouseButton::Right => 3,
                    _ => 0,
                };
                camera.on_mousebutton_down(id)
            }
            DeviceEvent {
                event:
                    Button {
                        button: id,
                        state: ElementState::Released,
                    },
                ..
            } => camera.on_mousebutton_up(id),
            WindowEvent {
                event:
                    MouseWheel {
                        delta: MouseScrollDelta::LineDelta(x, y),
                        ..
                    },
                ..
            } => camera.on_mousescroll(-x, -y),

            // Keyboard Press
            WindowEvent {
                event:
                    winit::WindowEvent::KeyboardInput {
                        input:
                            KeyboardInput {
                                virtual_keycode: Some(keycode),
                                state: ElementState::Pressed,
                                modifiers: mod_state,
                                ..
                            },
                        ..
                    },
                ..
            } => match keycode {
                VirtualKeyCode::Escape => done = true,
                VirtualKeyCode::Right => {
                    stop_at_offset = stop_at_offset.saturating_add(1);
                    need_reset = true;
                }
                VirtualKeyCode::Left => {
                    stop_at_offset = stop_at_offset.saturating_sub(1);
                    need_reset = true;
                }
                VirtualKeyCode::Up => {
                    stop_at_offset = stop_at_offset.saturating_add(0x10);
                    need_reset = true;
                }
                VirtualKeyCode::Down => {
                    stop_at_offset = stop_at_offset.saturating_sub(0x10);
                    need_reset = true;
                }
                VirtualKeyCode::PageUp => {
                    stop_at_offset = stop_at_offset.saturating_add(0x100);
                    need_reset = true;
                }
                VirtualKeyCode::PageDown => {
                    stop_at_offset = stop_at_offset.saturating_sub(0x100);
                    need_reset = true;
                }
                VirtualKeyCode::End => {
                    stop_at_offset = 100_000;
                    need_reset = true;
                }
                VirtualKeyCode::Home => {
                    stop_at_offset = 0;
                    need_reset = true;
                }
                VirtualKeyCode::Period => {
                    if mod_state.ctrl {
                        draw_mode.closeness = draw_mode.closeness.saturating_add(0x10);
                    } else {
                        draw_mode.closeness = draw_mode.closeness.saturating_add(0x1);
                    }
                    need_reset = true;
                }
                VirtualKeyCode::Comma => {
                    if mod_state.ctrl {
                        draw_mode.closeness = draw_mode.closeness.saturating_sub(0x10);
                    } else {
                        draw_mode.closeness = draw_mode.closeness.saturating_sub(0x1);
                    }
                    need_reset = true;
                }
                VirtualKeyCode::LBracket => {
                    draw_mode.frame_number = draw_mode.frame_number.saturating_sub(1);
                    need_reset = true;
                }
                VirtualKeyCode::RBracket => {
                    draw_mode.frame_number = draw_mode.frame_number.saturating_add(1);
                    need_reset = true;
                }
                VirtualKeyCode::D => {
                    draw_mode.damaged = !draw_mode.damaged;
                    need_reset = true;
                }
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
            window.note_resize();
            camera.set_aspect_ratio(window.aspect_ratio()?);
        }

        let ft = loop_start.elapsed();
        let ts = format!(
            "{}.{} ms",
            ft.as_secs() * 1000 + u64::from(ft.subsec_millis()),
            ft.subsec_micros()
        );
        window.debug_text(10f32, 30f32, 15f32, [1f32, 1f32, 1f32, 1f32], &ts);

        let params = format!(
            "stop:{:04X}, dam:{}, close:{:04X}, frame:{}",
            stop_at_offset, draw_mode.damaged, draw_mode.closeness, draw_mode.frame_number
        );
        window.debug_text(600f32, 30f32, 18f32, [1f32, 1f32, 1f32, 1f32], &params);
    }
}
