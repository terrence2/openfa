// This file is part of Nitrogen.
//
// Nitrogen is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Nitrogen is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with Nitrogen.  If not, see <http://www.gnu.org/licenses/>.
use command::{Bindings, Command, Key};
use failure::{bail, Fallible};
use log::warn;
use smallvec::{smallvec, SmallVec};
use std::{
    collections::HashMap,
    sync::mpsc::{channel, Receiver, TryRecvError},
    thread,
};
use winit::{
    event::{
        DeviceEvent, DeviceId, ElementState, Event, KeyboardInput, MouseScrollDelta, StartCause,
        WindowEvent,
    },
    event_loop::{ControlFlow, EventLoop, EventLoopProxy},
    platform::desktop::EventLoopExtDesktop,
    window::Window,
};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum MetaEvent {
    Stop,
}

pub struct InputSystem {
    // Prioritized list of input binding sets. The last set with a matching
    // input binding "wins" and determines the command that is sent for that
    // input event.
    bindings: Vec<Bindings>,

    // Track key states so that we can match button combos.
    button_state: HashMap<Key, ElementState>,

    event_receiver: Receiver<Event<MetaEvent>>,

    event_thread: Option<thread::JoinHandle<()>>,
    event_loop_proxy: EventLoopProxy<MetaEvent>,
    window: Window,
}

impl InputSystem {
    pub fn new(bindings: Vec<Bindings>) -> Fallible<Self> {
        let (tx_event, rx_event) = channel();
        let (tx_window, rx_window) = channel();
        let (tx_proxy, rx_proxy) = channel();
        let event_thread = thread::spawn(move || {
            let mut event_loop = EventLoop::<MetaEvent>::with_user_event();
            tx_proxy
                .send(event_loop.create_proxy())
                .expect("unable to return event proxy");
            let window = Window::new(&event_loop).expect("unable to create window");
            tx_window.send(window).expect("unable to return window");

            event_loop.run_return(move |event, _target, control_flow| {
                *control_flow = ControlFlow::Wait;
                if event == Event::UserEvent(MetaEvent::Stop) {
                    *control_flow = ControlFlow::Exit;
                    return;
                }
                tx_event.send(event).expect("send okay");
            });
        });
        let window = rx_window.recv()?;
        let event_loop_proxy = rx_proxy.recv()?;
        Ok(Self {
            bindings,
            button_state: HashMap::new(),
            event_receiver: rx_event,
            event_thread: Some(event_thread),
            event_loop_proxy,
            window,
        })
    }

    pub fn push_bindings(&mut self, bindings: Bindings) {
        self.bindings.push(bindings);
    }

    pub fn pop_bindings(&mut self) -> Option<Bindings> {
        self.bindings.pop()
    }

    pub fn poll(&mut self) -> Fallible<SmallVec<[Command; 8]>> {
        let mut out = SmallVec::new();
        let mut evt = self.event_receiver.try_recv();
        while evt.is_ok() {
            out.extend(self.handle_event(evt?));
            evt = self.event_receiver.try_recv();
        }
        match evt.err().unwrap() {
            TryRecvError::Empty => Ok(out),
            TryRecvError::Disconnected => bail!("input system stopped"),
        }
    }

    pub fn handle_event(&mut self, e: Event<MetaEvent>) -> SmallVec<[Command; 8]> {
        match e {
            Event::WindowEvent { event, .. } => self.handle_window_event(event),
            Event::DeviceEvent { device_id, event } => self.handle_device_event(device_id, event),
            Event::EventsCleared => smallvec![],
            Event::NewEvents(StartCause::WaitCancelled { .. }) => smallvec![],
            unhandled => {
                warn!("don't know how to handle: {:?}", unhandled);
                smallvec![]
            }
        }
    }

    pub fn window(&self) -> &Window {
        &self.window
    }

