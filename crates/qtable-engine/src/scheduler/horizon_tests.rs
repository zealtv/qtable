//! plan-whole-clusters: a plan's horizon extends once, from its first event.
use super::tests::project;
use super::*;
use crate::model::*;

const MS: fn(u64) -> Duration = Duration::from_millis;
const US: fn(u64) -> Duration = Duration::from_micros;

/// A looping one-beat clip (120 BPM: 960 ticks in 500 ms) with one hit at each tick position,
/// launched at the clock's start, with a 20 ms lookahead.
fn rig(positions: &[u64]) -> (Planner, InternalClock, Instant) {
    let mut p = project(0.0, true);
    let track = &mut p.tracks[0];
    let hits: Vec<_> = positions
        .iter()
        .enumerate()
        .map(|(i, &position)| {
            track.hit(
                BeatTime(position),
                OscAddress::Absolute(format!("/h{i}")),
                vec![],
                || DeviceId(1),
            )
        })
        .collect();
    track.slots[0].clip.as_mut().unwrap().events = hits;
    let start = Instant::now();
    let mut clock = InternalClock::default();
    clock.play(start);
    let mut planner = Planner::new(p, MS(20)).unwrap();
    planner.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
    (planner, clock, start)
}

/// Due times of the planned sends, in µs after `start`.
fn sends(events: &[DueEvent], start: Instant) -> Vec<u64> {
    events
        .iter()
        .filter(|e| matches!(e.action, PlannedAction::Send { .. }))
        .map(|e| e.at.duration_since(start).as_micros() as u64)
        .collect()
}

/// Hits at about 50.0, 59.9, 75.0 and 90.1 ms.
const SPREAD: [u64; 4] = [96, 115, 144, 173];

#[test]
fn a_plan_extends_once_from_its_first_event() {
    let (planner, clock, start) = rig(&SPREAD);
    // now + lookahead = 55 ms reaches only the first hit; the horizon then extends to 50 + 20 =
    // 70 ms. It doesn't chain from 59.9 ms, which would reach 75 ms.
    let planned = planner.plan(&clock, start + MS(35));
    assert_eq!(sends(&planned, start), [50_000, 59_895]);
}

#[test]
fn an_overdue_first_event_keeps_the_now_horizon() {
    let (planner, clock, start) = rig(&SPREAD);
    // The 50 ms hit is overdue at 80 ms: the horizon stays at now + lookahead, 100 ms.
    let planned = planner.plan(&clock, start + MS(80));
    assert_eq!(sends(&planned, start), [50_000, 59_895, 75_000, 90_104]);
}

#[test]
fn nothing_found_leaves_the_horizon_alone() {
    let (planner, clock, start) = rig(&SPREAD);
    assert!(planner.plan(&clock, start + MS(10)).is_empty());
}

/// The worker's pattern: plan when nothing is pending, then dispatch each event at its time.
/// Returns when each plan ran.
fn worker_plans(
    planner: &mut Planner,
    clock: &InternalClock,
    start: Instant,
    until: Duration,
) -> Vec<Duration> {
    let mut pending = std::collections::VecDeque::new();
    let mut plans = Vec::new();
    let mut now = start;
    while now < start + until {
        if pending.is_empty() {
            pending = planner.plan(clock, now).into();
            plans.push(now - start);
        }
        while let Some(event) = pending.front().filter(|e| e.at <= now) {
            planner.consume(event);
            pending.pop_front();
        }
        now += US(100);
    }
    plans
}

#[test]
fn a_cluster_shorter_than_the_lookahead_is_planned_before_its_first_hit() {
    // Hits at 50.0, 50.5 and 51.0 ms: before the fix, the plan reaching 50 ms held only the
    // first, and the rest were planned at 50 ms, between deadlines.
    let (mut planner, clock, start) = rig(&[96, 97, 98]);
    let plans = worker_plans(&mut planner, &clock, start, MS(60));
    assert!(
        plans.iter().all(|at| *at < MS(50) || *at >= MS(51)),
        "planned mid-cluster: {plans:?}"
    );
}

#[test]
fn a_tempo_change_between_plan_and_cluster_replans_at_the_new_tempo() {
    let (mut planner, mut clock, start) = rig(&SPREAD);
    let before = sends(&planner.plan(&clock, start + MS(35)), start);
    assert_eq!(before, [50_000, 59_895]);
    // The worker clears its pending events on the command and plans again.
    planner.set_tempo(&mut clock, Tempo::new(240.0).unwrap(), start + MS(40));
    let after = sends(&planner.plan(&clock, start + MS(40)), start);
    assert!(
        after[0] < 50_000,
        "the first hit moves earlier at 240 BPM: {after:?}"
    );
    assert_ne!(before, after[..before.len().min(after.len())]);
}
