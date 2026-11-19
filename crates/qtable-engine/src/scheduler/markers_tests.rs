//! Deterministic marker playback contracts, based on the accepted v3.4 Live checks.
use super::automation::Automation;
use super::*;
use crate::model::region::Region as Markers;
use crate::model::*;

const B: u64 = 960;
fn markers(start: u64, end: u64, looping: bool) -> Markers {
    Markers {
        start: BeatTime(start * B),
        end: BeatTime(end * B),
        loop_start: BeatTime(4 * B),
        loop_end: BeatTime(8 * B),
        looping,
    }
}
fn fixture(markers: Markers, delay: f64) -> Project {
    let mut p = super::tests::project(delay, markers.looping);
    let hits = (0..=13)
        .map(|at| {
            p.tracks[0].hit(
                BeatTime(at * B),
                OscAddress::Absolute(format!("/at{at}")),
                vec![],
                || DeviceId(1),
            )
        })
        .collect();
    let clip = p.clip_mut(TrackId(1), 0).unwrap();
    clip.events = hits;
    clip.start = markers.start;
    clip.end = markers.end;
    clip.loop_start = markers.loop_start;
    clip.loop_end = markers.loop_end;
    p
}
fn rig(p: Project, horizon_ms: u64) -> (Planner, InternalClock, Instant) {
    let start = Instant::now();
    let mut clock = InternalClock::default();
    clock.play(start);
    let mut planner = Planner::new(p, Duration::from_millis(horizon_ms)).unwrap();
    planner.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
    (planner, clock, start)
}
fn sends(
    planner: &Planner,
    clock: &InternalClock,
    now: Instant,
    start: Instant,
) -> Vec<(u64, String)> {
    planner
        .plan(clock, now)
        .iter()
        .filter_map(|e| match &e.action {
            PlannedAction::Send {
                packet: Some(OscPacket::Message(m)),
                ..
            } => Some((
                e.at.duration_since(start).as_millis() as u64,
                m.addr.clone(),
            )),
            _ => None,
        })
        .collect()
}
fn envelope(p: &mut Project, points: &[(u64, f64)]) -> FieldRef {
    p.tracks[0].mixer_device.volume.output.enabled = true;
    let target = ArgumentRef::volume();
    p.clip_mut(TrackId(1), 0)
        .unwrap()
        .lanes
        .push(Lane::Envelope(AutomationEnvelope {
            region: None,
            target: target.clone(),
            points: points
                .iter()
                .map(|&(t, value)| Breakpoint {
                    time: BeatTime(t * B),
                    value,
                    curve: 0.0,
                })
                .collect(),
        }));
    target.on(TrackId(1))
}
fn jump_beats(planner: &Planner, clock: &InternalClock, now: Instant) -> Vec<u64> {
    planner
        .plan(clock, now)
        .iter()
        .filter_map(|e| match e.action {
            PlannedAction::Automate { .. } => clock.beat_at(e.at).map(|b| b.0 / B),
            _ => None,
        })
        .collect()
}
#[test]
fn intro_hits_play_once_then_only_the_half_open_loop_repeats() {
    let (p, clock, start) = rig(fixture(markers(1, 12, true), 0.0), 6000);
    let actual = sends(&p, &clock, start, start);
    let expected: Vec<_> = [1, 2, 3, 4, 5, 6, 7, 4, 5, 6, 7, 4, 5]
        .into_iter()
        .enumerate()
        .map(|(i, at)| (i as u64 * 500, format!("/at{at}")))
        .collect();
    assert_eq!(actual, expected);
    assert_eq!(p.project.clip(TrackId(1), 0).unwrap().events.len(), 14);
    let voice = &p.voices[0];
    let clip = p.project.clip(TrackId(1), 0).unwrap();
    for (elapsed, position) in [(0, 1), (3, 4), (6, 7), (7, 4), (11, 4), (12, 5)] {
        assert_eq!(
            voice.timeline(clip).position_at(BeatTime(elapsed * B)),
            BeatTime(position * B)
        );
    }
    let mut published = super::follow_queue_tests::published(&p, &clock);
    published.publish(&p, &clock, start + Duration::from_millis(3750));
    assert_eq!(
        published.clip_positions[0].position,
        BeatTime(4 * B + B / 2)
    );
    assert_eq!(
        published.tracks[0].position_at(BeatTime(8 * B)),
        BeatTime(5 * B)
    );
}
#[test]
fn start_inside_the_loop_shortens_only_the_first_pass() {
    let (p, clock, start) = rig(fixture(markers(6, 12, true), 0.0), 3000);
    assert_eq!(
        sends(&p, &clock, start, start),
        [
            (0, "/at6".into()),
            (500, "/at7".into()),
            (1000, "/at4".into()),
            (1500, "/at5".into()),
            (2000, "/at6".into()),
            (2500, "/at7".into()),
            (3000, "/at4".into())
        ]
    );
}
#[test]
fn loop_off_uses_start_to_end_once_and_excludes_the_end_hit() {
    let (p, clock, start) = rig(fixture(markers(5, 10, false), 0.0), 4000);
    assert_eq!(
        sends(&p, &clock, start, start),
        (5..10)
            .enumerate()
            .map(|(i, at)| (i as u64 * 500, format!("/at{at}")))
            .collect::<Vec<_>>()
    );
    assert!(
        p.plan(&clock, start)
            .iter()
            .any(|e| matches!(e.action, PlannedAction::TrackSilent(_))
                && e.at == start + Duration::from_millis(2500))
    );
}
#[test]
fn live_loop_off_continues_to_end_or_stops_immediately_past_end() {
    for (elapsed, end, stop_ms) in [(8, 12, 7500), (6, 6, 3000)] {
        let (mut p, clock, start) = rig(fixture(markers(1, end, true), 0.0), 10000);
        let now = start + Duration::from_millis(elapsed * 500);
        for e in p.plan(&clock, start).into_iter().filter(|e| e.at < now) {
            p.consume(&e);
        }
        let mut edited = (*p.project).clone();
        edited.clip_mut(TrackId(1), 0).unwrap().looping = false;
        p.update_project(edited, &clock, now).unwrap();
        let events = p.plan(&clock, now);
        let stop = events
            .iter()
            .find(|e| matches!(e.action, PlannedAction::TrackSilent(_)))
            .unwrap();
        assert_eq!(stop.at, start + Duration::from_millis(stop_ms));
        if elapsed == 8 {
            assert_eq!(
                sends(&p, &clock, now, start),
                (5..12)
                    .enumerate()
                    .map(|(i, at)| (4000 + i as u64 * 500, format!("/at{at}")))
                    .collect::<Vec<_>>()
            );
        } else {
            assert!(sends(&p, &clock, now, start).is_empty());
        }
    }
}
#[test]
fn loop_off_retains_delayed_intro_and_loop_hits_before_the_switch() {
    let (mut p, clock, start) = rig(fixture(markers(1, 12, true), 1000.0), 10000);
    let now = start + Duration::from_millis(3600);
    for e in p.plan(&clock, start).into_iter().filter(|e| e.at < now) {
        p.consume(&e);
    }
    let mut edited = (*p.project).clone();
    edited.clip_mut(TrackId(1), 0).unwrap().looping = false;
    p.update_project(edited, &clock, now).unwrap();
    let pending = sends(&p, &clock, now, start);
    assert_eq!(
        &pending[..4],
        &[
            (4000, "/at7".into()),
            (4500, "/at4".into()),
            (5000, "/at5".into()),
            (5500, "/at6".into())
        ]
    );
    assert_eq!(pending.last(), Some(&(8000, "/at11".into())));
}
#[test]
fn launch_and_wrap_jumps_sample_start_and_loop_position() {
    let mut project = fixture(markers(1, 12, true), 0.0);
    let target = envelope(&mut project, &[(0, 0.0), (4, 0.4), (8, 0.8), (13, 1.0)]);
    let (p, clock, start) = rig(project, 6000);
    assert_eq!(jump_beats(&p, &clock, start), [0, 7, 11]);
    let mut automation = Automation::default();
    automation.sync(&p, &clock, start);
    for (elapsed, expected) in [(0, 0.1), (6, 0.7), (7, 0.4), (8, 0.5), (11, 0.4)] {
        let at = clock.instant_at(BeatTime(elapsed * B)).unwrap();
        let actual = automation
            .values(at)
            .into_iter()
            .find(|v| v.field == target)
            .unwrap()
            .value;
        assert!(
            (actual - expected).abs() < 1e-12,
            "elapsed={elapsed} {actual} != {expected}"
        );
    }
}
#[test]
fn point_jumps_outside_markers_are_skipped_and_wrap_coalesces_position_points() {
    let mut project = fixture(markers(1, 12, true), 0.0);
    envelope(
        &mut project,
        &[
            (0, 0.0),
            (0, 0.1),
            (4, 0.1),
            (4, 0.4),
            (7, 0.4),
            (7, 0.7),
            (8, 0.7),
            (8, 0.8),
            (10, 0.8),
            (10, 0.9),
        ],
    );
    let (mut p, clock, start) = rig(project, 6000);
    assert_eq!(jump_beats(&p, &clock, start), [0, 3, 6, 7, 10, 11]);
    assert_eq!(
        p.skip_stale_jumps(
            &clock,
            start + Duration::from_millis(6100),
            Duration::from_millis(100)
        ),
        6
    );
}
#[test]
fn linked_follow_counts_the_intro_as_pass_one_and_loop_off_uses_clip_end() {
    for (start, looping, num, den, expected) in [
        (1, true, 1, 1, 7),
        (1, true, 2, 1, 11),
        (1, true, 3, 2, 9),
        (1, true, 1, 2, 3),
        (6, true, 1, 1, 2),
        (6, true, 2, 1, 6),
        (1, false, 9, 1, 11),
    ] {
        let mut project = fixture(markers(start, 12, looping), 0.0);
        project.follow_actions_enabled = true;
        let f = &mut project.clip_mut(TrackId(1), 0).unwrap().follow_actions;
        f.actions.enabled = true;
        f.linked = true;
        f.multiplier = FollowMultiplier::new(num, den).unwrap();
        let (p, _, _) = rig(project, 0);
        // Fractions scale the first pass when less than one; an intro ×1 has no extra loop.
        let expected = if num == 1 && den == 2 {
            7 * B / 2
        } else {
            expected * B
        };
        assert_eq!(p.follow_beat(&p.voices[0]), Some(BeatTime(expected)));
    }
}
#[test]
fn legato_maps_elapsed_takeover_through_the_destination_markers() {
    for looping in [true, false] {
        let mut project = fixture(markers(1, 12, true), 0.0);
        let destination = project.clip_mut(TrackId(1), 1).unwrap();
        destination.start = BeatTime(6 * B);
        destination.end = BeatTime(12 * B);
        destination.loop_start = BeatTime(4 * B);
        destination.loop_end = BeatTime(8 * B);
        destination.looping = looping;
        destination.legato = true;
        let (mut p, _, _) = rig(project, 0);
        p.launch_at(TrackId(1), 1, BeatTime(7 * B)).unwrap();
        let v = p.voices.last().unwrap();
        let clip = p.project.clip(TrackId(1), 1).unwrap();
        assert_eq!(
            v.timeline(clip).position_at(BeatTime(7 * B)),
            BeatTime(if looping { 5 * B } else { 12 * B })
        );
    }
}

