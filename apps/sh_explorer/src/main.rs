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
use sh::RawShape;
use shape::ShRenderer;
use simplelog::{Config, LevelFilter, TermLogger};
use std::{cell::RefCell, rc::Rc, sync::Arc, time::Instant};
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
    TermLogger::init(LevelFilter::Debug, Config::default())?;

    let (omni, inputs) = opt.find_inputs()?;
    if inputs.is_empty() {
        bail!("no inputs");
    }
    let (game, name) = inputs.first().unwrap();
    let lib = omni.library(&game);
    let system_palette = Rc::new(Box::new(Palette::from_bytes(&lib.load("PALETTE.PAL")?)?));

    let mut window = GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?;

    let sh_renderer = Arc::new(RefCell::new(ShRenderer::new(&window)?));
    let text_renderer = Arc::new(RefCell::new(TextRenderer::new(
        system_palette.clone(),
        &lib,
        &window,
    )?));

    window.add_render_subsystem(sh_renderer.clone());
    window.add_render_subsystem(text_renderer.clone());

    let fps_handle = text_renderer
        .borrow_mut()
        .add_screen_text(Font::HUD11, "", &window)?
        .with_color(&[1f32, 0f32, 0f32, 1f32])
        .with_horizontal_position(TextPositionH::Left)
        .with_horizontal_anchor(TextAnchorH::Left)
        .with_vertical_position(TextPositionV::Top)
        .with_vertical_anchor(TextAnchorV::Top);
    let state_handle = text_renderer
        .borrow_mut()
        .add_screen_text(Font::HUD11, "", &window)?
        .with_color(&[1f32, 0.5f32, 0f32, 1f32])
        .with_horizontal_position(TextPositionH::Right)
        .with_horizontal_anchor(TextAnchorH::Right)
        .with_vertical_position(TextPositionV::Bottom)
        .with_vertical_anchor(TextAnchorV::Bottom);

    let sh = RawShape::from_bytes(&lib.load(&name)?)?;
    let instance =
        sh_renderer
            .borrow_mut()
            .add_shape_to_render(&system_palette, name, &sh, &lib, &window)?;

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
                        VirtualKeyCode::Delete => {
                            instance.draw_state().borrow_mut().toggle_damaged();
                        }
                        VirtualKeyCode::PageUp => {
                            instance.draw_state().borrow_mut().consume_sam();
                        }
                        VirtualKeyCode::G => {
                            instance.draw_state().borrow_mut().toggle_gear(&loop_start);
                        }
                        VirtualKeyCode::F => {
                            instance.draw_state().borrow_mut().toggle_flaps();
                            instance.draw_state().borrow_mut().toggle_slats();
                        }
                        VirtualKeyCode::A => {
                            instance.draw_state().borrow_mut().move_stick_left();
                        }
                        VirtualKeyCode::D => {
                            instance.draw_state().borrow_mut().move_stick_right();
                        }
                        VirtualKeyCode::W => {
                            if mod_state.shift {
                                instance.draw_state().borrow_mut().vector_thrust_forward();
                            } else {
                                instance.draw_state().borrow_mut().move_stick_forward();
                            }
                        }
                        VirtualKeyCode::S => {
                            if mod_state.shift {
                                instance.draw_state().borrow_mut().vector_thrust_backward();
                            } else {
                                instance.draw_state().borrow_mut().move_stick_backward();
                            }
                        }
                        VirtualKeyCode::B => {
                            instance.draw_state().borrow_mut().toggle_airbrake();
                        }
                        VirtualKeyCode::H => {
                            instance.draw_state().borrow_mut().toggle_hook();
                        }
                        VirtualKeyCode::O => {
                            instance.draw_state().borrow_mut().toggle_bay(&loop_start);
                        }
                        VirtualKeyCode::K => {
                            instance.draw_state().borrow_mut().toggle_player_dead();
                        }
                        VirtualKeyCode::E => {
                            instance.draw_state().borrow_mut().bump_eject_state();
                        }
                        VirtualKeyCode::Key6 => {
                            instance.draw_state().borrow_mut().enable_afterburner();
                            instance.draw_state().borrow_mut().increase_wing_sweep();
                        }
                        VirtualKeyCode::Key1 => {
                            instance.draw_state().borrow_mut().disable_afterburner();
                            instance.draw_state().borrow_mut().decrease_wing_sweep();
                        }
                        VirtualKeyCode::Key2
                        | VirtualKeyCode::Key3
                        | VirtualKeyCode::Key4
                        | VirtualKeyCode::Key5 => {
                            instance.draw_state().borrow_mut().disable_afterburner();
                        }
                        VirtualKeyCode::Z => {
                            instance.draw_state().borrow_mut().move_rudder_left();
                        }
                        VirtualKeyCode::C => {
                            instance.draw_state().borrow_mut().move_rudder_right();
                        }
                        VirtualKeyCode::Q => done = true,
                        _ => trace!("unknown keycode: {:?}", keycode),
                    }
                } else if pressed == ElementState::Released {
                    match keycode {
                        VirtualKeyCode::Z => {
                            instance.draw_state().borrow_mut().move_rudder_center();
                        }
                        VirtualKeyCode::X => {
                            instance.draw_state().borrow_mut().move_rudder_center();
                        }
                        VirtualKeyCode::A => {
                            instance.draw_state().borrow_mut().move_stick_center();
                        }
                        VirtualKeyCode::S => {
                            instance.draw_state().borrow_mut().move_stick_center();
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

        sh_renderer.borrow_mut().animate(&loop_start)?;
        window.drive_frame(&camera)?;

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
            "dam:{}, gear:{}/{:.1}, flaps:{}, brake:{}, hook:{}, bay:{}/{:.1}, aft:{}, swp:{}",
            !instance.draw_state().borrow().show_damaged(),
            !instance.draw_state().borrow().gear_retracted(),
            instance.draw_state().borrow().gear_position(),
            instance.draw_state().borrow().flaps_down(),
            instance.draw_state().borrow().airbrake_extended(),
            instance.draw_state().borrow().hook_extended(),
            !instance.draw_state().borrow().bay_closed(),
            instance.draw_state().borrow().bay_position(),
            instance.draw_state().borrow().afterburner_enabled(),
            instance.draw_state().borrow().wing_sweep_angle(),
        );
        state_handle.set_span(&params, &window)?;
    }
}
