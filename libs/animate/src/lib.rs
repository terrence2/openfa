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
use approx::relative_eq;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq)]
struct AnimationRange {
    start: f32,
    end: f32,
}

impl AnimationRange {
    const fn new(start: f32, end: f32) -> Self {
        Self { start, end }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinearAnimationTemplate {
    range: AnimationRange,
    duration: Duration,
}

impl LinearAnimationTemplate {
    pub const fn new(duration: Duration, range: (f32, f32)) -> Self {
        Self {
            range: AnimationRange::new(range.0, range.1),
            duration,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Animation {
    template: LinearAnimationTemplate,
    start: Instant,
    value: f32,
    active: bool,
    forward: bool,
}

impl Animation {
    pub fn new(template: &LinearAnimationTemplate) -> Self {
        Self {
            template: *template,
            start: Instant::now(),
            value: template.range.start,
            active: false,
            forward: true,
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn value(&self) -> f32 {
        self.value
    }

    pub fn animate(&mut self, now: &Instant) {
        assert!(*now >= self.start, "time moved backwards");
        if !self.is_active() {
            return;
        }

        let elapsed = *now - self.start;
        if elapsed > self.template.duration {
            self.value = if self.forward {
                self.template.range.end
            } else {
                self.template.range.start
            };
            self.active = false;
            return;
        }

        let f = elapsed.as_millis() as f32 / self.template.duration.as_millis() as f32;
        self.value = if self.forward {
            self.template.range.start + ((self.template.range.end - self.template.range.start) * f)
        } else {
            self.template.range.end - ((self.template.range.end - self.template.range.start) * f)
        };
    }

    // Starts a stopped animation from template range start to end.
    //
    // Note that if the animation was previously completed forward, this will
    // jump the animation back to its start.
    pub fn start_forward(&mut self, start: &Instant) {
        assert!(!self.active);
        self.start = *start;
        self.value = self.template.range.start;
        self.forward = true;
        self.active = true;
    }

    // Starts a stopped animation from template range end to start.
    //
    // Note that if the animation was previously completed backward, this will
    // jump the animation back to its end.
    pub fn start_backward(&mut self, start: &Instant) {
        assert!(!self.active);
        self.start = *start;
        self.value = self.template.range.end;
        self.forward = false;
        self.active = true;
    }

    // Starts a stopped animation with direction such that the value is not
    // discontinuous. Note that since there is no facility for stopping an
    // animation in the middle, this will always start from the beginning or end
    // an be able to obey duration.
    pub fn start(&mut self, now: &Instant) {
        assert!(!self.active);
        if self.completed_forward() {
            self.start_backward(now);
        } else {
            assert!(self.completed_backward());
            self.start_forward(now);
        }
    }

    pub fn start_or_reverse(&mut self, now: &Instant) {
        if self.active {
            self.reverse_direction(now);
        } else {
            self.start(now);
        }
    }

    // Note that this is not simply reversing direction because we are animating
    // over a fixed interval of time. If we were 90% of the way done and
    // reversed, the reverse animation would take 10% of the time, instead of
    // another 90% of the time.
    pub fn reverse_direction(&mut self, now: &Instant) {
        assert!(self.active);
        self.forward = !self.forward;
        let elapsed = *now - self.start;
        let desired_end = self.start + 2 * elapsed;
        self.start = desired_end - self.template.duration;
    }

    pub fn completed_forward(&self) -> bool {
        !self.active && relative_eq!(self.value, self.template.range.end)
    }

    pub fn completed_backward(&self) -> bool {
        !self.active && relative_eq!(self.value, self.template.range.start)
    }

    pub fn start_position(&self) -> f32 {
        self.template.range.start
    }

    pub fn end_position(&self) -> f32 {
        self.template.range.end
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use approx::assert_relative_eq;

    const TEST_LINEAR_TEMPLATE: LinearAnimationTemplate = LinearAnimationTemplate {
        duration: Duration::from_millis(10),
        range: AnimationRange {
            start: 0f32,
            end: 10f32,
        },
    };

    const TEST_LINEAR_REVERSE_TEMPLATE: LinearAnimationTemplate = LinearAnimationTemplate {
        duration: Duration::from_millis(10),
        range: AnimationRange {
            start: 10f32,
            end: 0f32,
        },
    };

    #[test]
    fn basic_creation() {
        let anim = Animation::new(&TEST_LINEAR_TEMPLATE);
        assert_relative_eq!(anim.start_position(), 0f32);
        assert_relative_eq!(anim.end_position(), 10f32);
    }

    #[test]
    fn run_animation_to_completion() {
        let mut anim = Animation::new(&TEST_LINEAR_TEMPLATE);
        assert!(anim.completed_backward());
        anim.start(&Instant::now());
        assert!(!anim.completed_backward());
        while anim.is_active() {
            anim.animate(&Instant::now());
        }
        assert!(anim.completed_forward());
        assert_relative_eq!(anim.value(), 10f32);
    }

    #[test]
    fn run_reverse_animation_to_completion() {
        let mut anim = Animation::new(&TEST_LINEAR_REVERSE_TEMPLATE);
        assert!(anim.completed_backward());
        anim.start(&Instant::now());
        assert!(!anim.completed_backward());
        while anim.is_active() {
            anim.animate(&Instant::now());
        }
        assert!(anim.completed_forward());
        assert_relative_eq!(anim.value(), 0f32);
    }

    #[test]
    fn restart_animation() {
        let mut anim = Animation::new(&TEST_LINEAR_TEMPLATE);
        assert!(anim.completed_backward());
        anim.start(&Instant::now());
        while anim.is_active() {
            anim.animate(&Instant::now());
        }
        assert_relative_eq!(anim.value(), 10f32);
        anim.start_forward(&Instant::now());
        assert!(!relative_eq!(anim.value(), 10f32));
        while anim.is_active() {
            anim.animate(&Instant::now());
        }
        assert!(anim.completed_forward());
        assert_relative_eq!(anim.value(), 10f32);
    }
}
