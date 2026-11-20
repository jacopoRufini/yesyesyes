//! Turns yesyesyes off after a while with no request, so it is not left answering by mistake.

use crate::state::State;
use chrono::{Local, Timelike};
use eframe::egui;
use std::time::{Duration, Instant};

/// The choices of the window, in minutes; 0: never.
pub const CHOICES: [u32; 8] = [5, 15, 30, 60, 120, 240, 480, 0];

const TICK: Duration = Duration::from_secs(1);

#[derive(Default)]
struct Timer {
    /// When yesyesyes was last turned on, or answered its last request since.
    since: Option<Instant>,
}

impl Timer {
    /// Whether to turn off now, and the deadline while on. `activity`: the last request recorded.
    fn step(&mut self, enabled: bool, minutes: u32, activity: Option<Instant>, now: Instant) -> (bool, Option<Instant>) {
        if !enabled {
            self.since = None;
            return (false, None);
        }
        let on = *self.since.get_or_insert(now);
        let since = activity.map_or(on, |a| a.max(on));
        self.since = Some(since);
        if minutes == 0 {
            return (false, None);
        }
        let deadline = since + Duration::from_secs(u64::from(minutes) * 60);
        if now >= deadline {
            self.since = None;
            return (true, None);
        }
        (false, Some(deadline))
    }
}

pub fn start(state: State, ctx: egui::Context) {
    std::thread::spawn(move || {
        let mut timer = Timer::default();
        loop {
            let (enabled, minutes, activity, shown) = {
                let shared = state.lock();
                (shared.config.enabled, shared.config.auto_off_minutes, shared.last_activity, shared.off_at)
            };
            let now = Instant::now();
            let (turn_off, deadline) = timer.step(enabled, minutes, activity, now);
            if turn_off {
                state.update(|c| c.enabled = false);
            }
            // Rounded to the second, so the time shown does not change at every tick.
            let off_at = deadline.map(|d| (Local::now() + (d - now)).with_nanosecond(0).unwrap_or_else(Local::now));
            if turn_off || off_at != shown {
                state.lock().off_at = off_at;
                ctx.request_repaint();
            }
            std::thread::sleep(TICK);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turns_off_after_the_minutes_from_being_turned_on() {
        let mut timer = Timer::default();
        let start = Instant::now();
        let (off, deadline) = timer.step(true, 15, None, start);
        assert!(!off);
        assert_eq!(deadline, Some(start + Duration::from_secs(900)));
        assert!(!timer.step(true, 15, None, start + Duration::from_secs(899)).0);
        assert_eq!(timer.step(true, 15, None, start + Duration::from_secs(900)), (true, None));
    }

    #[test]
    fn turning_on_again_starts_over() {
        let mut timer = Timer::default();
        let start = Instant::now();
        timer.step(true, 15, None, start);
        timer.step(false, 15, None, start + Duration::from_secs(600));
        let again = start + Duration::from_secs(700);
        assert_eq!(timer.step(true, 15, None, again).1, Some(again + Duration::from_secs(900)));
    }

    #[test]
    fn each_request_starts_the_wait_over() {
        let mut timer = Timer::default();
        let start = Instant::now();
        timer.step(true, 15, None, start);
        let request = start + Duration::from_secs(600);
        let (off, deadline) = timer.step(true, 15, Some(request), start + Duration::from_secs(900));
        assert!(!off, "a request 5 minutes ago");
        assert_eq!(deadline, Some(request + Duration::from_secs(900)));
        assert_eq!(timer.step(true, 15, Some(request), request + Duration::from_secs(900)), (true, None));
        let before = start - Duration::from_secs(60);
        let again = request + Duration::from_secs(1000);
        assert_eq!(timer.step(true, 15, Some(before), again).1, Some(again + Duration::from_secs(900)), "a request from before it was turned on");
    }

    #[test]
    fn never_turns_off_with_zero() {
        let mut timer = Timer::default();
        let start = Instant::now();
        assert_eq!(timer.step(true, 0, None, start), (false, None));
        assert_eq!(timer.step(true, 0, None, start + Duration::from_secs(86_400)), (false, None));
    }
}
