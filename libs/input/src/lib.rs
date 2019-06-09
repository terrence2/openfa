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
use smallvec::SmallVec;
use std::collections::HashMap;
use winit::{
    dpi::{LogicalPosition, LogicalSize},
    DeviceEvent, DeviceId, ElementState, Event, EventsLoop, KeyboardInput, MouseScrollDelta,
    ScanCode, VirtualKeyCode, WindowEvent, WindowId,
};

pub trait CommandType {
    //fn from_str(command: &str) -> (CommandType, Option<CommandType>);
}

// Map from key, buttons, and axes to commands.
pub struct InputBindings<T: CommandType + Copy + Sync + Sized + Send> {
    pub name: String,
    chords: HashMap<Key, Vec<(KeySet, T, Option<T>)>>,
}

impl<T: CommandType + Copy + Sync + Sized + Send> InputBindings<T> {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            chords: HashMap::new(),
        }
    }

    pub fn bind_state(mut self, keyset: &str, activate: T, deactivate: T) -> Fallible<Self> {
        for ks in KeySet::from_virtual(keyset)?.drain(..) {
            self.chords
                .entry(ks.activating())
                .or_insert_with(Vec::new)
                .push((ks, activate, Some(deactivate)));
        }
        Ok(self)
    }

    pub fn bind_event(mut self, keyset: &str, activate: T) -> Fallible<Self> {
        for ks in KeySet::from_virtual(keyset)?.drain(..) {
            self.chords
                .entry(ks.activating())
                .or_insert_with(Vec::new)
                .push((ks, activate, None));
        }
        Ok(self)
    }

    fn match_keycode(
        &self,
        code: VirtualKeyCode,
        state: ElementState,
        keycode_states: &HashMap<VirtualKeyCode, ElementState>,
    ) -> Option<Command<T>> {
        if let Some(chords) = self.chords.get(&Key::Virtual(code)) {
            for (chord, activate, deactivate) in chords {
                if state == ElementState::Pressed
                    && Self::chord_is_pressed(&chord.keys, keycode_states)
                {
                    return Some(Command::new(*activate));
                }

                if deactivate.is_some()
                    && state == ElementState::Released
                    && Self::chord_is_pressed(&chord.keys[..chord.keys.len() - 1], keycode_states)
                {
                    return Some(Command::new(deactivate.unwrap()));
                }
            }
        }
        None
    }

    fn chord_is_pressed(
        binding_keys: &[Key],
        keycode_states: &HashMap<VirtualKeyCode, ElementState>,
    ) -> bool {
        for binding_key in binding_keys.iter() {
            if let Key::Virtual(binding_keycode) = binding_key {
                println!("CHECKING: {:?}", binding_keycode);
                if let Some(current_state) = keycode_states.get(binding_keycode) {
                    if *current_state == ElementState::Released {
                        return false;
                    }
                }
            }
        }
        true
    }
}

/*
impl From<LogicalSize> for Args {
    fn from(v: LogicalSize) -> Self {
        Args::two(v.width.into(), v.height.into())
    }
}

impl From<LogicalPosition> for Args {
    fn from(v: LogicalPosition) -> Self {
        Args::two(v.x.into(), v.y.into())
    }
}

impl From<(f64, f64)> for Args {
    fn from(v: (f64, f64)) -> Self {
        Args::two(v.0.into(), v.1.into())
    }
}
*/

#[derive(Clone, Debug)]
pub enum CommandArg {
    None,
    Position(LogicalPosition),
    Size(LogicalSize),
}

#[derive(Clone, Debug)]
pub struct Command<T: CommandType + Send + Sized + Sync> {
    pub name: T,
    pub arg: CommandArg,
}

impl<T: CommandType + Copy + Send + Sized + Sync> Command<T> {
    pub fn new(name: T) -> Self {
        Self {
            name,
            arg: CommandArg::None,
        }
    }
}

pub struct InputSystem<'a, T: CommandType + Copy + Send + Sized + Sync> {
    // Prioritized list of input binding sets. The last set with a matching
    // input binding "wins" and determines the command that is sent for that
    // input event.
    bindings: Vec<&'a InputBindings<T>>,

    // Track key states so that we can match button combos.
    keycodes_state: HashMap<VirtualKeyCode, ElementState>,
    scancodes_state: HashMap<ScanCode, ElementState>,
}

impl<'a, T: CommandType + Copy + Send + Sized + Sync> InputSystem<'a, T> {
    pub fn empty() -> Self {
        Self {
            bindings: Vec::new(),
            keycodes_state: HashMap::new(),
            scancodes_state: HashMap::new(),
        }
    }

