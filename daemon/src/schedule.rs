//! Screen-time engine: allowed daily window + daily budget of minutes, with parent-granted extra
//! time. Pure logic (no clock, no I/O) so it is fully unit-testable without a desktop or root; the
//! daemon feeds it the local day-id + minute-of-day and whether the session is active (that wiring,
//! plus Wayland lock/freeze enforcement, is a later slice). Design per the agy architecture consult.

use serde::Deserialize;

/// Current (day-id, minute-of-day) in the machine's LOCAL timezone. day-id is unique per calendar
/// day (only equality matters, for the daily reset); minute-of-day is 0..1440.
pub fn local_day_minute() -> (u64, u32) {
    unsafe {
        let t = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&t, &mut tm).is_null() {
            return (0, 0);
        }
        let day = (tm.tm_year as u64) * 366 + tm.tm_yday.max(0) as u64;
        let minute = (tm.tm_hour.max(0) as u32) * 60 + tm.tm_min.max(0) as u32;
        (day, minute)
    }
}

/// Screen-time policy. Absent/`enabled = false` ⇒ no time limits.
#[derive(Debug, Clone, Deserialize)]
pub struct Schedule {
    #[serde(default)]
    pub enabled: bool,
    /// Daily budget in minutes (`None` = unlimited budget; a window may still apply).
    #[serde(default)]
    pub daily_budget_minutes: Option<u32>,
    /// Allowed window start, minutes from local midnight (e.g. 7*60 = 420). `None` = no lower bound.
    #[serde(default)]
    pub window_start_min: Option<u32>,
    /// Allowed window end, minutes from local midnight (e.g. 20*60+30 = 1230). `None` = no upper bound.
    #[serde(default)]
    pub window_end_min: Option<u32>,
    /// Shell command the daemon runs when the session transitions to BLOCKED (host-side lock hook).
    #[serde(default)]
    pub lock_command: Option<String>,
    /// Shell command run when the session transitions back to ALLOWED (e.g. after a grant).
    #[serde(default)]
    pub unlock_command: Option<String>,
    /// How often (seconds) the daemon ticks usage/status. Default 30.
    #[serde(default = "default_tick_secs")]
    pub tick_secs: u64,
}

fn default_tick_secs() -> u64 {
    30
}

/// What the enforcer should do right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Session may run. `remaining_secs` is `None` when the budget is unlimited.
    Allowed { remaining_secs: Option<u64> },
    /// Current local time is outside the allowed window.
    OutsideWindow,
    /// Inside the window but the daily budget is used up.
    BudgetExhausted,
}

/// Stateful engine. The caller ticks it with the current local day-id and minute-of-day.
#[derive(Debug, Clone)]
pub struct TimeEngine {
    sched: Schedule,
    day: u64,
    used_secs: u64,
    granted_secs: u64,
}

impl TimeEngine {
    pub fn new(sched: Schedule) -> Self {
        Self { sched, day: 0, used_secs: 0, granted_secs: 0 }
    }

    fn roll_day(&mut self, day: u64) {
        if day != self.day {
            self.day = day;
            self.used_secs = 0;
            self.granted_secs = 0;
        }
    }

    fn in_window(&self, minute_of_day: u32) -> bool {
        // MVP supports a normal daytime window (start <= end). Missing bound = open on that side.
        if let Some(s) = self.sched.window_start_min {
            if minute_of_day < s {
                return false;
            }
        }
        if let Some(e) = self.sched.window_end_min {
            if minute_of_day >= e {
                return false;
            }
        }
        true
    }

    /// Total budget for today in seconds (base daily budget + granted extra), or `None` if unlimited.
    fn budget_total_secs(&self) -> Option<u64> {
        self.sched
            .daily_budget_minutes
            .map(|m| m as u64 * 60 + self.granted_secs)
    }

    fn remaining_secs(&self) -> Option<u64> {
        self.budget_total_secs()
            .map(|total| total.saturating_sub(self.used_secs))
    }