    fn handle_window_event(&self, event: WindowEvent) -> SmallVec<[Command; 8]> {
        match event {
            // System Stuff
            WindowEvent::Resized(s) => smallvec![Command::with_arg("window-resize", s.into())],
            WindowEvent::Moved(p) => smallvec![Command::with_arg("window-move", p.into())],
            WindowEvent::Destroyed => smallvec![Command::new("window-destroy")],
            WindowEvent::CloseRequested => smallvec![Command::new("window-close")],
            WindowEvent::Focused(b) => smallvec![Command::with_arg("window-focus", b.into())],
            WindowEvent::DroppedFile(p) => {
                smallvec![Command::with_arg("window-file-drop", p.into())]
            }
            WindowEvent::HoveredFile(p) => {
                smallvec![Command::with_arg("window-file-hover", p.into())]
            }
            WindowEvent::HoveredFileCancelled => {
                smallvec![Command::new("window-file-hover-cancel")]
            }
            WindowEvent::HiDpiFactorChanged(f) => {
                smallvec![Command::with_arg("window-dpi-change", f.into())]
            }
            WindowEvent::CursorEntered { device_id } => {
                smallvec![Command::with_arg("window-cursor-entered", device_id.into())]
            }
            WindowEvent::CursorLeft { device_id } => {
                smallvec![Command::with_arg("window-cursor-left", device_id.into())]
            }

            // Track real cursor position in the window including window system accel
            // warping, and other such; mostly useful for software mice, but also for
            // picking with a hardware mouse.
            WindowEvent::CursorMoved { position, .. } => {
                smallvec![Command::with_arg("window-cursor-move", position.into())]
            }

            // Ignore events duplicated by device capture.
            WindowEvent::ReceivedCharacter { .. } => smallvec![],
            WindowEvent::KeyboardInput { .. } => smallvec![],
            WindowEvent::MouseInput { .. } => smallvec![],
            WindowEvent::MouseWheel { .. } => smallvec![],

            // Ignore events we don't get on the device.
            WindowEvent::Touch(_) => smallvec![],
            WindowEvent::TouchpadPressure { .. } => smallvec![],
            WindowEvent::AxisMotion { .. } => smallvec![],

            // We should not need invalidation given our game loop.
            WindowEvent::RedrawRequested => smallvec![],

            WindowEvent::ModifiersChanged { .. } => smallvec![],
        }
    }

    fn handle_device_event(
        &mut self,
        device_id: DeviceId,
        event: DeviceEvent,
    ) -> SmallVec<[Command; 8]> {
        match event {
            // Device change events
            DeviceEvent::Added => smallvec![Command::with_arg("device-added", device_id.into())],
            DeviceEvent::Removed => {
                smallvec![Command::with_arg("device-removed", device_id.into())]
            }

            // Mouse Motion
            DeviceEvent::MouseMotion { delta } => {
                smallvec![Command::with_arg("mouse-move", delta.into())]
            }

            // Mouse Wheel
            DeviceEvent::MouseWheel {
                delta: MouseScrollDelta::LineDelta(x, y),
            } => smallvec![Command::with_arg("mouse-wheel", (x, y).into())],
            DeviceEvent::MouseWheel {
                delta: MouseScrollDelta::PixelDelta(s),
            } => smallvec![Command::with_arg("mouse-wheel", s.into())],

            // Mouse Button
            DeviceEvent::Button { button, state } => {
                self.button_state.insert(Key::MouseButton(button), state);
                self.match_key(Key::MouseButton(button), state)
            }

            // Match virtual keycodes.
            DeviceEvent::Key(KeyboardInput {
                virtual_keycode: Some(code),
                scancode,
                state,
                ..
            }) => {
                self.button_state.insert(Key::Physical(scancode), state);
                self.button_state.insert(Key::Virtual(code), state);
                self.match_key(Key::Virtual(code), state)
            }

            // Match scancodes.
            DeviceEvent::Key(KeyboardInput {
                virtual_keycode: None,
                scancode,
                state,
                ..
            }) => {
                self.button_state.insert(Key::Physical(scancode), state);
                smallvec![]
            }

            // Duplicate from MouseMotion for some reason?
            DeviceEvent::Motion { .. } => smallvec![],

            // I'm not sure what this does?
            DeviceEvent::Text { .. } => smallvec![],
        }
    }

    fn match_key(&self, key: Key, state: ElementState) -> SmallVec<[Command; 8]> {
        let mut out = SmallVec::new();
        for bindings in self.bindings.iter().rev() {
            out.extend(bindings.match_key(key, state, &self.button_state));
        }
        out
    }
}

