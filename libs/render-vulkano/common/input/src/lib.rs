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
mod keyset;

pub use crate::keyset::{Key, KeySet};
use failure::{bail, Fallible};
use log::warn;
use smallvec::{smallvec, SmallVec};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};
use winit::{
    dpi::{LogicalPosition, LogicalSize},
    DeviceEvent, DeviceId, ElementState, Event, EventsLoop, KeyboardInput, MouseScrollDelta,
    WindowEvent, WindowId,
};

// Map from key, buttons, and axes to commands.
pub struct InputBindings {
    pub name: String,
    press_chords: HashMap<Key, Vec<(KeySet, String)>>,
    release_keys: HashMap<Key, HashSet<String>>,
}

impl InputBindings {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            press_chords: HashMap::new(),
            release_keys: HashMap::new(),
        }
    }

    pub fn bind(mut self, command: &str, keyset: &str) -> Fallible<Self> {
        let (activate, deactivate) = if command.starts_with('+') {
            (command, Some(format!("-{}", &command[1..])))
        } else {
            (command, None)
        };
        for ks in KeySet::from_virtual(keyset)?.drain(..) {
            let sets = self
                .press_chords
                .entry(ks.activating())
                .or_insert_with(Vec::new);

            if let Some(ref d) = deactivate {
                for key in &ks.keys {
                    let keys = self.release_keys.entry(*key).or_insert_with(HashSet::new);
                    keys.insert(d.to_owned());
                }
            }

            sets.push((ks, activate.to_owned()));
            sets.sort_by_key(|(set, _)| usize::max_value() - set.keys.len());
        }
        Ok(self)
    }

    fn match_key(
        &self,
        key: Key,
        state: ElementState,
        key_states: &HashMap<Key, ElementState>,
    ) -> SmallVec<[Command; 4]> {
        if state == ElementState::Pressed {
            if let Some(chords) = self.press_chords.get(&key) {
                for (chord, activate) in chords {
                    if Self::chord_is_pressed(&chord.keys, key_states) {
                        return smallvec![Command::from_string(activate.to_owned())];
                    }
                }
            }
        } else if let Some(commands) = self.release_keys.get(&key) {
            return commands
                .iter()
                .map(|v| Command::from_string(v.to_owned()))
                .collect::<SmallVec<_>>();
        }
        smallvec![]
    }

    fn chord_is_pressed(binding_keys: &[Key], key_states: &HashMap<Key, ElementState>) -> bool {
        for binding_key in binding_keys.iter() {
            if let Some(current_state) = key_states.get(binding_key) {
                if *current_state == ElementState::Released {
                    return false;
                }
            } else {
                return false;
            }
        }
        true
    }
}

#[derive(Clone, Debug)]
pub enum CommandArg {
    None,
    Boolean(bool),
    Float(f64),
    Path(PathBuf),
    Device(DeviceId),
    Displacement((f64, f64)),
}

impl From<DeviceId> for CommandArg {
    fn from(v: DeviceId) -> Self {
        CommandArg::Device(v)
    }
}

impl From<(f64, f64)> for CommandArg {
    fn from(v: (f64, f64)) -> Self {
        CommandArg::Displacement((v.0, v.1))
    }
}

impl From<(f32, f32)> for CommandArg {
    fn from(v: (f32, f32)) -> Self {
        CommandArg::Displacement((f64::from(v.0), f64::from(v.1)))
    }
}

impl From<f64> for CommandArg {
    fn from(v: f64) -> Self {
        CommandArg::Float(v)
    }
}

impl From<LogicalSize> for CommandArg {
    fn from(v: LogicalSize) -> Self {
        CommandArg::Displacement((v.width, v.height))
    }
}

impl From<LogicalPosition> for CommandArg {
    fn from(v: LogicalPosition) -> Self {
        CommandArg::Displacement((v.x, v.y))
    }
}

impl From<PathBuf> for CommandArg {
    fn from(v: PathBuf) -> Self {
        CommandArg::Path(v)
    }
}

impl From<bool> for CommandArg {
    fn from(v: bool) -> Self {
        CommandArg::Boolean(v)
    }
}

#[derive(Clone, Debug)]
pub struct Command {
    pub name: String,
    pub arg: CommandArg,
}

