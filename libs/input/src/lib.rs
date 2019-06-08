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

// Map from key, buttons, and axes to commands.
pub struct InputBindings {
    pub name: String,
    chords: HashMap<Key, Vec<(KeySet, &'static str)>>,
}

impl InputBindings {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            chords: HashMap::new(),
        }
    }

    pub fn bind(mut self, command: &'static str, keyset: &str) -> Fallible<Self> {
        for ks in KeySet::from_virtual(keyset)?.drain(..) {
            // TODO parse command into up-down
            self.chords
                .entry(ks.activating())
                .or_insert_with(Vec::new)
                .push((ks, command));
        }
        Ok(self)
    }

    fn match_keycode(
        &self,
        code: VirtualKeyCode,
        state: ElementState,
        keycode_states: &HashMap<VirtualKeyCode, ElementState>,
    ) -> Option<Command> {
        if let Some(chords) = self.chords.get(&Key::Virtual(code)) {
            for (chord, command) in chords {
                if state == ElementState::Pressed
                    && Self::chord_is_pressed(&chord.keys, keycode_states)
                {
                    return Some(Command::new(command));
                }

                if state == ElementState::Released
                    && Self::chord_is_pressed(&chord.keys[..chord.keys.len() - 1], keycode_states)
                {
                    // use std::borrow::Cow;
                    // let cmd = if command.starts_with("+") {
                    //     Cow::from("-".to_owned() + &command[1..])
                    // } else {
                    //     Cow::from(command)
                    // };
                    // return Some(Command::new(cmd));
                    return Some(Command::new(command));
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

#[derive(Clone, Debug)]
pub enum Arg {
    Device(DeviceId),
    Float(f64),
}

impl Arg {
    pub fn float(&self) -> Fallible<f64> {
        match self {
            Arg::Float(v) => Ok(*v),
            _ => bail!("argument is not a float"),
        }
    }
}

impl From<f64> for Arg {
    fn from(v: f64) -> Self {
        Arg::Float(v)
    }
}

impl From<f32> for Arg {
    fn from(v: f32) -> Self {
        Arg::Float(f64::from(v))
    }
}

impl From<DeviceId> for Arg {
    fn from(v: DeviceId) -> Self {
        Arg::Device(v)
    }
}

#[derive(Clone, Debug)]
pub struct Args {
    a0: Option<Arg>,
    a1: Option<Arg>,
}

impl Args {
    pub fn empty() -> Self {
        Args { a0: None, a1: None }
    }

    pub fn one(a0: Arg) -> Self {
        Args {
            a0: Some(a0),
            a1: None,
        }
    }

    pub fn two(a0: Arg, a1: Arg) -> Self {
        Args {
            a0: Some(a0),
            a1: Some(a1),
        }
    }

    pub fn first(&self) -> Fallible<Arg> {
        if let Some(ref a) = self.a0 {
            return Ok(a.clone());
        }
        bail!("not enough arguments: 1 of 0")
    }

    pub fn second(&self) -> Fallible<Arg> {
        if let Some(ref a) = self.a1 {
            return Ok(a.clone());
        }
        bail!(
            "not enough arguments: 2 of {}",
            if self.a0.is_some() { 1 } else { 0 }
        )
    }
}

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

impl From<Arg> for Args {
    fn from(v: Arg) -> Self {
        Args::one(v)
    }
}

impl From<DeviceId> for Args {
    fn from(v: DeviceId) -> Self {
        Args::one(v.into())
    }
}

impl From<(f64, f64)> for Args {
    fn from(v: (f64, f64)) -> Self {
        Args::two(v.0.into(), v.1.into())
    }
}

#[derive(Clone, Debug)]
pub struct Command {
    pub name: &'static str,
    pub args: Args,
}

impl Command {
    // pub fn empty() -> Self {
    //     Self {
    //         name: "empty",
    //         args: Args::empty(),
    //     }
    // }

    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            args: Args::empty(),
        }
    }
}

pub fn cmd(name: &'static str, args: Args) -> Option<Command> {
    Some(Command { name, args })
}

pub struct InputSystem<'a> {
    // Prioritized list of input binding sets. The last set with a matching
    // input binding "wins" and determines the command that is sent for that
    // input event.
    bindings: Vec<&'a InputBindings>,

    // Track key states so that we can match button combos.
    keycodes_state: HashMap<VirtualKeyCode, ElementState>,
    scancodes_state: HashMap<ScanCode, ElementState>,
}

impl<'a> InputSystem<'a> {
    pub fn empty() -> Self {
        Self {
            bindings: Vec::new(),
            keycodes_state: HashMap::new(),
            scancodes_state: HashMap::new(),
        }
    }

    pub fn new(bindings: &[&'a InputBindings]) -> Self {
        Self {
            bindings: bindings.to_owned(),
            keycodes_state: HashMap::new(),
            scancodes_state: HashMap::new(),
        }
    }

    pub fn push_bindings(mut self, bindings: &'a InputBindings) -> Self {
        self.bindings.push(bindings);
        self
    }

    pub fn pop_bindings(mut self) -> Self {
        self.bindings.pop();
        self
    }

    pub fn poll(&mut self, events: &mut EventsLoop) -> SmallVec<[Command; 8]> {
        let mut output = SmallVec::new();
        events.poll_events(|e| {
            if let Some(c) = self.handle_event(e) {
                output.push(c);
            }
        });
        output
    }

    pub fn handle_event(&mut self, e: Event) -> Option<Command> {
        match e {
            Event::WindowEvent { window_id, event } => self.handle_window_event(window_id, event),
            Event::DeviceEvent { device_id, event } => self.handle_device_event(device_id, event),
            unhandled => {
                warn!("don't know how to handle: {:?}", unhandled);
                None
            }
        }
    }

    fn handle_window_event(&self, window_id: WindowId, event: WindowEvent) -> Option<Command> {
        //println!("WINDOW EVENT ({:?}): {:?}", window_id, event);
        match event {
            WindowEvent::Resized(s) => cmd("window-resize", s.into()),
            _ => {
                warn!("unknown window event: {:?}", event);
                None
            }
        }
    }

    fn handle_device_event(&mut self, device_id: DeviceId, event: DeviceEvent) -> Option<Command> {
        //println!("DEVICE EVENT ({:?}): {:?}", device_id, event);
        match event {
            DeviceEvent::Added => cmd("device-added", device_id.into()),
            DeviceEvent::Removed => cmd("device-removed", device_id.into()),
            DeviceEvent::MouseMotion { delta } => cmd("mouse-move", delta.into()),
            DeviceEvent::MouseWheel {
                delta: MouseScrollDelta::LineDelta(x, y),
            } => cmd("mouse-wheel", Args::two(x.into(), y.into())),
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

    fn match_keycode(&self, code: VirtualKeyCode, state: ElementState) -> Option<Command> {
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

    #[test]
    fn test_can_handle_common_events() -> Fallible<()> {
        let fps = InputBindings::new("fps")
            .bind("+moveforward", "W")?
            .bind("+moveleft", "a")?
            .bind("+moveback", "S")?
            .bind("+moveright", "d")?
            .bind("eject", "shift+e")?;
        let mut input = InputSystem::new(&[&fps]);

        let cmd = input
            .handle_event(win_evt(WindowEvent::Resized(logical_size())))
            .unwrap();
        assert_eq!(cmd.name, "window-resize");
        assert_relative_eq!(cmd.args.first()?.float()?, 10f64);

        let cmd = input
            .handle_event(dev_evt(DeviceEvent::MouseMotion { delta: (10., 10.) }))
            .unwrap();
        assert_eq!(cmd.name, "mouse-move");
        assert_relative_eq!(cmd.args.first()?.float()?, 10f64);

        let cmd = input
            .handle_event(dev_evt(DeviceEvent::MouseWheel {
                delta: MouseScrollDelta::LineDelta(10., 10.),
            }))
            .unwrap();
        assert_eq!(cmd.name, "mouse-wheel");
        assert_relative_eq!(cmd.args.first()?.float()?, 10f64);

        let cmd = input
            .handle_event(dev_evt(DeviceEvent::Key(vkey(VirtualKeyCode::W, true))))
            .unwrap();
        assert_eq!(cmd.name, "+moveforward");
        let cmd = input
            .handle_event(dev_evt(DeviceEvent::Key(vkey(VirtualKeyCode::W, false))))
            .unwrap();
        assert_eq!(cmd.name, "-moveforward");

        Ok(())
    }

    #[test]
    fn test_poll_events() -> Fallible<()> {
        let mut window = GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?;
        let fps = InputBindings::new("fps")
            .bind("+moveforward", "W")?
            .bind("+moveleft", "A")?
            .bind("+moveback", "S")?
            .bind("+moveright", "D")?;
        let mut input = InputSystem::new(&[&fps]);
        input.poll(&mut window.events_loop);
        Ok(())
    }

    #[test]
    //#[ignore]
    fn test_run_forever() -> Fallible<()> {
        use simplelog::{Config, LevelFilter, TermLogger};
        TermLogger::init(LevelFilter::Trace, Config::default())?;
        let mut window = GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?;
        let fps = InputBindings::new("fps")
            .bind("+moveforward", "W")?
            .bind("+moveleft", "A")?
            .bind("+moveback", "S")?
            .bind("+moveright", "D")?;
        let mut input = InputSystem::new(&[&fps]);
        loop {
            let evt = input.poll(&mut window.events_loop);
            std::thread::sleep_ms(4);
        }
    }
}
