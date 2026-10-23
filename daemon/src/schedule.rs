//! Screen-time engine: allowed daily window + daily budget of minutes, with parent-granted extra
//! time. Pure logic (no clock, no I/O) so it is fully unit-testable without a desktop or root; the
//! daemon feeds it the local day-id + minute-of-day and whether the session is active (that wiring,
//! plus Wayland lock/freeze enforcement, is a later slice). Design per the agy architecture consult.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Current (day-id, minute-of-day) in the machine's LOCAL timezone. day-id is unique per calendar
/// day (only equality matters, for the daily reset); minute-of-day is 0..1440.
pub fn local_day_minute() -> (u64, u32) {
    let (day, _, minute) = local_now();
    (day, minute)
}

/// Current (day-id, weekday, minute-of-day) in LOCAL time. weekday is 0=Sunday .. 6=Saturday
/// (matching libc `tm_wday`), used to pick the per-day schedule.
pub fn local_now() -> (u64, u8, u32) {
    unsafe {
        let t = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&t, &mut tm).is_null() {
            return (0, 0, 0);
        }
        let day = (tm.tm_year as u64) * 366 + tm.tm_yday.max(0) as u64;
        let minute = (tm.tm_hour.max(0) as u32) * 60 + tm.tm_min.max(0) as u32;
        let wday = tm.tm_wday.clamp(0, 6) as u8;
        (day, wday, minute)
    }
}

/// 3-letter key for a weekday (0=Sun). Matches the keys used in `Schedule.days`.
fn day_key(weekday: u8) -> &'static str {
    ["sun", "mon", "tue", "wed", "thu", "fri", "sat"][(weekday % 7) as usize]
}

/// Per-weekday override. Any field left `None` falls back to the top-level default for that field.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DaySchedule {
    #[serde(default)]
    pub budget_minutes: Option<u32>,
    #[serde(default)]
    pub window_start_min: Option<u32>,
    #[serde(default)]
    pub window_end_min: Option<u32>,
}

/// Per-child override of the default schedule (keys under `[schedule.children.<name>]`). Any field
/// left unset inherits the top-level default. `enabled`, the lock/unlock commands and `tick_secs`
/// always come from the default (they're machine-wide, not per child).
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChildSchedule {
    #[serde(default)]
    pub daily_budget_minutes: Option<u32>,
    #[serde(default)]
    pub window_start_min: Option<u32>,
    #[serde(default)]
    pub window_end_min: Option<u32>,
    #[serde(default)]
    pub quiet_start_min: Option<u32>,
    #[serde(default)]
    pub quiet_end_min: Option<u32>,
    #[serde(default)]
    pub days: HashMap<String, DaySchedule>,
}

/// Screen-time policy. Absent/`enabled = false` ⇒ no time limits. The top-level budget/window are the
/// DEFAULT for every day; `days` overrides per weekday (keys "mon".."sun"); `quiet_*` is a nightly
/// curfew (blackout) that blocks regardless of budget and may wrap past midnight. `children` holds
/// per-child overrides so several kids on one machine each get their own budget/window/curfew.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Schedule {
    #[serde(default)]
    pub enabled: bool,
    /// Default daily budget in minutes (`None` = unlimited; a window/curfew may still apply).
    #[serde(default)]
    pub daily_budget_minutes: Option<u32>,
    /// Default allowed-window start, minutes from local midnight (e.g. 7*60 = 420). `None` = open.
    #[serde(default)]
    pub window_start_min: Option<u32>,
    /// Default allowed-window end, minutes from local midnight (e.g. 20*60+30 = 1230). `None` = open.
    #[serde(default)]
    pub window_end_min: Option<u32>,
    /// Per-weekday overrides, keyed "mon","tue","wed","thu","fri","sat","sun". Missing ⇒ use defaults.
    #[serde(default)]
    pub days: HashMap<String, DaySchedule>,
    /// Per-child overrides, keyed by child username (`[schedule.children.<name>]`). Missing ⇒ default.
    #[serde(default)]
    pub children: HashMap<String, ChildSchedule>,
    /// Nightly curfew ("cisza nocna") start, minutes from midnight. Blocks regardless of budget.
    #[serde(default)]
    pub quiet_start_min: Option<u32>,
    /// Nightly curfew end, minutes from midnight. If end < start the curfew wraps past midnight
    /// (e.g. start=21:00, end=07:00 blocks 21:00→07:00).
    #[serde(default)]
    pub quiet_end_min: Option<u32>,
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