#[test]
fn loop_off_linked_follow_moves_to_the_actual_end_of_the_continuation() {
    let mut project = fixture(markers(1, 12, true), 0.0);
    project.follow_actions_enabled = true;
    let f = &mut project.clip_mut(TrackId(1), 0).unwrap().follow_actions;
    f.actions.enabled = true;
    f.linked = true;
    f.multiplier = FollowMultiplier::new(10, 1).unwrap();
    let (mut p, clock, start) = rig(project, 0);
    let now = start + Duration::from_millis(4000);
    let mut edited = (*p.project).clone();
    edited.clip_mut(TrackId(1), 0).unwrap().looping = false;
    p.update_project(edited, &clock, now).unwrap();
    assert_eq!(p.follow_beat(&p.voices[0]), Some(BeatTime(15 * B)));
}

#[test]
fn delayed_wrap_jump_before_loop_off_is_preserved_then_endpoints_use_the_linear_segment() {
    let mut project = fixture(markers(1, 12, true), 1000.0);
    let target = envelope(&mut project, &[(0, 0.0), (4, 0.4), (8, 0.8), (12, 1.0)]);
    let (mut p, clock, start) = rig(project, 10000);
    let now = start + Duration::from_millis(3600);
    for e in p.plan(&clock, start).into_iter().filter(|e| e.at < now) {
        p.consume(&e);
    }
    let mut automation = Automation::default();
    automation.sync(&p, &clock, start);
    let mut edited = (*p.project).clone();
    edited.clip_mut(TrackId(1), 0).unwrap().looping = false;
    p.update_project(edited, &clock, now).unwrap();
    automation.sync(&p, &clock, now);
    let jumps: Vec<_> = p
        .plan(&clock, now)
        .iter()
        .filter(|e| matches!(e.action, PlannedAction::Automate { .. }))
        .map(|e| e.at.duration_since(start).as_millis())
        .collect();
    assert_eq!(jumps, [4500]); // The wrap at musical 3500 ms still owes its delayed send.
    for (ms, expected) in [
        (4000, 0.7),
        (4500, 0.4),
        (5000, 0.5),
        (8000, 0.95),
        (8500, 1.0),
    ] {
        let at = start + Duration::from_millis(ms);
        let values = automation.values(at - Duration::from_millis(1000));
        let value = values
            .into_iter()
            .find(|v| v.field == target)
            .unwrap()
            .value;
        assert!(
            (value - expected).abs() < 1e-10,
            "ms={ms}: {value} expected {expected}"
        );
    }
}