    pub fn new(bindings: &[&'a InputBindings<T>]) -> Self {
        Self {
            bindings: bindings.to_owned(),
            keycodes_state: HashMap::new(),
            scancodes_state: HashMap::new(),
        }
    }

    pub fn push_bindings(mut self, bindings: &'a InputBindings<T>) -> Self {
        self.bindings.push(bindings);
        self
    }

    pub fn pop_bindings(mut self) -> Self {
        self.bindings.pop();
        self
    }

    pub fn poll(&mut self, events: &mut EventsLoop) -> SmallVec<[Command<T>; 8]> {
        let mut output = SmallVec::new();
        events.poll_events(|e| {
            if let Some(c) = self.handle_event(e) {
                output.push(c);
            }
        });
        output
    }

    pub fn handle_event(&mut self, e: Event) -> Option<Command<T>> {
        match e {
            Event::WindowEvent { window_id, event } => self.handle_window_event(window_id, event),
            Event::DeviceEvent { device_id, event } => self.handle_device_event(device_id, event),
            unhandled => {
                warn!("don't know how to handle: {:?}", unhandled);
                None
            }
        }
    }

    fn handle_window_event(&self, window_id: WindowId, event: WindowEvent) -> Option<Command<T>> {
        //println!("WINDOW EVENT ({:?}): {:?}", window_id, event);
        match event {
            //WindowEvent::Resized(s) => cmd("window-resize", s.into()),
            _ => {
                warn!("unknown window event: {:?}", event);
                None
            }
        }
    }

    fn handle_device_event(
        &mut self,
        device_id: DeviceId,
        event: DeviceEvent,
    ) -> Option<Command<T>> {
        //println!("DEVICE EVENT ({:?}): {:?}", device_id, event);
        match event {
            // DeviceEvent::Added => cmd("device-added", device_id.into()),
            // DeviceEvent::Removed => cmd("device-removed", device_id.into()),

            // Mouse Motion
            DeviceEvent::MouseMotion { delta } => cmd("mouse-move", delta.into()),

            // DeviceEvent::MouseWheel {
            //     delta: MouseScrollDelta::LineDelta(x, y),
            // } => cmd("mouse-wheel", Args::two(x.into(), y.into())),

            // Match virtual keycodes.
            DeviceEvent::Key(KeyboardInput {
                virtual_keycode: Some(code),
                scancode,
                state,
                ..
            }) => {
                self.scancodes_state.insert(scancode, state);
                self.keycodes_state.insert(code, state);
                self.match_keycode(code, state)
            }

            // Match scancodes.
            DeviceEvent::Key(KeyboardInput {
                virtual_keycode: None,
                scancode,
                state,
                ..
            }) => {
                self.scancodes_state.insert(scancode, state);
                None
            }

            // Duplicate from MouseMotion for some reason?
            DeviceEvent::Motion { .. } => None,
            _ => {
                warn!("unknown device event: {:?}", event);
                None
            }
        }
    }

    fn match_keycode(&self, code: VirtualKeyCode, state: ElementState) -> Option<Command<T>> {
        for bindings in self.bindings.iter().rev() {
            if let Some(c) = bindings.match_keycode(code, state, &self.keycodes_state) {
                return Some(c);
            }
        }
        None
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use approx::assert_relative_eq;
    use window::{GraphicsConfigBuilder, GraphicsWindow};
    use winit::ModifiersState;

    fn logical_size() -> LogicalSize {
        LogicalSize {
            width: 10.,
            height: 10.,
        }
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

    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    enum MenuCommands {
        SelectMenu(bool),
        Exit,
    }
    impl CommandType for MenuCommands {}
    use MenuCommands as MC;

    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    enum FpsCommands {
        MoveForward(bool),
        Eject,
    }
    impl CommandType for FpsCommands {}
    use FpsCommands as FC;

    #[test]
    fn test_can_handle_nested_events() -> Fallible<()> {
        let menu = InputBindings::<MC>::new("fps")
            .bind_state("alt", MC::SelectMenu(true), MC::SelectMenu(false))?
            .bind_event("shift+e", MC::Exit)?;
        let fps = InputBindings::<FC>::new("fps")
            .bind_state("w", FC::MoveForward(true), FC::MoveForward(false))?
            .bind_event("shift+e", FC::Eject)?;
        let mut input = InputSystem::new(&[&menu, &fps]);

        // let cmd = input
        //     .handle_event(win_evt(WindowEvent::Resized(logical_size())))
        //     .unwrap();
        // assert_eq!(cmd.name, "window-resize");
        // assert_relative_eq!(cmd.args.first()?.float()?, 10f64);

        // let cmd = input
        //     .handle_event(dev_evt(DeviceEvent::MouseMotion { delta: (10., 10.) }))
        //     .unwrap();
        // assert_eq!(cmd.name, "mouse-move");
        // assert_relative_eq!(cmd.args.first()?.float()?, 10f64);

        // let cmd = input
        //     .handle_event(dev_evt(DeviceEvent::MouseWheel {
        //         delta: MouseScrollDelta::LineDelta(10., 10.),
        //     }))
        //     .unwrap();
        // assert_eq!(cmd.name, "mouse-wheel");
        // assert_relative_eq!(cmd.args.first()?.float()?, 10f64);

        let cmd = input
            .handle_event(dev_evt(DeviceEvent::Key(vkey(VirtualKeyCode::W, true))))
            .unwrap();
        assert_eq!(cmd.name, FC::MoveForward(true));
        let cmd = input
            .handle_event(dev_evt(DeviceEvent::Key(vkey(VirtualKeyCode::W, false))))
            .unwrap();
        assert_eq!(cmd.name, FC::MoveForward(false));

        Ok(())
    }

    #[test]
    fn test_poll_events() -> Fallible<()> {
        let mut window = GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?;
        let fps = InputBindings::<FC>::new("fps")
            .bind_state("w", FC::MoveForward(true), FC::MoveForward(false))?
            .bind_event("shift+e", FC::Eject)?;
        let mut input = InputSystem::new(&[&fps]);
        input.poll(&mut window.events_loop);
        Ok(())
    }

    #[test]
    #[ignore]
    fn test_run_forever() -> Fallible<()> {
        use simplelog::{Config, LevelFilter, TermLogger};
        TermLogger::init(LevelFilter::Trace, Config::default())?;
        let mut window = GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?;
        let fps = InputBindings::<FC>::new("fps")
            .bind_state("w", FC::MoveForward(true), FC::MoveForward(false))?
            .bind_event("shift+e", FC::Eject)?;
        let mut input = InputSystem::new(&[&fps]);
        loop {
            let evt = input.poll(&mut window.events_loop);
            std::thread::sleep_ms(4);
        }
    }
}