impl Command {
    pub fn from_string(name: String) -> Self {
        Self {
            name,
            arg: CommandArg::None,
        }
    }

    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            arg: CommandArg::None,
        }
    }

    pub fn with_arg(name: &str, arg: CommandArg) -> Self {
        Self {
            name: name.to_owned(),
            arg,
        }
    }

    pub fn boolean(&self) -> Fallible<bool> {
        match self.arg {
            CommandArg::Boolean(v) => Ok(v),
            _ => bail!("not a boolean argument"),
        }
    }

    pub fn float(&self) -> Fallible<f64> {
        match self.arg {
            CommandArg::Float(v) => Ok(v),
            _ => bail!("not a float argument"),
        }
    }

    pub fn path(&self) -> Fallible<PathBuf> {
        match &self.arg {
            CommandArg::Path(v) => Ok(v.to_path_buf()),
            _ => bail!("not a path argument"),
        }
    }

    pub fn displacement(&self) -> Fallible<(f64, f64)> {
        match self.arg {
            CommandArg::Displacement(v) => Ok(v),
            _ => bail!("not a displacement argument"),
        }
    }

    pub fn device(&self) -> Fallible<DeviceId> {
        match self.arg {
            CommandArg::Device(v) => Ok(v),
            _ => bail!("not a device argument"),
        }
    }
}

pub struct InputSystem {
    // Prioritized list of input binding sets. The last set with a matching
    // input binding "wins" and determines the command that is sent for that
    // input event.
    bindings: Vec<InputBindings>,

    // Track key states so that we can match button combos.
    button_state: HashMap<Key, ElementState>,
}

impl InputSystem {
    pub fn new(bindings: Vec<InputBindings>) -> Self {
        Self {
            bindings,
            button_state: HashMap::new(),
        }
    }

    pub fn push_bindings(&mut self, bindings: InputBindings) {
        self.bindings.push(bindings);
    }

    pub fn pop_bindings(&mut self) -> Option<InputBindings> {
        self.bindings.pop()
    }

    pub fn poll(&mut self, events: &mut EventsLoop) -> SmallVec<[Command; 8]> {
        let mut out = SmallVec::new();
        events.poll_events(|e| {
            out.extend(self.handle_event(e));
        });
        out
    }

    pub fn handle_event(&mut self, e: Event) -> SmallVec<[Command; 8]> {
        match e {
            Event::WindowEvent { window_id, event } => self.handle_window_event(window_id, event),
            Event::DeviceEvent { device_id, event } => self.handle_device_event(device_id, event),
            unhandled => {
                warn!("don't know how to handle: {:?}", unhandled);
                smallvec![]
            }
        }
    }

    fn handle_window_event(
        &self,
        _window_id: WindowId,
        event: WindowEvent,
    ) -> SmallVec<[Command; 8]> {
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
            WindowEvent::Refresh => smallvec![],
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

#[cfg(test)]
mod test {
    use super::*;
    use approx::assert_relative_eq;
    use window::{GraphicsConfigBuilder, GraphicsWindow};
    use winit::{ModifiersState, VirtualKeyCode};

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

    fn win_evt(event: WindowEvent) -> Event {
        Event::WindowEvent {
            window_id: unsafe { WindowId::dummy() },
            event,
        }
    }

    fn dev_evt(event: DeviceEvent) -> Event {
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
        let mut input = InputSystem::new(vec![]);

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
        let menu = InputBindings::new("fps")
            .bind("+enter-menu", "alt")?
            .bind("exit", "shift+e")?
            .bind("click", "mouse0")?;
        let fps = InputBindings::new("fps")
            .bind("+move-forward", "w")?
            .bind("eject", "shift+e")?
            .bind("fire", "mouse0")?;
        let mut input = InputSystem::new(vec![menu, fps]);

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
        let flight = InputBindings::new("flight").bind("+pickle", "mouse0")?;
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
        let mut window = GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?;
        let fps = InputBindings::new("fps")
            .bind("+moveforward", "w")?
            .bind("eject", "shift+e")?;
        let mut input = InputSystem::new(vec![fps]);
        input.poll(&mut window.events_loop);
        Ok(())
    }

    #[test]
    #[ignore]
    fn test_run_forever() -> Fallible<()> {
        use simplelog::{Config, LevelFilter, TermLogger};
        TermLogger::init(LevelFilter::Trace, Config::default())?;
        let mut window = GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?;
        let fps = InputBindings::new("fps")
            .bind("+moveforward", "w")?
            .bind("eject", "shift+e")?;
        let mut input = InputSystem::new(vec![fps]);
        loop {
            let _evt = input.poll(&mut window.events_loop);
            std::thread::sleep(std::time::Duration::from_millis(4));
        }
    }
}