impl Schedule {
    /// The effective schedule for one child: the default with that child's overrides applied. Machine-
    /// wide fields (enabled / lock / unlock / tick_secs) come from the default; the returned schedule
    /// has an empty `children` map (it describes a single child).
    pub fn effective_for(&self, child: &str) -> Schedule {
        match self.children.get(child) {
            Some(c) => self.with_child_override(c),
            None => {
                let mut s = self.clone();
                s.children = HashMap::new();
                s
            }
        }
    }

    /// The default schedule with one child-override overlaid (used for config AND for a schedule the
    /// parent pushes from the app at runtime). Machine-wide fields are kept from the default.
    pub fn with_child_override(&self, c: &ChildSchedule) -> Schedule {
        let mut s = self.clone();
        s.children = HashMap::new();
        if c.daily_budget_minutes.is_some() {
            s.daily_budget_minutes = c.daily_budget_minutes;
        }
        if c.window_start_min.is_some() {
            s.window_start_min = c.window_start_min;
        }
        if c.window_end_min.is_some() {
            s.window_end_min = c.window_end_min;
        }
        if c.quiet_start_min.is_some() {
            s.quiet_start_min = c.quiet_start_min;
        }
        if c.quiet_end_min.is_some() {
            s.quiet_end_min = c.quiet_end_min;
        }
        if !c.days.is_empty() {
            s.days = c.days.clone();
        }
        s
    }
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

    /// Effective (start, end) allowed window for `weekday`: the per-day override field if set, else
    /// the top-level default. `None` on a side = open on that side.
    fn window_for(&self, weekday: u8) -> (Option<u32>, Option<u32>) {
        let d = self.sched.days.get(day_key(weekday));
        let start = d.and_then(|x| x.window_start_min).or(self.sched.window_start_min);
        let end = d.and_then(|x| x.window_end_min).or(self.sched.window_end_min);
        (start, end)
    }

    /// Effective daily budget in minutes for `weekday` (per-day override, else default).
    fn budget_minutes_for(&self, weekday: u8) -> Option<u32> {
        self.sched
            .days
            .get(day_key(weekday))
            .and_then(|x| x.budget_minutes)
            .or(self.sched.daily_budget_minutes)
    }

    fn in_window(&self, weekday: u8, minute_of_day: u32) -> bool {
        let (start, end) = self.window_for(weekday);
        if let Some(s) = start {
            if minute_of_day < s {
                return false;
            }
        }
        if let Some(e) = end {
            if minute_of_day >= e {
                return false;
            }
        }
        true
    }

    /// Nightly curfew ("cisza nocna"): true when the minute is inside [quiet_start, quiet_end),
    /// wrapping past midnight when end < start (e.g. 21:00→07:00).
    fn in_quiet(&self, minute_of_day: u32) -> bool {
        match (self.sched.quiet_start_min, self.sched.quiet_end_min) {
            (Some(s), Some(e)) => {
                if s <= e {
                    minute_of_day >= s && minute_of_day < e
                } else {
                    minute_of_day >= s || minute_of_day < e // wraps past midnight
                }
            }
            _ => false,
        }
    }

    /// Allowed to be on right now (ignoring budget): inside the window AND not in the curfew.
    fn in_allowed_time(&self, weekday: u8, minute_of_day: u32) -> bool {
        self.in_window(weekday, minute_of_day) && !self.in_quiet(minute_of_day)
    }

    /// Total budget for today in seconds (today's budget + granted extra), or `None` if unlimited.
    fn budget_total_secs(&self, weekday: u8) -> Option<u64> {
        self.budget_minutes_for(weekday)
            .map(|m| m as u64 * 60 + self.granted_secs)
    }

    fn remaining_secs(&self, weekday: u8) -> Option<u64> {
        self.budget_total_secs(weekday)
            .map(|total| total.saturating_sub(self.used_secs))
    }

