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
use crate::upload::{ShapeErrata, VertexFlags};
use animate::{Animation, LinearAnimationTemplate};
use bitflags::bitflags;
use anyhow::{bail, Result};
use std::time::{Duration, Instant};

const ANIMATION_FRAME_TIME: usize = 166; // ms

const GEAR_ANIMATION_TEMPLATE: LinearAnimationTemplate =
    LinearAnimationTemplate::new(Duration::from_millis(5000), (8192f32, 0f32));

const BAY_ANIMATION_TEMPLATE: LinearAnimationTemplate =
    LinearAnimationTemplate::new(Duration::from_millis(5000), (8192f32, 0f32));

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
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrawState {
    gear_animation: Animation,
    bay_animation: Animation,
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

impl DrawState {
    pub fn new(errata: ShapeErrata) -> Self {
        DrawState {
            gear_animation: Animation::new(&GEAR_ANIMATION_TEMPLATE),
            bay_animation: Animation::new(&BAY_ANIMATION_TEMPLATE),
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

    pub fn gear_retracted(&self) -> bool {
        self.gear_animation.completed_backward()
    }

    pub fn gear_position(&self) -> f32 {
        self.gear_animation.value()
    }

    pub fn bay_closed(&self) -> bool {
        self.bay_animation.completed_backward()
    }

    pub fn bay_position(&self) -> f32 {
        self.bay_animation.value()
    }

    pub fn time_origin(&self) -> &Instant {
        &self.base_time
    }

    pub fn thrust_vector_position(&self) -> f32 {
        f32::from(self.thrust_vector_pos)
    }

    pub fn flaps_down(&self) -> bool {
        self.flags.contains(DrawStateFlags::FLAPS_DOWN)
    }

    pub fn slats_down(&self) -> bool {
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
        (!self.gear_retracted()) as u32
    }

    pub fn x86_gear_position(&self) -> u32 {
        self.gear_position() as u32
    }

    pub fn x86_bay_open(&self) -> u32 {
        (!self.bay_closed()) as u32
    }

    pub fn x86_bay_position(&self) -> u32 {
        self.bay_position() as u32
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

    pub fn player_dead(&self) -> bool {
        self.flags.contains(DrawStateFlags::PLAYER_DEAD)
    }

    pub fn rudder_left(&self) -> bool {
        self.flags.contains(DrawStateFlags::RUDDER_LEFT)
    }

    pub fn rudder_right(&self) -> bool {
        self.flags.contains(DrawStateFlags::RUDDER_RIGHT)
    }

    pub fn left_aileron_down(&self) -> bool {
        self.flags.contains(DrawStateFlags::LEFT_AILERON_DOWN)
    }

    pub fn left_aileron_up(&self) -> bool {
        self.flags.contains(DrawStateFlags::LEFT_AILERON_UP)
    }

    pub fn right_aileron_down(&self) -> bool {
        self.flags.contains(DrawStateFlags::RIGHT_AILERON_DOWN)
    }

    pub fn right_aileron_up(&self) -> bool {
        self.flags.contains(DrawStateFlags::RIGHT_AILERON_UP)
    }

    pub fn toggle_flaps(&mut self) {
        self.flags.toggle(DrawStateFlags::FLAPS_DOWN);
    }

    pub fn toggle_slats(&mut self) {
        self.flags.toggle(DrawStateFlags::SLATS_DOWN);
    }

    pub fn toggle_airbrake(&mut self) {
        self.flags.toggle(DrawStateFlags::AIRBRAKE_EXTENDED);
    }

    pub fn toggle_hook(&mut self) {
        self.flags.toggle(DrawStateFlags::HOOK_EXTENDED);
    }

    pub fn toggle_player_dead(&mut self) {
        self.flags.toggle(DrawStateFlags::PLAYER_DEAD);
    }

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

    pub fn move_rudder_center(&mut self) {
        self.flags.remove(DrawStateFlags::RUDDER_LEFT);
        self.flags.remove(DrawStateFlags::RUDDER_RIGHT);
    }

    pub fn move_rudder_left(&mut self) {
        self.flags.insert(DrawStateFlags::RUDDER_LEFT);
        self.flags.remove(DrawStateFlags::RUDDER_RIGHT);
    }

    pub fn move_rudder_right(&mut self) {
        self.flags.remove(DrawStateFlags::RUDDER_LEFT);
        self.flags.insert(DrawStateFlags::RUDDER_RIGHT);
    }

    pub fn move_stick_center(&mut self) {
        self.flags.remove(DrawStateFlags::LEFT_AILERON_DOWN);
        self.flags.remove(DrawStateFlags::LEFT_AILERON_UP);
        self.flags.remove(DrawStateFlags::RIGHT_AILERON_DOWN);
        self.flags.remove(DrawStateFlags::RIGHT_AILERON_UP);
    }

    pub fn move_stick_forward(&mut self) {}

    pub fn move_stick_backward(&mut self) {}

    pub fn vector_thrust_forward(&mut self) {
        self.thrust_vector_delta = 10;
    }

    pub fn vector_thrust_backward(&mut self) {
        self.thrust_vector_delta = -10;
    }

    pub fn vector_thrust_stop(&mut self) {
        self.thrust_vector_delta = 0;
    }

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

    pub fn move_stick_left(&mut self) {
        self.flags.remove(DrawStateFlags::LEFT_AILERON_DOWN);
        self.flags.insert(DrawStateFlags::LEFT_AILERON_UP);
        self.flags.insert(DrawStateFlags::RIGHT_AILERON_DOWN);
        self.flags.remove(DrawStateFlags::RIGHT_AILERON_UP);
    }

    pub fn move_stick_right(&mut self) {
        self.flags.insert(DrawStateFlags::LEFT_AILERON_DOWN);
        self.flags.remove(DrawStateFlags::LEFT_AILERON_UP);
        self.flags.remove(DrawStateFlags::RIGHT_AILERON_DOWN);
        self.flags.insert(DrawStateFlags::RIGHT_AILERON_UP);
    }

    pub fn consume_sam(&mut self) {
        self.sam_count -= 1;
        if self.sam_count < 0 {
            self.sam_count = 3;
        }
    }

    pub fn toggle_gear(&mut self, start: &Instant) {
        self.gear_animation.start_or_reverse(start);
    }

    pub fn toggle_bay(&mut self, start: &Instant) {
        self.bay_animation.start_or_reverse(start);
    }

    pub fn animate(&mut self, now: &Instant) {
        self.gear_animation.animate(now);
        self.bay_animation.animate(now);
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

        mask |= if self.flaps_down() {
            VertexFlags::LEFT_FLAP_DOWN | VertexFlags::RIGHT_FLAP_DOWN
        } else {
            VertexFlags::LEFT_FLAP_UP | VertexFlags::RIGHT_FLAP_UP
        };

        mask |= if self.slats_down() {
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

        mask |= if self.gear_retracted() {
            VertexFlags::GEAR_UP
        } else {
            VertexFlags::GEAR_DOWN
        };

        mask |= if self.bay_closed() {
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