impl Drop for InputSystem {
    fn drop(&mut self) {
        self.event_loop_proxy
            .send_event(MetaEvent::Stop)
            .expect("unable to send stop event");
        self.event_thread
            .take()
            .expect("a join handle")
            .join()
            .expect("result");
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use approx::assert_relative_eq;
    use std::path::PathBuf;
    use winit::{
        dpi::LogicalSize,
        event::{ModifiersState, VirtualKeyCode},
        window::WindowId,
    };

    fn logical_size() -> LogicalSize {
        LogicalSize {
            width: 8.,
            height: 9.,
        }
    }

    fn path() -> PathBuf {
        let mut buf = PathBuf::new();
        buf.push("a");
        buf.push("b");
        buf
    }

    fn win_evt(event: WindowEvent) -> Event<MetaEvent> {
        Event::WindowEvent {
            window_id: unsafe { WindowId::dummy() },
            event,
        }
    }

    fn dev_evt(event: DeviceEvent) -> Event<MetaEvent> {
        Event::DeviceEvent {
            device_id: unsafe { DeviceId::dummy() },
            event,
        }
    }

    fn vkey(key: VirtualKeyCode, state: bool) -> KeyboardInput {
        KeyboardInput {
            scancode: 0,
            virtual_keycode: Some(key),
            state: if state {
                ElementState::Pressed
            } else {
                ElementState::Released
            },
            modifiers: ModifiersState {
                ctrl: false,
                shift: false,
                logo: false,
                alt: false,
            },
        }
    }

    #[test]
    fn test_handle_system_events() -> Fallible<()> {
        let mut input = InputSystem::new(vec![])?;

        let cmd = input
            .handle_event(win_evt(WindowEvent::Resized(logical_size())))
            .first()
            .unwrap()
            .to_owned();
        assert_eq!(cmd.name, "window-resize");
        assert_relative_eq!(cmd.displacement()?.0, 8f64);
        assert_relative_eq!(cmd.displacement()?.1, 9f64);

        let cmd = input
            .handle_event(win_evt(WindowEvent::Destroyed))
            .first()
            .unwrap()
            .to_owned();
        assert_eq!(cmd.name, "window-destroy");

        let cmd = input
            .handle_event(win_evt(WindowEvent::CloseRequested))
            .first()
            .unwrap()
            .to_owned();
        assert_eq!(cmd.name, "window-close");

        let cmd = input
            .handle_event(win_evt(WindowEvent::DroppedFile(path())))
            .first()
            .unwrap()
            .to_owned();
        assert_eq!(cmd.name, "window-file-drop");
        assert_eq!(cmd.path()?, path());

        let cmd = input
            .handle_event(win_evt(WindowEvent::Focused(true)))
            .first()
            .unwrap()
            .to_owned();
        assert_eq!(cmd.name, "window-focus");
        assert!(cmd.boolean()?);

        let cmd = input
            .handle_event(win_evt(WindowEvent::HiDpiFactorChanged(42.)))
            .first()
            .unwrap()
            .to_owned();
        assert_eq!(cmd.name, "window-dpi-change");
        assert_relative_eq!(cmd.float()?, 42.);

        let cmd = input
            .handle_event(dev_evt(DeviceEvent::Added))
            .first()
            .unwrap()
            .to_owned();
        assert_eq!(cmd.name, "device-added");
        let cmd = input
            .handle_event(dev_evt(DeviceEvent::Removed))
            .first()
            .unwrap()
            .to_owned();
        assert_eq!(cmd.name, "device-removed");

        let cmd = input
            .handle_event(dev_evt(DeviceEvent::MouseMotion { delta: (8., 9.) }))
            .first()
            .unwrap()
            .to_owned();
        assert_eq!(cmd.name, "mouse-move");
        assert_relative_eq!(cmd.displacement()?.0, 8f64);
        assert_relative_eq!(cmd.displacement()?.1, 9f64);

        let cmd = input
            .handle_event(dev_evt(DeviceEvent::MouseWheel {
                delta: MouseScrollDelta::LineDelta(8., 9.),
            }))
            .first()
            .unwrap()
            .to_owned();
        assert_eq!(cmd.name, "mouse-wheel");
        assert_relative_eq!(cmd.displacement()?.0, 8f64);
        assert_relative_eq!(cmd.displacement()?.1, 9f64);

        Ok(())
    }

    #[test]
    fn test_can_handle_nested_events() -> Fallible<()> {
        let menu = Bindings::new("fps")
            .bind("+enter-menu", "alt")?
            .bind("exit", "shift+e")?
            .bind("click", "mouse0")?;
        let fps = Bindings::new("fps")
            .bind("+move-forward", "w")?
            .bind("eject", "shift+e")?
            .bind("fire", "mouse0")?;
        let mut input = InputSystem::new(vec![menu, fps])?;

        // FPS forward.
        let cmd = input
            .handle_event(dev_evt(DeviceEvent::Key(vkey(VirtualKeyCode::W, true))))
            .first()
            .unwrap()
            .to_owned();
        assert_eq!(cmd.name, "+move-forward");
        let cmd = input
            .handle_event(dev_evt(DeviceEvent::Key(vkey(VirtualKeyCode::W, false))))
            .first()
            .unwrap()
            .to_owned();
        assert_eq!(cmd.name, "-move-forward");

        // Mouse Button + find fire before click.
        let cmd = input
            .handle_event(dev_evt(DeviceEvent::Button {
                button: 0,
                state: ElementState::Pressed,
            }))
            .first()
            .unwrap()
            .to_owned();
        assert_eq!(cmd.name, "fire");
        let cmd = input.handle_event(dev_evt(DeviceEvent::Button {
            button: 0,
            state: ElementState::Released,
        }));
        assert!(cmd.is_empty());

        // Multiple buttons + found shift from LShfit + find eject instead of exit
        let cmd = input.handle_event(dev_evt(DeviceEvent::Key(vkey(
            VirtualKeyCode::LShift,
            true,
        ))));
        assert!(cmd.is_empty());
        let cmd = input
            .handle_event(dev_evt(DeviceEvent::Key(vkey(VirtualKeyCode::E, true))))
            .first()
            .unwrap()
            .to_owned();
        assert_eq!(cmd.name, "eject");

        // Let off e, drop fps, then hit again and get the other command
        let cmd = input.handle_event(dev_evt(DeviceEvent::Key(vkey(VirtualKeyCode::E, false))));
        assert!(cmd.is_empty());
        input.pop_bindings();
        let cmd = input
            .handle_event(dev_evt(DeviceEvent::Key(vkey(VirtualKeyCode::E, true))))
            .first()
            .unwrap()
            .to_owned();
        assert_eq!(cmd.name, "exit");
        let cmd = input.handle_event(dev_evt(DeviceEvent::Key(vkey(
            VirtualKeyCode::LShift,
            false,
        ))));
        assert!(cmd.is_empty());

        // Push on a new command set and ensure that it masks.
        let flight = Bindings::new("flight").bind("+pickle", "mouse0")?;
        input.push_bindings(flight);

        let cmd = input
            .handle_event(dev_evt(DeviceEvent::Button {
                button: 0,
                state: ElementState::Pressed,
            }))
            .first()
            .unwrap()
            .to_owned();
        assert_eq!(cmd.name, "+pickle");
        let cmd = input
            .handle_event(dev_evt(DeviceEvent::Button {
                button: 0,
                state: ElementState::Released,
            }))
            .first()
            .unwrap()
            .to_owned();
        assert_eq!(cmd.name, "-pickle");

        Ok(())
    }

    #[test]
    fn test_poll_events() -> Fallible<()> {
        let fps = Bindings::new("fps")
            .bind("+moveforward", "w")?
            .bind("eject", "shift+e")?;
        let mut input = InputSystem::new(vec![fps])?;
        input.poll()?;
        Ok(())
    }

    #[test]
    #[ignore]
    fn test_run_forever() -> Fallible<()> {
        use simplelog::{Config, LevelFilter, TermLogger};
        TermLogger::init(LevelFilter::Trace, Config::default())?;
        let fps = Bindings::new("fps")
            .bind("+moveforward", "w")?
            .bind("eject", "shift+e")?;
        let mut input = InputSystem::new(vec![fps])?;
        loop {
            let _evt = input.poll()?;
            std::thread::sleep(std::time::Duration::from_millis(4));
        }
    }
}