    /// Status without advancing time.
    pub fn status(&mut self, day: u64, weekday: u8, minute_of_day: u32) -> Status {
        self.roll_day(day);
        if !self.sched.enabled {
            return Status::Allowed { remaining_secs: None }; // no limits
        }
        if !self.in_allowed_time(weekday, minute_of_day) {
            return Status::OutsideWindow;
        }
        match self.remaining_secs(weekday) {
            Some(0) => Status::BudgetExhausted,
            rem => Status::Allowed { remaining_secs: rem },
        }
    }

    /// Advance by `elapsed_secs` of session activity, then report status. Budget only decrements
    /// while the session is `active` and limits are enabled; outside allowed time nothing is charged.
    pub fn on_tick(&mut self, day: u64, weekday: u8, minute_of_day: u32, active: bool, elapsed_secs: u64) -> Status {
        self.roll_day(day);
        if self.sched.enabled && active && self.in_allowed_time(weekday, minute_of_day) {
            self.used_secs = self.used_secs.saturating_add(elapsed_secs);
        }
        self.status(day, weekday, minute_of_day)
    }

    /// Replace the schedule at runtime (a parent pushed a new one from the app). Keeps today's used /
    /// granted minutes so a mid-day change doesn't reset the budget.
    pub fn set_schedule(&mut self, sched: Schedule) {
        self.sched = sched;
    }

    /// Parent grants extra minutes for today (via the signed approval channel).
    pub fn grant_minutes(&mut self, day: u64, minutes: u32) {
        self.roll_day(day);
        self.granted_secs = self.granted_secs.saturating_add(minutes as u64 * 60);
    }

    /// Persisted state (day, used_secs, granted_secs) so a reboot doesn't reset the budget (H2).
    pub fn snapshot(&self) -> (u64, u64, u64) {
        (self.day, self.used_secs, self.granted_secs)
    }

