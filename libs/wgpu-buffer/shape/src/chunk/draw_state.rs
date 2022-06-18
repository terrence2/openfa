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
use crate::chunk::upload::{ShapeErrata, VertexFlags};
use animate::{Animation, LinearAnimationTemplate};
use anyhow::{bail, Result};
use bevy_ecs::prelude::*;
use bitflags::bitflags;
use nitrous::{inject_nitrous_component, method, NitrousComponent};
use std::time::{Duration, Instant};

const ANIMATION_FRAME_TIME: usize = 166; // ms

const BAY_ANIMATION_EXTENT: f32 = -8192f32;
const GEAR_ANIMATION_EXTENT: f32 = -8192f32;

bitflags! {
    pub struct DrawStateFlags: u16 {
        const FLAPS_DOWN          = 0x0002;
        const SLATS_DOWN          = 0x0004;
        const AIRBRAKE_EXTENDED   = 0x0008;
        const HOOK_EXTENDED       = 0x0010;
        const AFTERBURNER_ENABLED = 0x0020;
        const PLAYER_DEAD         = 0x0040;
        const RUDDER_LEFT         = 0x0080;
        const RUDDER_RIGHT        = 0x0100;
        const LEFT_AILERON_DOWN   = 0x0200;
        const LEFT_AILERON_UP     = 0x0400;
        const RIGHT_AILERON_DOWN  = 0x0800;
        const RIGHT_AILERON_UP    = 0x1000;
        const BAY_VISIBLE         = 0x2000;
        const GEAR_VISIBLE        = 0x4000;
    }
}

#[derive(Component, NitrousComponent, Clone, Copy, Debug, PartialEq)]
#[Name = "draw_state"]
pub struct DrawState {
    gear_position: f32,
    bay_position: f32,
    base_time: Instant,
    thrust_vector_pos: i16,
    thrust_vector_delta: i16,
    wing_sweep_pos: i16,
    wing_sweep_delta: i16,
    sam_count: i8,
    eject_state: u8,
    flags: DrawStateFlags,
    errata: ShapeErrata,
}

#[inject_nitrous_component]
impl DrawState {
    pub fn new(errata: ShapeErrata) -> Self {
        DrawState {
            gear_position: 0.,
            bay_position: 0.,
            base_time: Instant::now(),
            thrust_vector_pos: 0,
            thrust_vector_delta: 0,
            wing_sweep_pos: 0,
            wing_sweep_delta: 0,
            sam_count: 3,
            eject_state: 0,
            flags: DrawStateFlags::AIRBRAKE_EXTENDED
                | DrawStateFlags::HOOK_EXTENDED
                | DrawStateFlags::AFTERBURNER_ENABLED,
            errata,
        }
    }

    pub fn gear_visible(&self) -> bool {
        self.flags.contains(DrawStateFlags::GEAR_VISIBLE)
    }

    pub fn bay_visible(&self) -> bool {
        self.flags.contains(DrawStateFlags::BAY_VISIBLE)
    }

    pub fn time_origin(&self) -> &Instant {
        &self.base_time
    }

    pub fn thrust_vector_position(&self) -> f32 {
        f32::from(self.thrust_vector_pos)
    }

    pub fn flaps_extended(&self) -> bool {
        self.flags.contains(DrawStateFlags::FLAPS_DOWN)
    }

    pub fn slats_extended(&self) -> bool {
        self.flags.contains(DrawStateFlags::SLATS_DOWN)
    }

    pub fn airbrake_extended(&self) -> bool {
        self.flags.contains(DrawStateFlags::AIRBRAKE_EXTENDED)
    }

    pub fn hook_extended(&self) -> bool {
        self.flags.contains(DrawStateFlags::HOOK_EXTENDED)
    }

    pub fn afterburner_enabled(&self) -> bool {
        self.flags.contains(DrawStateFlags::AFTERBURNER_ENABLED)
    }

