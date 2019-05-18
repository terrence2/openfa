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
use std::{
    ops::Range,
    time::{Duration, Instant},
};

#[derive(Debug)]
pub struct Animation {
    start: Instant,
    duration: Duration,
    range: Range<f32>,
    value: f32,
    active: bool,
}

impl Animation {
    pub fn start(start: Instant, duration: Duration, range: Range<f32>) -> Self {
        Self {
            start,
            duration,
            value: range.start,
            range,
            active: true,
        }
    }

    pub fn empty(value: f32) -> Self {
        Self {
            start: Instant::now(),
            duration: Duration::from_millis(0),
            value,
            range: 0f32..0f32,
            active: false,
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn value(&self) -> f32 {
        self.value
    }

    pub fn elapsed(&mut self, now: Instant) -> Option<Duration> {
        if !self.active {
            return None;
        }
        Some(now - self.start)
    }

    pub fn animate(&mut self, now: Instant) -> f32 {
        assert!(now >= self.start, "time moved backwards");
        if !self.active {
            return self.value;
        }
        let elapsed = now - self.start;
        if elapsed > self.duration {
            self.active = false;
            self.value = self.range.end;
            return self.value;
        }

        let f = elapsed.as_millis() as f32 / self.duration.as_millis() as f32;
        self.value = self.range.start + ((self.range.end - self.range.start) * f);
        self.value
    }

    pub fn restart(&mut self, start: Instant) {
        self.start = start;
        self.value = self.range.start;
        self.active = true;
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use approx::{assert_relative_eq, relative_eq};

    #[test]
    fn basic_creation() {
        Animation::start(Instant::now(), Duration::from_millis(10), 0f32..10f32);
    }

    #[test]
    fn run_animation_to_completion() {
        let mut anim = Animation::start(Instant::now(), Duration::from_millis(1), 0f32..42f32);
        while anim.is_active() {
            anim.animate(Instant::now());
        }
        assert_relative_eq!(anim.value(), 42f32);
    }

    #[test]
    fn restart_animation() {
        let mut anim = Animation::start(Instant::now(), Duration::from_millis(1), 0f32..42f32);
        while anim.is_active() {
            anim.animate(Instant::now());
        }
        assert_relative_eq!(anim.value(), 42f32);
        anim.restart(Instant::now());
        assert!(!relative_eq!(anim.value(), 42f32));
        while anim.is_active() {
            anim.animate(Instant::now());
        }
        assert_relative_eq!(anim.value(), 42f32);
    }
}