    /// Restore previously-persisted usage. A later `roll_day` with a new day still resets correctly.
    pub fn restore(&mut self, day: u64, used_secs: u64, granted_secs: u64) {
        self.day = day;
        self.used_secs = used_secs;
        self.granted_secs = granted_secs;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MON: u8 = 1;

    fn sched(enabled: bool, budget: Option<u32>, start: Option<u32>, end: Option<u32>) -> Schedule {
        Schedule {
            enabled,
            daily_budget_minutes: budget,
            window_start_min: start,
            window_end_min: end,
            days: HashMap::new(),
            children: HashMap::new(),
            quiet_start_min: None,
            quiet_end_min: None,
            lock_command: None,
            unlock_command: None,
            tick_secs: 30,
        }
    }

    #[test]
    fn disabled_is_always_allowed() {
        let mut e = TimeEngine::new(sched(false, Some(1), Some(600), Some(601)));
        assert_eq!(e.status(1, MON, 0), Status::Allowed { remaining_secs: None });
    }

    #[test]
    fn outside_window_blocks() {
        let mut e = TimeEngine::new(sched(true, None, Some(7 * 60), Some(20 * 60)));
        assert_eq!(e.status(1, MON, 6 * 60), Status::OutsideWindow); // 06:00 before 07:00
        assert_eq!(e.status(1, MON, 20 * 60), Status::OutsideWindow); // 20:00 == end is excluded
        assert!(matches!(e.status(1, MON, 12 * 60), Status::Allowed { .. })); // noon inside
    }

    #[test]
    fn budget_counts_down_and_exhausts() {
        let mut e = TimeEngine::new(sched(true, Some(2), None, None)); // 2 min = 120 s
        assert_eq!(e.on_tick(1, MON, 600, true, 60), Status::Allowed { remaining_secs: Some(60) });
        assert_eq!(e.on_tick(1, MON, 600, true, 60), Status::BudgetExhausted);
        assert_eq!(e.on_tick(1, MON, 600, true, 60), Status::BudgetExhausted);
    }

    #[test]
    fn idle_does_not_consume_budget() {
        let mut e = TimeEngine::new(sched(true, Some(1), None, None));
        assert_eq!(e.on_tick(1, MON, 600, false, 300), Status::Allowed { remaining_secs: Some(60) });
    }

    #[test]
    fn grant_extends_budget() {
        let mut e = TimeEngine::new(sched(true, Some(1), None, None));
        assert_eq!(e.on_tick(1, MON, 600, true, 60), Status::BudgetExhausted);
        e.grant_minutes(1, 5);
        assert_eq!(e.status(1, MON, 600), Status::Allowed { remaining_secs: Some(5 * 60) });
    }

    #[test]
    fn new_day_resets_usage_and_grants() {
        let mut e = TimeEngine::new(sched(true, Some(1), None, None));
        assert_eq!(e.on_tick(1, MON, 600, true, 60), Status::BudgetExhausted);
        e.grant_minutes(1, 10);
        assert_eq!(e.status(2, MON, 600), Status::Allowed { remaining_secs: Some(60) });
    }

    #[test]
    fn per_day_budget_override() {
        // Default 120 min, but Saturday (wday 6) gets 200 min.
        let mut s = sched(true, Some(120), None, None);
        s.days.insert("sat".into(), DaySchedule { budget_minutes: Some(200), ..Default::default() });
        let mut e = TimeEngine::new(s);
        const SAT: u8 = 6;
        assert_eq!(e.status(1, SAT, 600), Status::Allowed { remaining_secs: Some(200 * 60) });
        assert_eq!(e.status(1, MON, 600), Status::Allowed { remaining_secs: Some(120 * 60) });
    }

    #[test]
    fn per_day_window_override() {
        // Default window 07:00–20:00; Friday (wday 5) extended to 22:00.
        let mut s = sched(true, None, Some(7 * 60), Some(20 * 60));
        s.days.insert("fri".into(), DaySchedule { window_end_min: Some(22 * 60), ..Default::default() });
        let mut e = TimeEngine::new(s);
        const FRI: u8 = 5;
        assert_eq!(e.status(1, MON, 21 * 60), Status::OutsideWindow); // Mon 21:00 blocked
        assert!(matches!(e.status(1, FRI, 21 * 60), Status::Allowed { .. })); // Fri 21:00 allowed
    }

    #[test]
    fn quiet_hours_wrap_midnight() {
        // Curfew 21:00 → 07:00, no other limits.
        let mut s = sched(true, None, None, None);
        s.quiet_start_min = Some(21 * 60);
        s.quiet_end_min = Some(7 * 60);
        let mut e = TimeEngine::new(s);
        assert_eq!(e.status(1, MON, 22 * 60), Status::OutsideWindow); // 22:00 in curfew
        assert_eq!(e.status(1, MON, 3 * 60), Status::OutsideWindow);  // 03:00 in curfew (wrapped)
        assert!(matches!(e.status(1, MON, 12 * 60), Status::Allowed { .. })); // noon fine
    }

    #[test]
    fn effective_for_applies_child_override() {
        let mut s = sched(true, Some(120), Some(7 * 60), Some(20 * 60));
        s.children.insert("zosia".into(), ChildSchedule {
            daily_budget_minutes: Some(60),
            quiet_start_min: Some(21 * 60),
            quiet_end_min: Some(7 * 60),
            ..Default::default()
        });
        // Zosia: tighter budget + a curfew; window inherited from the default.
        let z = s.effective_for("zosia");
        assert_eq!(z.daily_budget_minutes, Some(60));
        assert_eq!(z.window_start_min, Some(7 * 60));
        assert_eq!(z.quiet_start_min, Some(21 * 60));
        // An unknown child just gets the defaults.
        let d = s.effective_for("someone-else");
        assert_eq!(d.daily_budget_minutes, Some(120));
        assert_eq!(d.quiet_start_min, None);
    }

    #[test]
    fn quiet_hours_do_not_charge_budget() {
        // During curfew an active tick must not drain the budget (it's blocked anyway).
        let mut s = sched(true, Some(60), None, None);
        s.quiet_start_min = Some(21 * 60);
        s.quiet_end_min = Some(7 * 60);
        let mut e = TimeEngine::new(s);
        e.on_tick(1, MON, 23 * 60, true, 600); // 10 min of "activity" during curfew
        assert_eq!(e.status(1, MON, 12 * 60), Status::Allowed { remaining_secs: Some(60 * 60) });
    }
}