    pub fn wing_sweep_angle(&self) -> i16 {
        self.wing_sweep_pos
    }

    pub fn x86_gear_down(&self) -> u32 {
        self.gear_visible() as u32
    }

    pub fn x86_gear_position(&self) -> u32 {
        self.gear_position as i32 as u32
    }

    pub fn x86_bay_open(&self) -> u32 {
        self.bay_visible() as u32
    }

    pub fn x86_bay_position(&self) -> u32 {
        self.bay_position as i32 as u32
    }

    pub fn x86_canard_position(&self) -> u32 {
        self.thrust_vector_position() as i32 as u32
    }

    pub fn x86_vertical_angle(&self) -> u32 {
        self.thrust_vector_position() as i32 as u32
    }

    pub fn x86_afterburner_enabled(&self) -> u32 {
        self.afterburner_enabled() as u32
    }

    pub fn x86_swing_wing(&self) -> u32 {
        i32::from(self.wing_sweep_angle()) as u32
    }

    #[method]
    pub fn player_dead(&self) -> bool {
        self.flags.contains(DrawStateFlags::PLAYER_DEAD)
    }

    #[method]
    pub fn rudder_left(&self) -> bool {
        self.flags.contains(DrawStateFlags::RUDDER_LEFT)
    }

    #[method]
    pub fn rudder_right(&self) -> bool {
        self.flags.contains(DrawStateFlags::RUDDER_RIGHT)
    }

    #[method]
    pub fn left_aileron_down(&self) -> bool {
        self.flags.contains(DrawStateFlags::LEFT_AILERON_DOWN)
    }

    #[method]
    pub fn left_aileron_up(&self) -> bool {
        self.flags.contains(DrawStateFlags::LEFT_AILERON_UP)
    }

    #[method]
    pub fn right_aileron_down(&self) -> bool {
        self.flags.contains(DrawStateFlags::RIGHT_AILERON_DOWN)
    }

    #[method]
    pub fn right_aileron_up(&self) -> bool {
        self.flags.contains(DrawStateFlags::RIGHT_AILERON_UP)
    }

    pub fn set_flaps(&mut self, extended: bool) {
        self.flags.set(DrawStateFlags::FLAPS_DOWN, extended);
        self.flags.set(DrawStateFlags::SLATS_DOWN, extended);
    }

    pub fn set_airbrake(&mut self, extended: bool) {
        self.flags.set(DrawStateFlags::AIRBRAKE_EXTENDED, extended);
    }

    pub fn set_hook(&mut self, extended: bool) {
        self.flags.set(DrawStateFlags::HOOK_EXTENDED, extended);
    }

    #[method]
    pub fn toggle_player_dead(&mut self) {
        self.flags.toggle(DrawStateFlags::PLAYER_DEAD);
    }

    #[method]
    pub fn bump_eject_state(&mut self) {
        self.eject_state += 1;
        self.eject_state %= 5;
    }

    pub fn enable_afterburner(&mut self) {
        self.flags.insert(DrawStateFlags::AFTERBURNER_ENABLED);
    }

    pub fn disable_afterburner(&mut self) {
        self.flags.remove(DrawStateFlags::AFTERBURNER_ENABLED);
    }

    #[method]
    pub fn move_rudder_center(&mut self) {
        self.flags.remove(DrawStateFlags::RUDDER_LEFT);
        self.flags.remove(DrawStateFlags::RUDDER_RIGHT);
    }

    #[method]
    pub fn move_rudder_left(&mut self) {
        self.flags.insert(DrawStateFlags::RUDDER_LEFT);
        self.flags.remove(DrawStateFlags::RUDDER_RIGHT);
    }

    #[method]
    pub fn move_rudder_right(&mut self) {
        self.flags.remove(DrawStateFlags::RUDDER_LEFT);
        self.flags.insert(DrawStateFlags::RUDDER_RIGHT);
    }

