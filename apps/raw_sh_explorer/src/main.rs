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
use log::trace;
use omnilib::{make_opt_struct, OmniLib};
use pal::Palette;
use render::{DrawMode, RawShRenderer};
use sh::RawShape;
use simplelog::{Config, LevelFilter, TermLogger};
use std::{num::ParseIntError, rc::Rc, time::Instant};
use structopt::StructOpt;
use text::{Font, TextAnchorH, TextAnchorV, TextPositionH, TextPositionV, TextRenderer};
use vulkano::command_buffer::AutoCommandBufferBuilder;
use window::{GraphicsConfigBuilder, GraphicsWindow};
use winit::{
    DeviceEvent::{Button, MouseMotion},
    ElementState,
    Event::{DeviceEvent, WindowEvent},
    KeyboardInput, MouseButton, MouseScrollDelta, VirtualKeyCode,
    WindowEvent::{CloseRequested, Destroyed, MouseInput, MouseWheel, Resized},
};

fn from_number(src: &str) -> Result<usize, ParseIntError> {
    if src.starts_with("0x") {
        return usize::from_str_radix(&src[2..], 16);
    }
    usize::from_str_radix(src, 10)
}

make_opt_struct!(#[structopt(
    name = "sh_explorer",
    about = "Show the contents of a SH file"
)]
Opt {
    #[structopt(
        short = "s",
        long = "stop",
        help = "Stop at this instruction.",
        parse(try_from_str = "from_number")
    )]
    stop_at_offset => Option<usize>,

    #[structopt(short = "r", long = "range", help = "Show only this range.")]
    ranged => Option<String>
});