#[test]
fn frame_wakes_ignore_changing_segments_outside_the_markers() {
    let mut project = fixture(markers(6, 12, true), 0.0);
    let target = envelope(
        &mut project,
        &[
            (0, 0.0),
            (2, 0.2),
            (4, 0.2),
            (6, 0.2),
            (8, 0.2),
            (9, 0.9),
            (12, 0.9),
        ],
    );
    let (p, clock, start) = rig(project, 0);
    let mut automation = Automation::default();
    automation.sync(&p, &clock, start);
    assert_eq!(
        automation.next_frame(&target.parameter, start + Duration::from_millis(1), 0.0),
        None
    );
    let mut project = fixture(markers(1, 12, true), 0.0);
    let target = envelope(&mut project, &[(0, 0.0), (4, 0.4), (8, 0.8), (12, 0.8)]);
    let (p, clock, start) = rig(project, 0);
    automation.sync(&p, &clock, start);
    let at = start + Duration::from_millis(3750);
    assert_eq!(automation.next_frame(&target.parameter, at, 0.0), Some(at));
}

#[test]
fn loop_off_rebases_early_sparse_sends_by_time_instead_of_the_old_loop_cursor() {
    let mut project = fixture(markers(1, 12, true), -1000.0);
    project
        .clip_mut(TrackId(1), 0)
        .unwrap()
        .events
        .retain(|e| [4, 5, 10, 11].contains(&(e.position.0 / B)));
    let (mut p, clock, start) = rig(project, 10000);
    let now = start + Duration::from_millis(3200);
    for e in p.plan(&clock, start).into_iter().filter(|e| e.at < now) {
        p.consume(&e);
    }
    let mut edited = (*p.project).clone();
    edited.clip_mut(TrackId(1), 0).unwrap().looping = false;
    p.update_project(edited, &clock, now).unwrap();
    assert_eq!(
        sends(&p, &clock, now, start),
        [(3500, "/at10".into()), (4000, "/at11".into())]
    );
}