    /// Status without advancing time.
    pub fn status(&mut self, day: u64, minute_of_day: u32) -> Status {
        self.roll_day(day);
        if !self.sched.enabled {
            return Status::Allowed { remaining_secs: None }; // no limits
        }
        if !self.in_window(minute_of_day) {
            return Status::OutsideWindow;
        }
        match self.remaining_secs() {
            Some(0) => Status::BudgetExhausted,
            rem => Status::Allowed { remaining_secs: rem },
        }
    }

    /// Advance by `elapsed_secs` of session activity, then report status. Budget only decrements
    /// while the session is `active` and limits are enabled; outside the window nothing is charged.
    pub fn on_tick(&mut self, day: u64, minute_of_day: u32, active: bool, elapsed_secs: u64) -> Status {
        self.roll_day(day);
        if self.sched.enabled && active && self.in_window(minute_of_day) {
            self.used_secs = self.used_secs.saturating_add(elapsed_secs);
        }
        self.status(day, minute_of_day)
    }

    /// Parent grants extra minutes for today (via the signed approval channel).
    pub fn grant_minutes(&mut self, day: u64, minutes: u32) {
        self.roll_day(day);
        self.granted_secs = self.granted_secs.saturating_add(minutes as u64 * 60);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sched(enabled: bool, budget: Option<u32>, start: Option<u32>, end: Option<u32>) -> Schedule {
        Schedule {
            enabled,
            daily_budget_minutes: budget,
            window_start_min: start,
            window_end_min: end,
            lock_command: None,
            unlock_command: None,
            tick_secs: 30,
        }
    }

    #[test]
    fn disabled_is_always_allowed() {
        let mut e = TimeEngine::new(sched(false, Some(1), Some(600), Some(601)));
        assert_eq!(e.status(1, 0), Status::Allowed { remaining_secs: None });
    }

    #[test]
    fn outside_window_blocks() {
        let mut e = TimeEngine::new(sched(true, None, Some(7 * 60), Some(20 * 60)));
        assert_eq!(e.status(1, 6 * 60), Status::OutsideWindow); // 06:00 before 07:00
        assert_eq!(e.status(1, 20 * 60), Status::OutsideWindow); // 20:00 == end is excluded
        assert!(matches!(e.status(1, 12 * 60), Status::Allowed { .. })); // noon inside
    }

    #[test]
    fn budget_counts_down_and_exhausts() {
        let mut e = TimeEngine::new(sched(true, Some(2), None, None)); // 2 min = 120 s
        assert_eq!(e.on_tick(1, 600, true, 60), Status::Allowed { remaining_secs: Some(60) });
        assert_eq!(e.on_tick(1, 600, true, 60), Status::BudgetExhausted);
        // further active ticks stay exhausted
        assert_eq!(e.on_tick(1, 600, true, 60), Status::BudgetExhausted);
    }

    #[test]
    fn idle_does_not_consume_budget() {
        let mut e = TimeEngine::new(sched(true, Some(1), None, None));
        assert_eq!(e.on_tick(1, 600, false, 300), Status::Allowed { remaining_secs: Some(60) });
    }

    #[test]
    fn grant_extends_budget() {
        let mut e = TimeEngine::new(sched(true, Some(1), None, None));
        assert_eq!(e.on_tick(1, 600, true, 60), Status::BudgetExhausted);
        e.grant_minutes(1, 5);
        assert_eq!(e.status(1, 600), Status::Allowed { remaining_secs: Some(5 * 60) });
    }

    #[test]
    fn new_day_resets_usage_and_grants() {
        let mut e = TimeEngine::new(sched(true, Some(1), None, None));
        assert_eq!(e.on_tick(1, 600, true, 60), Status::BudgetExhausted);
        e.grant_minutes(1, 10);
        // next day: fresh budget, granted extra cleared
        assert_eq!(e.status(2, 600), Status::Allowed { remaining_secs: Some(60) });
    }
}
