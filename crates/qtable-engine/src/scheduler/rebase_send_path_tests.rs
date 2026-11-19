//! A playing voice rebases when its track's delay or target availability changes.
use super::tests::project;
use super::*;
use crate::model::*;

const MS: fn(u64) -> Duration = Duration::from_millis;

struct Rig {
    start: Instant,
    clock: InternalClock,
    p: Planner,
}

fn rig() -> Rig {
    let start = Instant::now();
    let mut clock = InternalClock::default();
    clock.play(start);
    let mut p = Planner::new(project(0.0, true), MS(20)).unwrap();
    p.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
    Rig { start, clock, p }
}

impl Rig {
    fn run_to(&mut self, ms: u64) {
        let at = self.start + MS(ms);
        for event in self.p.plan(&self.clock, at) {
            if event.at <= at {
                self.p.consume(&event);
            }
        }
    }

    fn edit(&mut self, ms: u64, change: impl FnOnce(&mut Project)) {
        let mut project = (*self.p.project).clone();
        change(&mut project);
        self.p
            .update_project(project, &self.clock, self.start + MS(ms))
            .unwrap();
    }

    /// Send times planned at `ms` as milliseconds after start (negative: past).
    fn sends(&self, ms: u64, window: u64) -> Vec<i64> {
        let mut p = self.p.clone();
        p.lookahead = MS(window);
        p.plan(&self.clock, self.start + MS(ms))
            .iter()
            .filter(|e| matches!(e.action, PlannedAction::Send { .. }))
            .map(|e| match e.at.checked_duration_since(self.start) {
                Some(after) => after.as_millis() as i64,
                None => -(self.start.duration_since(e.at).as_millis() as i64),
            })
            .collect()
    }
}

#[test]
fn restoring_a_cleared_target_resumes_without_replaying_the_gap() {
    let mut r = rig();
    r.run_to(1000);
    r.edit(1000, |p| p.tracks[0].route = crate::model::Route::Group);
    r.run_to(600_000);
    r.edit(600_000, |p| p.tracks[0].route = Route::single(TrackId(201)));
    // 600 000 ms is a whole number of cycles; the next send is the one at 600 000.
    let planned = r.p.clone().plan(&r.clock, r.start + MS(600_000));
    assert!(planned.len() <= 3, "planned {} events", planned.len());
    let sends = r.sends(600_000, 600);
    assert!(
        !sends.is_empty() && sends.iter().all(|&at| at >= 600_000),
        "{sends:?}"
    );
}

#[test]
fn a_more_negative_delay_skips_events_that_fell_behind() {
    let mut r = rig();
    r.run_to(1000);
    r.edit(1000, |p| p.tracks[0].delay_ms = -300.0);
    let sends = r.sends(1000, 400);
    assert!(sends.iter().all(|&at| at >= 1000), "burst: {sends:?}");
    // Next undispatched event is the cycle-3 e1 at musical 1500, now sent at 1200.
    assert_eq!(sends.first(), Some(&1200), "{sends:?}");
}

#[test]
fn a_longer_delay_does_not_resend_what_already_went_out() {
    let mut r = rig();
    r.run_to(1100);
    r.edit(1100, |p| p.tracks[0].delay_ms = 300.0);
    // Cycle-3 e1 at musical 1000 went out at 1000; next is e0/e2 at 1250 + 300.
    let sends = r.sends(1100, 600);
    assert_eq!(sends.first(), Some(&1550), "{sends:?}");
}

#[test]
fn an_unchanged_send_path_leaves_the_cursor_alone() {
    let mut r = rig();
    r.run_to(1100);
    let before = r.p.voices[0].cursor;
    r.edit(1100, |p| p.tracks[0].name = "renamed".into());
    assert_eq!(r.p.voices[0].cursor, before);
}