    #[method]
    pub fn move_stick_center(&mut self) {
        self.flags.remove(DrawStateFlags::LEFT_AILERON_DOWN);
        self.flags.remove(DrawStateFlags::LEFT_AILERON_UP);
        self.flags.remove(DrawStateFlags::RIGHT_AILERON_DOWN);
        self.flags.remove(DrawStateFlags::RIGHT_AILERON_UP);
    }

    pub fn elevator_up(&mut self) {}

    pub fn elevator_down(&mut self) {}

    pub fn elevator_center(&mut self) {}

    #[method]
    pub fn vector_thrust_forward(&mut self, pressed: bool) {
        if pressed {
            self.thrust_vector_delta = 10;
        } else {
            self.thrust_vector_delta = 0;
        }
    }

    #[method]
    pub fn vector_thrust_backward(&mut self, pressed: bool) {
        if pressed {
            self.thrust_vector_delta = -10;
        } else {
            self.thrust_vector_delta = 0;
        }
    }

    #[method]
    pub fn vector_thrust_recenter(&mut self) {
        self.thrust_vector_pos = 0;
    }

    pub fn increase_wing_sweep(&mut self) {
        self.wing_sweep_delta = 50;
    }

    pub fn decrease_wing_sweep(&mut self) {
        self.wing_sweep_delta = -50;
    }

    pub fn stop_wing_sweep(&mut self) {
        self.wing_sweep_delta = 0;
    }

    #[method]
    pub fn move_stick_left(&mut self) {
        self.flags.remove(DrawStateFlags::LEFT_AILERON_DOWN);
        self.flags.insert(DrawStateFlags::LEFT_AILERON_UP);
        self.flags.insert(DrawStateFlags::RIGHT_AILERON_DOWN);
        self.flags.remove(DrawStateFlags::RIGHT_AILERON_UP);
    }

    #[method]
    pub fn move_stick_right(&mut self) {
        self.flags.insert(DrawStateFlags::LEFT_AILERON_DOWN);
        self.flags.remove(DrawStateFlags::LEFT_AILERON_UP);
        self.flags.remove(DrawStateFlags::RIGHT_AILERON_DOWN);
        self.flags.insert(DrawStateFlags::RIGHT_AILERON_UP);
    }

    #[method]
    pub fn consume_sam(&mut self) {
        self.sam_count -= 1;
        if self.sam_count < 0 {
            self.sam_count = 3;
        }
    }

    pub fn set_gear_visible(&mut self, v: bool) {
        self.flags.set(DrawStateFlags::GEAR_VISIBLE, v);
    }

    // Map [0,1] to [-8192,0]
    pub fn set_gear_position(&mut self, f: f32) {
        self.gear_position = (1. - f) * GEAR_ANIMATION_EXTENT;
    }

    pub fn set_bay_visible(&mut self, v: bool) {
        self.flags.set(DrawStateFlags::BAY_VISIBLE, v);
    }

    // Map [0,1] to [-8192,0]
    pub fn set_bay_position(&mut self, f: f32) {
        self.bay_position = (1. - f) * BAY_ANIMATION_EXTENT;
    }

    pub fn animate(&mut self, now: &Instant) {
        self.thrust_vector_pos += self.thrust_vector_delta;
        self.wing_sweep_pos += self.wing_sweep_delta;
    }

    pub fn build_mask_into(&self, start: &Instant, buffer: &mut [u32]) -> Result<()> {
        let flags = self.build_mask(start)?;
        buffer[0] = (flags & 0xFFFF_FFFF) as u32;
        buffer[1] = (flags >> 32) as u32;
        Ok(())
    }

