//! Fast-forwarding past stale sends must match planning and dropping them one by one.
use super::tests::project;
use super::*;
use crate::model::TrackId;

const BOUND: Duration = Duration::from_millis(50);

fn playing(delay: f64, looping: bool) -> (Instant, InternalClock, Planner) {
    let mut planner = Planner::new(project(delay, looping), Duration::from_millis(12)).unwrap();
    let start = Instant::now();
    let mut clock = InternalClock::default();
    clock.play(start);
    planner.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
    (start, clock, planner)
}

/// What the worker does without the shortcut: plan everything, drop what is stale.
fn drop_one_by_one(planner: &mut Planner, clock: &InternalClock, now: Instant) -> u64 {
    let mut dropped = 0;
    for event in planner.plan(clock, now) {
        let stale = now.saturating_duration_since(event.at) > BOUND;
        if stale && matches!(event.action, PlannedAction::Send { .. }) {
            dropped += 1;
            planner.consume(&event);
        }
    }
    dropped
}

fn upcoming(planner: &Planner, clock: &InternalClock, now: Instant) -> Vec<(Instant, u64, u64)> {
    planner
        .plan(clock, now)
        .iter()
        .map(|e| (e.at, e.voice, e.cursor))
        .collect()
}

#[test]
fn skipping_matches_dropping_for_looping_and_one_shot_clips_with_any_delay() {
    for looping in [true, false] {
        for delay in [0.0, -200.0, 300.0] {
            for stall_ms in [60, 400, 5_000, 61_234] {
                let (start, clock, mut slow) = playing(delay, looping);
                let mut fast = slow.clone();
                let now = start + Duration::from_millis(stall_ms);
                let expected = drop_one_by_one(&mut slow, &clock, now);
                let skipped = fast.skip_stale_sends(&clock, now, BOUND);
                let case = format!("looping {looping}, delay {delay}, stall {stall_ms} ms");
                assert_eq!(skipped, expected, "{case}");
                assert_eq!(
                    upcoming(&fast, &clock, now),
                    upcoming(&slow, &clock, now),
                    "{case}"
                );
            }
        }
    }
}

#[test]
fn skipping_leaves_events_inside_the_bound_and_respects_stops() {
    let (start, clock, mut planner) = playing(0.0, true);
    // The 500 ms loop has events at 0, 250 and 250 ms.
    let now = start + Duration::from_millis(540);
    assert_eq!(planner.skip_stale_sends(&clock, now, BOUND), 3);
    assert_eq!(planner.skip_stale_sends(&clock, now, BOUND), 0);
    let now = start + Duration::from_millis(560);
    assert_eq!(planner.skip_stale_sends(&clock, now, BOUND), 1);
    // A pending stop at 250 ms ends the voice's sends, so later ones were never due.
    let (start, clock, mut planner) = playing(0.0, true);
    planner.stops.insert(
        TrackId(1),
        PendingStop {
            beat: BeatTime(480),
            scene: None,
        },
    );
    let now = start + Duration::from_secs(10);
    assert_eq!(planner.skip_stale_sends(&clock, now, BOUND), 1);
}

#[test]
fn skipping_leaves_voices_with_tempo_deadlines_alone() {
    let (start, clock, mut planner) = playing(0.0, true);
    let id = planner.voices[0].id;
    planner
        .deadlines
        .insert((id, 0, crate::model::TrackId(201)), start);
    let now = start + Duration::from_secs(10);
    assert_eq!(planner.skip_stale_sends(&clock, now, BOUND), 0);
}
