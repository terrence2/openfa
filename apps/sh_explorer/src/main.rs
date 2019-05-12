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
use failure::{bail, Fallible};
use log::trace;
use omnilib::{make_opt_struct, OmniLib};
use pal::Palette;
use render::{ArcBallCamera, ShRenderer};
use sh::RawShape;
use simplelog::{Config, LevelFilter, TermLogger};
use std::{rc::Rc, time::Instant};
use structopt::StructOpt;
use text::{Font, TextAnchorH, TextAnchorV, TextPositionH, TextPositionV, TextRenderer};
use window::{GraphicsConfigBuilder, GraphicsWindow};
use winit::{
    DeviceEvent::{Button, MouseMotion},
    ElementState,
    Event::{DeviceEvent, WindowEvent},
    KeyboardInput, MouseButton, MouseScrollDelta, VirtualKeyCode,
    WindowEvent::{CloseRequested, Destroyed, MouseInput, MouseWheel, Resized},
};

make_opt_struct!(
    #[structopt(name = "sh_explorer", about = "Show the contents of a SH file")]
    Opt {}
);

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
    let mut sh_renderer = ShRenderer::new(&window)?;
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

    let sh = RawShape::from_bytes(&lib.load(&name)?)?;
    let mut instance =
        sh_renderer.add_shape_to_render(&system_palette, name, &sh, &lib, &window)?;

    //let model = Isometry3::new(nalgebra::zero(), nalgebra::zero());
    let mut camera = ArcBallCamera::new(window.aspect_ratio()?, 0.1f32, 3.4e+38f32);
    camera.set_distance(40f32);

    loop {
        let loop_start = Instant::now();

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
                                modifiers: _mod_state,
                                ..
                            },
                        ..
                    },
                ..
            } => {
                if pressed == ElementState::Pressed {
                    match keycode {
                        VirtualKeyCode::Escape => done = true,
                        /*
                        VirtualKeyCode::LBracket => {
                            instance.frame_number = instance.frame_number.saturating_sub(1);
                        }
                        VirtualKeyCode::RBracket => {
                            instance.frame_number = instance.frame_number.saturating_add(1);
                        }
                        */
                        VirtualKeyCode::D => {
                            instance.toggle_damaged().unwrap();
                        }
                        VirtualKeyCode::G => {
                            instance.toggle_gear().unwrap();
                        }
                        VirtualKeyCode::F => {
                            instance.toggle_flaps().unwrap();
                            instance.toggle_slats().unwrap();
                        }
                        VirtualKeyCode::A => {
                            instance.move_stick_left().unwrap();
                        }
                        VirtualKeyCode::S => {
                            instance.move_stick_right().unwrap();
                        }
                        VirtualKeyCode::C => {
                            instance.bump_sam_count().unwrap();
                        }
                        VirtualKeyCode::B => {
                            instance.toggle_airbrake().unwrap();
                        }
                        VirtualKeyCode::H => {
                            instance.toggle_hook().unwrap();
                        }
                        VirtualKeyCode::O => {
                            instance.toggle_bay().unwrap();
                        }
                        VirtualKeyCode::K => {
                            instance.toggle_player_dead().unwrap();
                        }
                        VirtualKeyCode::E => {
                            instance.bump_eject_state().unwrap();
                        }
                        VirtualKeyCode::Key6 => {
                            instance.enable_afterburner().unwrap();
                        }
                        VirtualKeyCode::Key1
                        | VirtualKeyCode::Key2
                        | VirtualKeyCode::Key3
                        | VirtualKeyCode::Key4
                        | VirtualKeyCode::Key5 => {
                            instance.disable_afterburner().unwrap();
                        }
                        VirtualKeyCode::Z => {
                            instance.move_rudder_left().unwrap();
                        }
                        VirtualKeyCode::X => {
                            instance.move_rudder_right().unwrap();
                        }
                        VirtualKeyCode::Q => done = true,
                        _ => trace!("unknown keycode: {:?}", keycode),
                    }
                } else if pressed == ElementState::Released {
                    match keycode {
                        VirtualKeyCode::Z => {
                            instance.move_rudder_center().unwrap();
                        }
                        VirtualKeyCode::X => {
                            instance.move_rudder_center().unwrap();
                        }
                        VirtualKeyCode::A => {
                            instance.move_stick_center().unwrap();
                        }
                        VirtualKeyCode::S => {
                            instance.move_stick_center().unwrap();
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
        text_renderer.set_projection(&window)?;

        window.drive_frame(|command_buffer, dynamic_state| {
            let cb = command_buffer;
            let cb = sh_renderer.render(
                camera.projection_matrix(),
                &camera.view_matrix(),
                cb,
                dynamic_state,
            )?;
            let cb = text_renderer.render(cb, dynamic_state)?;
            Ok(cb)
        })?;

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
            "dam:{}, frame:{}, gear:{:?}, flaps:{}, brake:{}, hook:{}, bay:{:?}, aft:{}, rudder:{}",
            false, // instance.damaged,
            0,     // instance.frame_number,
            instance.has_gear_down()?,
            instance.has_flaps_down()?,
            instance.has_airbrake_extended()?,
            instance.has_hook_extended()?,
            instance.has_bay_open()?,
            instance.has_afterburner_enabled()?,
            instance.get_rudder_position()?,
        );
        state_handle.set_span(&params, &window)?;
    }
}