    pub fn build_mask(&self, start: &Instant) -> Result<u64> {
        let mut mask = VertexFlags::STATIC | VertexFlags::BLEND_TEXTURE;

        let elapsed = start.elapsed().as_millis() as usize;
        let frame_off = elapsed / ANIMATION_FRAME_TIME;
        mask |= VertexFlags::ANIM_FRAME_0_2.displacement(frame_off % 2)?;
        mask |= VertexFlags::ANIM_FRAME_0_3.displacement(frame_off % 3)?;
        mask |= VertexFlags::ANIM_FRAME_0_4.displacement(frame_off % 4)?;
        mask |= VertexFlags::ANIM_FRAME_0_6.displacement(frame_off % 6)?;

        mask |= if self.flaps_extended() {
            VertexFlags::LEFT_FLAP_DOWN | VertexFlags::RIGHT_FLAP_DOWN
        } else {
            VertexFlags::LEFT_FLAP_UP | VertexFlags::RIGHT_FLAP_UP
        };

        mask |= if self.slats_extended() {
            VertexFlags::SLATS_DOWN
        } else {
            VertexFlags::SLATS_UP
        };

        mask |= if self.airbrake_extended() {
            VertexFlags::BRAKE_EXTENDED
        } else {
            VertexFlags::BRAKE_RETRACTED
        };

        mask |= if self.hook_extended() {
            VertexFlags::HOOK_EXTENDED
        } else {
            VertexFlags::HOOK_RETRACTED
        };

        mask |= if self.rudder_right() {
            VertexFlags::RUDDER_RIGHT
        } else if self.rudder_left() {
            VertexFlags::RUDDER_LEFT
        } else {
            VertexFlags::RUDDER_CENTER
        };

        mask |= if self.left_aileron_down() {
            VertexFlags::LEFT_AILERON_DOWN
        } else if self.left_aileron_up() {
            if self.errata.no_upper_aileron {
                VertexFlags::LEFT_AILERON_CENTER
            } else {
                VertexFlags::LEFT_AILERON_UP
            }
        } else {
            VertexFlags::LEFT_AILERON_CENTER
        };

        mask |= if self.right_aileron_down() {
            VertexFlags::RIGHT_AILERON_DOWN
        } else if self.right_aileron_up() {
            if self.errata.no_upper_aileron {
                VertexFlags::RIGHT_AILERON_CENTER
            } else {
                VertexFlags::RIGHT_AILERON_UP
            }
        } else {
            VertexFlags::RIGHT_AILERON_CENTER
        };

        mask |= if self.afterburner_enabled() {
            VertexFlags::AFTERBURNER_ON
        } else {
            VertexFlags::AFTERBURNER_OFF
        };

        mask |= if !self.gear_visible() {
            VertexFlags::GEAR_UP
        } else {
            VertexFlags::GEAR_DOWN
        };

        mask |= if !self.bay_visible() {
            VertexFlags::BAY_CLOSED
        } else {
            VertexFlags::BAY_OPEN
        };

        mask |= match self.sam_count {
            0 => VertexFlags::SAM_COUNT_0,
            1 => VertexFlags::SAM_COUNT_0 | VertexFlags::SAM_COUNT_1,
            2 => VertexFlags::SAM_COUNT_0 | VertexFlags::SAM_COUNT_1 | VertexFlags::SAM_COUNT_2,
            3 => {
                VertexFlags::SAM_COUNT_0
                    | VertexFlags::SAM_COUNT_1
                    | VertexFlags::SAM_COUNT_2
                    | VertexFlags::SAM_COUNT_3
            }
            _ => bail!("expected sam count < 3"),
        };

        mask |= match self.eject_state {
            0 => VertexFlags::EJECT_STATE_0,
            1 => VertexFlags::EJECT_STATE_1,
            2 => VertexFlags::EJECT_STATE_2,
            3 => VertexFlags::EJECT_STATE_3,
            4 => VertexFlags::EJECT_STATE_4,
            _ => bail!("expected eject state in 0..4"),
        };

        mask |= if self.player_dead() {
            VertexFlags::PLAYER_DEAD
        } else {
            VertexFlags::PLAYER_ALIVE
        };

        Ok(mask.bits())
    }
}