fn main() -> Fallible<()> {
    let opt = Opt::from_args();
    TermLogger::init(LevelFilter::Trace, Config::default())?;

    let (omni, inputs) = opt.find_inputs()?;
    if inputs.is_empty() {
        bail!("no inputs");
    }
    let (game, name) = inputs.first().unwrap();
    let lib = omni.library(&game);

    let mut window = GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?;

    let system_palette = Rc::new(Box::new(Palette::from_bytes(&lib.load("PALETTE.PAL")?)?));
    let mut text_renderer = TextRenderer::new(system_palette.clone(), &lib, &window)?;
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

    let mut sh_renderer = RawShRenderer::new(system_palette.clone(), &window)?;

    let sh = RawShape::from_bytes(&lib.load(&name)?)?;
    let mut stop_at_offset = opt.stop_at_offset.unwrap_or_else(|| sh.length());
    let mut draw_mode = DrawMode {
        range: opt.ranged.map(|s| {
            let mut parts = s.split(',');
            [
                usize::from_str_radix(parts.next().unwrap(), 16).unwrap(),
                usize::from_str_radix(parts.next().unwrap(), 16).unwrap(),
            ]
        }),
        damaged: false,
        closeness: 0x200,
        frame_number: 0,
        detail: 4,
        gear_position: Some(18),
        flaps_down: false,
        left_aileron_position: 0,
        right_aileron_position: 0,
        slats_down: false,
        airbrake_extended: true,
        hook_extended: true,
        bay_position: Some(18),
        afterburner_enabled: true,
        rudder_position: 0,
        sam_count: 0,
    };
    sh_renderer.add_shape_to_render("foo", &sh, stop_at_offset, &draw_mode, &lib, &window)?;

    let mut camera = ArcBallCamera::new(window.aspect_ratio()?, 0.1f32, 3.4e+38f32);
    camera.set_distance(40f32);

    let mut need_reset = false;
    loop {
        let loop_start = Instant::now();

        if need_reset {
            need_reset = false;
            sh_renderer.add_shape_to_render(
                "foo",
                &sh,
                stop_at_offset,
                &draw_mode,
                &lib,
                &window,
            )?;
        }

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
                                state: pressed,
                                modifiers: mod_state,
                                ..
                            },
                        ..
                    },
                ..
            } => {
                if pressed == ElementState::Pressed {
                    match keycode {
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
                        VirtualKeyCode::C => {
                            draw_mode.sam_count += 1;
                            draw_mode.sam_count %= 4;
                            need_reset = true;
                        }
                        VirtualKeyCode::G => {
                            if draw_mode.gear_position.is_some() {
                                draw_mode.gear_position = None;
                            } else {
                                draw_mode.gear_position = Some(0x0);
                            }
                            need_reset = true;
                        }
                        VirtualKeyCode::F => {
                            draw_mode.flaps_down = !draw_mode.flaps_down;
                            need_reset = true;
                        }
                        VirtualKeyCode::L => {
                            draw_mode.slats_down = !draw_mode.slats_down;
                            need_reset = true;
                        }
                        VirtualKeyCode::B => {
                            draw_mode.airbrake_extended = !draw_mode.airbrake_extended;
                            need_reset = true;
                        }
                        VirtualKeyCode::H => {
                            draw_mode.hook_extended = !draw_mode.hook_extended;
                            need_reset = true;
                        }
                        VirtualKeyCode::O => {
                            if draw_mode.bay_position.is_none() {
                                draw_mode.bay_position = Some(0x10);
                            } else {
                                draw_mode.bay_position = None;
                            }
                            need_reset = true;
                        }
                        VirtualKeyCode::A => {
                            draw_mode.left_aileron_position = 1;
                            draw_mode.right_aileron_position = -1;
                            need_reset = true;
                        }
                        VirtualKeyCode::S => {
                            draw_mode.left_aileron_position = -1;
                            draw_mode.right_aileron_position = 1;
                            need_reset = true;
                        }
                        VirtualKeyCode::Key6 => {
                            draw_mode.afterburner_enabled = !draw_mode.afterburner_enabled;
                            need_reset = true;
                        }
                        VirtualKeyCode::Z => {
                            draw_mode.rudder_position = 1;
                            need_reset = true;
                        }
                        VirtualKeyCode::X => {
                            draw_mode.rudder_position = -1;
                            need_reset = true;
                        }
                        VirtualKeyCode::Q => done = true,
                        VirtualKeyCode::R => need_reset = true,
                        _ => trace!("unknown keycode: {:?}", keycode),
                    }
                } else if pressed == ElementState::Released {
                    match keycode {
                        VirtualKeyCode::Z => {
                            draw_mode.rudder_position = 0;
                            need_reset = true;
                        }
                        VirtualKeyCode::X => {
                            draw_mode.rudder_position = 0;
                            need_reset = true;
                        }
                        VirtualKeyCode::A => {
                            draw_mode.left_aileron_position = 0;
                            draw_mode.right_aileron_position = 0;
                            need_reset = true;
                        }
                        VirtualKeyCode::S => {
                            draw_mode.left_aileron_position = 0;
                            draw_mode.right_aileron_position = 0;
                            need_reset = true;
                        }
                        _ => {}
                    }
                }
            }

            _ => (),
        });
        if done {
            return Ok(());
        }
        if resized {
            window.note_resize();
            camera.set_aspect_ratio(window.aspect_ratio()?);
        }

        {
            let frame = window.begin_frame()?;
            if !frame.is_valid() {
                continue;
            }

            text_renderer.before_frame(&window)?;
            sh_renderer.before_frame(&camera)?;

            let mut cbb = AutoCommandBufferBuilder::primary_one_time_submit(
                window.device(),
                window.queue().family(),
            )?;

            cbb = cbb.begin_render_pass(
                frame.framebuffer(&window),
                false,
                vec![[0f32, 0f32, 1f32, 1f32].into(), 0f32.into()],
            )?;

            cbb = sh_renderer.render(cbb, &window.dynamic_state)?;
            cbb = text_renderer.render(cbb, &window.dynamic_state)?;

            cbb = cbb.end_render_pass()?;

            let cb = cbb.build()?;

            frame.submit(cb, &mut window)?;
        }

        let ft = loop_start.elapsed();
        let ts = format!(
            "{}.{} ms",
            ft.as_secs() * 1000 + u64::from(ft.subsec_millis()),
            ft.subsec_micros()
        );
        fps_handle.set_span(&ts, &window)?;

        let params = format!(
            "stop:{:04X}, dam:{}, sams:{}, close:{:04X}, frame:{}, gear:{:?}, flaps:{}, brake:{}, hook:{}, bay:{:?}, aft:{}, rudder:{}",
            stop_at_offset,
            draw_mode.damaged,
            draw_mode.sam_count,
            draw_mode.closeness,
            draw_mode.frame_number,
            draw_mode.gear_position,
            draw_mode.flaps_down,
            draw_mode.airbrake_extended,
            draw_mode.hook_extended,
            draw_mode.bay_position,
            draw_mode.afterburner_enabled,
            draw_mode.rudder_position,
        );
        state_handle.set_span(&params, &window)?;
    }
}
