//! Focus mode: the light steps back while the user types or moves the mouse,
//! and comes back once their hands have been off for a while.

use std::time::Duration;

/// How long the light takes to step back once the user starts working.
const DIM: f32 = 0.25;
/// How long it takes to come back once they stop.
const UNDIM: f32 = 0.8;
/// Closer than this to where it is heading, the light is there.
const SETTLED: f32 = 0.002;

/// How often the keyboard and mouse are checked while focus mode is on.
pub const POLL: Duration = Duration::from_millis(100);

/// How much of the light focus mode lets through: `level` while the user's
/// last input is younger than `after`, all of it otherwise.
pub fn target(idle: Duration, after: Duration, level: f32) -> f32 {
    if idle < after { level } else { 1.0 }
}

/// The share of the light shown, moving toward its target instead of jumping.
pub struct Dimmer {
    pub shown: f32,
    target: f32,
}

impl Default for Dimmer {
    fn default() -> Self {
        Dimmer { shown: 1.0, target: 1.0 }
    }
}

impl Dimmer {
    /// Sets where the light is heading. Returns true when that changed, so a
    /// still light knows to draw again.
    pub fn aim(&mut self, target: f32) -> bool {
        let changed = target != self.target;
        self.target = target;
        changed
    }

    pub fn update(&mut self, dt: f32) {
        let tau = if self.target < self.shown { DIM } else { UNDIM };
        self.shown += (self.target - self.shown) * (1.0 - (-dt / tau).exp());
        if self.settled() {
            self.shown = self.target;
        }
    }

    pub fn settled(&self) -> bool {
        (self.shown - self.target).abs() < SETTLED
    }
}

pub use platform::idle;

#[cfg(windows)]
mod platform {
    use std::time::Duration;

    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

    /// Time since the last key press or mouse movement anywhere in the session.
    pub fn idle() -> Duration {
        let mut info = LASTINPUTINFO { cbSize: size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
        // SAFETY: `info` is a valid LASTINPUTINFO with its size filled in.
        if unsafe { GetLastInputInfo(&mut info) } == 0 {
            return Duration::MAX;
        }
        // Both are milliseconds since startup and wrap together after 49 days.
        let now = unsafe { GetTickCount() };
        Duration::from_millis(now.wrapping_sub(info.dwTime) as u64)
    }
}

#[cfg(not(windows))]
mod platform {
    pub fn idle() -> std::time::Duration {
        std::time::Duration::MAX
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_light_steps_back_while_working_and_returns_after_the_wait() {
        let after = Duration::from_secs(5);
        assert_eq!(target(Duration::ZERO, after, 0.3), 0.3);
        assert_eq!(target(Duration::from_millis(4900), after, 0.3), 0.3);
        assert_eq!(target(Duration::from_secs(5), after, 0.3), 1.0);
        assert_eq!(target(Duration::MAX, after, 0.3), 1.0);
    }

    /// Steps the dimmer at 60 fps until it settles; returns the seconds it took.
    fn settle(d: &mut Dimmer) -> f32 {
        let mut t = 0.0;
        while !d.settled() {
            d.update(1.0 / 60.0);
            t += 1.0 / 60.0;
            assert!(t < 10.0, "never settled at {}", d.shown);
        }
        t
    }

    #[test]
    fn the_light_fades_both_ways_and_steps_back_faster_than_it_returns() {
        let mut d = Dimmer::default();
        assert!(d.settled() && d.shown == 1.0);
        assert!(d.aim(0.3));
        assert!(!d.aim(0.3), "the same target again is no change");
        d.update(1.0 / 60.0);
        assert!(d.shown < 1.0 && d.shown > 0.3, "fades instead of jumping: {}", d.shown);
        let down = settle(&mut d);
        assert_eq!(d.shown, 0.3);
        d.aim(1.0);
        let up = settle(&mut d);
        assert_eq!(d.shown, 1.0);
        assert!(down < up, "down {down} s, up {up} s");
        // Measured against the time constants, so a change to either shows.
        assert!((down - DIM * (0.7f32 / SETTLED).ln()).abs() < 0.05, "down {down} s");
        assert!((up - UNDIM * (0.7f32 / SETTLED).ln()).abs() < 0.05, "up {up} s");
    }
}
