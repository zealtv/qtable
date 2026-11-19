//! Accepted v3.4 unlinked-envelope contracts, in deterministic musical time.
use super::automation::Automation;
use super::*;
use crate::edit::{LaneEdit, LaneKey, ProjectEdit};
use crate::model::region::Region as Markers;
use crate::model::*;

const B: u64 = 960;
fn region(start: u64, end: u64, loop_start: u64, loop_end: u64, looping: bool) -> Markers {
    Markers {
        start: BeatTime(start * B),
        end: BeatTime(end * B),
        loop_start: BeatTime(loop_start * B),
        loop_end: BeatTime(loop_end * B),
        looping,
    }
}
fn fixture(own: Option<Markers>, points: &[(u64, f64)], delay: f64) -> Project {
    let mut p = super::tests::project(delay, true);
    p.tracks[0].mixer_device.volume.output.enabled = true;
    let clip = p.clip_mut(TrackId(1), 0).unwrap();
    clip.end = BeatTime(16 * B);
    clip.loop_end = BeatTime(16 * B);
    clip.lanes.push(Lane::Envelope(AutomationEnvelope {
        target: ArgumentRef::volume(),
        region: own,
        points: points
            .iter()
            .map(|&(t, value)| Breakpoint {
                time: BeatTime(t * B),
                value,
                curve: 0.0,
            })
            .collect(),
    }));
    p
}
fn rig(p: Project) -> (Planner, InternalClock, Automation, Instant) {
    let start = Instant::now();
    let mut clock = InternalClock::default();
    clock.play(start);
    let mut p = Planner::new(p, Duration::from_secs(10)).unwrap();
    p.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
    let mut automation = Automation::default();
    automation.sync(&p, &clock, start);
    (p, clock, automation, start)
}
fn value(a: &Automation, clock: &InternalClock, ticks: u64) -> (f64, AutomationState) {
    let values = a.values(clock.instant_at(BeatTime(ticks)).unwrap());
    assert_eq!(values.len(), 1);
    (values[0].value, values[0].state)
}
fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-12, "{actual} != {expected}");
}
fn jumps(p: &Planner, c: &InternalClock, now: Instant) -> Vec<u64> {
    p.plan(c, now)
        .iter()
        .filter_map(|e| match e.action {
            PlannedAction::Automate { .. } => Some(c.beat_at(e.at).unwrap().0),
            _ => None,
        })
        .collect()
}
fn set_region(
    p: &mut Planner,
    c: &InternalClock,
    a: &mut Automation,
    at: Instant,
    region: Option<Markers>,
) {
    let mut project = (*p.project).clone();
    let target = project.clip(TrackId(1), 0).unwrap().lanes[0]
        .envelope()
        .unwrap()
        .target
        .clone();
    crate::edit::apply(
        &mut project,
        ProjectEdit::EditLane {
            track: TrackId(1),
            slot: 0,
            edit: LaneEdit::SetRegion {
                lane: LaneKey::Envelope(target),
                region,
            },
        },
    )
    .unwrap();
    p.update_project(project, c, at).unwrap();
    a.sync(p, c, at);
}

#[test]
fn unlinked_three_beat_loop_runs_across_a_sixteen_beat_clip_wrap() {
    let (p, c, a, start) = rig(fixture(
        Some(region(2, 8, 2, 5, true)),
        &[(0, 0.0), (10, 1.0)],
        0.0,
    ));
    for (beat, expected) in [
        (0, 0.2),
        (2, 0.4),
        (3, 0.2),
        (15, 0.2),
        (16, 0.3),
        (18, 0.2),
    ] {
        close(value(&a, &c, beat * B).0, expected);
    }
    assert_eq!(
        jumps(&p, &c, start),
        (0..=6).map(|i| i * 3 * B).collect::<Vec<_>>()
    );
    // Frames ask the lane's changing segment, even after the clip's own wrap.
    let key = a.values(start)[0].field.parameter.clone();
    let at = c.instant_at(BeatTime(16 * B + B / 2)).unwrap();
    assert_eq!(a.next_frame(&key, at, 0.0), Some(at));
    assert_eq!(
        p.project.clip(TrackId(1), 0).unwrap().region(),
        region(0, 16, 0, 16, true)
    );
}

#[test]
fn unlinked_loop_longer_than_the_clip_loop_runs_on_and_publishes_its_own_position() {
    // The v3.4 tour (2026-10-26): a lane loop longer than the clip's must not reset at the
    // clip's wrap. The clip loops 0-4; the lane loops 0-8 over a 0->1 ramp.
    let mut project = fixture(Some(region(0, 8, 0, 8, true)), &[(0, 0.0), (8, 1.0)], 0.0);
    let clip = project.clip_mut(TrackId(1), 0).unwrap();
    clip.end = BeatTime(4 * B);
    clip.loop_end = BeatTime(4 * B);
    let (p, c, a, start) = rig(project);
    for (beat, expected) in [
        (3, 0.375),
        (4, 0.5),
        (6, 0.75),
        (7, 0.875),
        (8, 0.0),
        (12, 0.5),
    ] {
        close(value(&a, &c, beat * B).0, expected);
    }
    assert_eq!(
        jumps(&p, &c, start),
        (0..=2).map(|i| i * 8 * B).collect::<Vec<_>>()
    );
    let published = super::follow_queue_tests::published(&p, &c);
    let track = &published.tracks[0];
    for (beat, clip, lane) in [(3, 3, 3), (6, 2, 6), (9, 1, 1), (14, 2, 6)] {
        let at = BeatTime(beat * B);
        assert_eq!(track.position_at(at), BeatTime(clip * B), "clip at {beat}");
        let lanes: Vec<_> = track.lane_positions_at(at).collect();
        assert_eq!(lanes.len(), 1);
        assert_eq!(lanes[0].position, BeatTime(lane * B), "lane at {beat}");
    }
}

#[test]
fn unlinked_intro_plays_once_then_own_brace_repeats_and_wakes_after_flats() {
    let (p, c, a, start) = rig(fixture(
        Some(region(1, 9, 4, 7, true)),
        &[(1, 0.1), (2, 0.2), (7, 0.2), (9, 0.9)],
        0.0,
    ));
    assert_eq!(jumps(&p, &c, start), [0]);
    let key = a.values(start)[0].field.parameter.clone();
    // Intro ramp has finished; the brace is entirely flat with matching wrap values.
    let at = c.instant_at(BeatTime(2 * B)).unwrap();
    assert!(a.next_frame(&key, at, 0.0).is_none());
    close(value(&a, &c, 12 * B).0, 0.2);
}

#[test]
fn unlinked_one_shot_holds_its_end_without_stopping_the_clip_or_waking_again() {
    let (p, c, a, start) = rig(fixture(
        Some(region(2, 5, 0, 8, false)),
        &[(0, 0.0), (10, 1.0)],
        0.0,
    ));
    close(value(&a, &c, 0).0, 0.2);
    close(value(&a, &c, 2 * B).0, 0.4);
    for beat in [3, 4, 16, 20] {
        let (v, state) = value(&a, &c, beat * B);
        close(v, 0.5);
        assert_eq!(state, AutomationState::Held);
    }
    assert_eq!(jumps(&p, &c, start), [0]);
    let key = a.values(start)[0].field.parameter.clone();
    let end = c.instant_at(BeatTime(3 * B)).unwrap();
    assert_eq!(a.next_frame(&key, end, 0.0), Some(end));
    assert!(
        a.next_frame(&key, end + Duration::from_nanos(1), 0.0)
            .is_none()
    );
    assert!(!p.voices[0].silent);
}

#[test]
fn unlinked_lane_takes_the_clip_phase_on_a_legato_take_over_as_live_does() {
    // Live 12: on a legato take-over an
    // unlinked envelope continues with the phase a launch at the clip's Start would have given it,
    // and with its Loop on wraps even past its own End; a plain launch restarts it at its Start.
    // Open: qtable's legato carries the donor's unwrapped elapsed time, but Live's runs only took
    // over within the donor's first pass, so later passes are unchecked against Live.
    let lane = |legato| {
        let mut project = fixture(Some(region(1, 3, 1, 3, true)), &[(0, 0.0), (10, 1.0)], 0.0);
        project.clip_mut(TrackId(1), 0).unwrap().legato = legato;
        rig(project)
    };
    let (_, plain_clock, plain, _) = lane(false);
    // Clip position 2 lies inside the lane's loop; clip position 5 lies past its End.
    for (take_over, first) in [(2, 0.1), (5, 0.2)] {
        let (mut p, c, mut a, _) = lane(true);
        p.launch_at(TrackId(1), 0, BeatTime(take_over * B)).unwrap();
        a.sync(&p, &c, c.instant_at(BeatTime(take_over * B)).unwrap());
        let voice = p.voices.last().unwrap();
        assert!(voice.origin < voice.launch);
        close(value(&a, &c, take_over * B).0, first);
        for ticks in (take_over * B..(take_over + 8) * B).step_by(B as usize / 4) {
            close(value(&a, &c, ticks).0, value(&plain, &plain_clock, ticks).0);
        }
    }
    // A plain retrigger still starts the lane at its own Start.
    let (mut p, c, mut a, _) = lane(false);
    p.launch_at(TrackId(1), 0, BeatTime(5 * B)).unwrap();
    a.sync(&p, &c, c.instant_at(BeatTime(5 * B)).unwrap());
    close(value(&a, &c, 5 * B).0, 0.1);
    close(value(&a, &c, 5 * B + B / 2).0, 0.15);
    close(value(&a, &c, 7 * B).0, 0.1);
}

#[test]
fn unlinked_edit_copies_clip_markers_and_relink_uses_clip_time_preserving_points() {
    let (mut p, c, mut a, _) = rig(fixture(None, &[(0, 0.0), (20, 1.0)], 0.0));
    let at = c.instant_at(BeatTime(4 * B)).unwrap();
    let markers = p.project.clip(TrackId(1), 0).unwrap().region();
    set_region(&mut p, &c, &mut a, at, Some(markers));
    close(value(&a, &c, 4 * B).0, 0.2);
    set_region(&mut p, &c, &mut a, at, Some(region(2, 8, 2, 5, true)));
    close(value(&a, &c, 4 * B).0, 0.15);
    let points = p.project.clip(TrackId(1), 0).unwrap().lanes[0]
        .envelope()
        .unwrap()
        .points
        .clone();
    set_region(&mut p, &c, &mut a, at, None);
    close(value(&a, &c, 4 * B).0, 0.2);
    assert_eq!(
        p.project.clip(TrackId(1), 0).unwrap().lanes[0]
            .envelope()
            .unwrap()
            .points,
        points
    );
}

#[test]
fn unlinked_point_jumps_and_wraps_follow_own_bounds_and_stale_catch_up() {
    let points = [
        (0, 0.0),
        (0, 0.1),
        (2, 0.2),
        (2, 0.3),
        (3, 0.4),
        (3, 0.5),
        (5, 0.6),
        (5, 0.7),
        (8, 0.8),
        (8, 0.9),
    ];
    let (mut p, c, _, start) = rig(fixture(Some(region(1, 9, 2, 5, true)), &points, 0.0));
    assert_eq!(
        jumps(&p, &c, start),
        [0, 1, 2, 4, 5, 7, 8, 10, 11, 13, 14, 16, 17, 19, 20].map(|b| b * B)
    );
    let now = c.instant_at(BeatTime(100_000 * B)).unwrap();
    assert!(p.skip_stale_jumps(&c, now, Duration::ZERO) > 30_000);
    assert_eq!(p.skip_stale_jumps(&c, now, Duration::ZERO), 0);
    assert!(jumps(&p, &c, now).iter().all(|&b| b >= 100_000 * B));
}

#[test]
fn unlinked_live_loop_off_continues_to_own_end_and_keeps_delayed_wraps() {
    let (mut p, c, mut a, start) = rig(fixture(
        Some(region(2, 8, 2, 5, true)),
        &[(0, 0.0), (10, 1.0)],
        1000.0,
    ));
    let at = c.instant_at(BeatTime(4 * B)).unwrap();
    for e in p.plan(&c, start).into_iter().filter(|e| e.at < at) {
        p.consume(&e);
    }
    set_region(&mut p, &c, &mut a, at, Some(region(2, 8, 2, 5, false)));
    // The wrap at musical beat 3 is still owed to the delayed output.
    assert!(
        p.plan(&c, at)
            .iter()
            .any(|e| matches!(e.action, PlannedAction::Automate { .. })
                && e.at == c.instant_at(BeatTime(5 * B)).unwrap())
    );
    for (beat, expected) in [(4, 0.3), (5, 0.4), (8, 0.7), (9, 0.8), (12, 0.8)] {
        close(value(&a, &c, beat * B).0, expected);
    }
    assert_eq!(value(&a, &c, 9 * B).1, AutomationState::Held);
}

#[test]
fn unlinked_clip_end_and_stop_cut_off_own_timeline_at_outgoing_value() {
    for natural in [false, true] {
        let mut project = fixture(
            Some(region(2, 8, 2, 5, true)),
            &[(0, 0.0), (10, 1.0)],
            200.0,
        );
        if natural {
            let clip = project.clip_mut(TrackId(1), 0).unwrap();
            clip.looping = false;
            clip.end = BeatTime(3 * B);
        }
        let (mut p, c, mut a, _) = rig(project);
        let end = c.instant_at(BeatTime(3 * B)).unwrap();
        if !natural {
            p.stop_transport(&c, end);
        }
        a.sync(&p, &c, end);
        close(value(&a, &c, 3 * B).0, 0.5);
        close(value(&a, &c, 10 * B).0, 0.5);
        assert_eq!(value(&a, &c, 10 * B).1, AutomationState::Held);
    }
}

#[test]
fn unlinked_flat_stretch_wakes_at_own_wrap_and_delayed_samples_keep_their_phase() {
    use super::parameter_controls::SendValue;
    let (_, c, a, start) = rig(fixture(
        Some(region(2, 8, 2, 5, true)),
        &[(2, 0.2), (3, 0.3), (5, 0.3)],
        1000.0,
    ));
    let key = a.values(start)[0].field.parameter.clone();
    let from = c.instant_at(BeatTime(2 * B)).unwrap();
    let wrap = c.instant_at(BeatTime(3 * B)).unwrap();
    assert_eq!(a.next_frame(&key, from, 0.0), Some(wrap));
    assert_eq!(
        a.next_frame(&key, from + Duration::from_secs(1), 1000.0),
        Some(wrap + Duration::from_secs(1))
    );
    for (at, delay) in [
        (start + Duration::from_millis(1250), 1000.0),
        (start + Duration::from_millis(50), -200.0),
    ] {
        let sample = a.sample(&key, at, delay).unwrap();
        let SendValue::Float(v) = sample.0[0] else {
            panic!("float")
        };
        assert!((v - 0.25).abs() < 1e-6);
    }
}

#[test]
fn unlinked_and_linked_jumps_coalesce_but_each_samples_its_own_time() {
    let mut project = fixture(Some(region(2, 8, 2, 5, true)), &[(0, 0.0), (10, 1.0)], 0.0);
    let track = &mut project.tracks[0];
    let device = track.first_target_mut_or_insert(|| DeviceId(99));
    let id = device.id;
    let target = device.target_mut().unwrap();
    target.parameters.push(ParameterDef {
        key: EntryKey("other".into()),
        name: String::new(),
        path: OscAddress::Relative("/other".into()),
        fields: vec![FieldDef::new(
            FieldKey("value".into()),
            FieldType::Float,
            true,
        )],
        send_mode: SendMode::Continuous,
    });
    track.slots[0]
        .clip
        .as_mut()
        .unwrap()
        .lanes
        .push(Lane::Envelope(AutomationEnvelope {
            region: None,
            target: ArgumentRef {
                device: DeviceRef::Chain(id),
                parameter: ParameterKey("other".into()),
                field: FieldKey("value".into()),
            },
            points: [(0, 0.0), (6, 0.0), (6, 1.0), (16, 1.0)]
                .into_iter()
                .map(|(beat, value)| Breakpoint {
                    time: BeatTime(beat * B),
                    value,
                    curve: 0.0,
                })
                .collect(),
        }));
    let (p, c, a, start) = rig(project);
    let at = c.instant_at(BeatTime(6 * B)).unwrap();
    let events: Vec<_> = p
        .plan(&c, start)
        .into_iter()
        .filter(|e| e.at == at && matches!(e.action, PlannedAction::Automate { .. }))
        .collect();
    assert_eq!(events.len(), 1);
    let PlannedAction::Automate { fields, .. } = &events[0].action else {
        unreachable!()
    };
    assert_eq!(fields.len(), 2);
    for v in a.values(at) {
        close(
            v.value,
            if v.field.parameter.device == DeviceRef::Mixer {
                0.2
            } else {
                1.0
            },
        );
    }
}

#[test]
fn unlinked_loop_off_past_own_end_holds_at_the_switch_instant() {
    let (mut p, c, mut a, _) = rig(fixture(
        Some(region(2, 3, 2, 5, true)),
        &[(0, 0.0), (10, 1.0)],
        0.0,
    ));
    let at = c.instant_at(BeatTime(2 * B)).unwrap();
    set_region(&mut p, &c, &mut a, at, Some(region(2, 3, 2, 5, false)));
    close(value(&a, &c, 2 * B).0, 0.4);
    close(value(&a, &c, 10 * B).0, 0.4);
    assert_eq!(value(&a, &c, 2 * B).1, AutomationState::Held);
    // A later unrelated edit must retain the same linear/held segment.
    let later = c.instant_at(BeatTime(4 * B)).unwrap();
    let mut project = (*p.project).clone();
    project.tracks[0].name = "renamed".into();
    p.update_project(project, &c, later).unwrap();
    a.sync(&p, &c, later);
    close(value(&a, &c, 10 * B).0, 0.4);
}

#[test]
fn unlinked_while_legato_playing_inherits_clip_phase_and_future_wraps() {
    // Stored Start may be past the brace; the unlink edit normalises its copy.
    for start in [1, 9] {
        let mut project = fixture(None, &[(0, 0.0), (8, 0.8)], 0.0);
        let clip = project.clip_mut(TrackId(1), 0).unwrap();
        clip.legato = true;
        clip.start = BeatTime(start * B);
        clip.loop_start = BeatTime(2 * B);
        clip.loop_end = BeatTime(6 * B);
        let (mut p, c, mut a, _) = rig(project);
        p.launch_at(TrackId(1), 0, BeatTime(3 * B)).unwrap();
        let launch = c.instant_at(BeatTime(3 * B)).unwrap();
        a.sync(&p, &c, launch);
        let at = c.instant_at(BeatTime(4 * B)).unwrap();
        let markers = p.project.clip(TrackId(1), 0).unwrap().region();
        let samples = [4 * B, 5 * B - 1, 5 * B, 6 * B, 9 * B];
        let before = samples.map(|t| value(&a, &c, t));
        let planned: Vec<_> = jumps(&p, &c, at)
            .into_iter()
            .filter(|t| *t >= 4 * B)
            .collect();
        set_region(&mut p, &c, &mut a, at, Some(markers));
        for (ticks, expected) in samples.into_iter().zip(before) {
            let actual = value(&a, &c, ticks);
            close(actual.0, expected.0);
            assert_eq!(actual.1, expected.1);
        }
        close(value(&a, &c, 4 * B).0, 0.5);
        close(value(&a, &c, 5 * B).0, 0.2);
        assert_eq!(jumps(&p, &c, at), planned);
        // An unrelated edit retains the inherited origin; a fresh launch discards it.
        let mut project = (*p.project).clone();
        project.tracks[0].name = "renamed".into();
        p.update_project(project, &c, at).unwrap();
        a.sync(&p, &c, at);
        close(value(&a, &c, 4 * B).0, 0.5);
        // A legato take-over would take the clip's phase again, so relaunch plainly.
        let mut project = (*p.project).clone();
        project.clip_mut(TrackId(1), 0).unwrap().legato = false;
        p.update_project(project, &c, at).unwrap();
        p.launch_at(TrackId(1), 0, BeatTime(7 * B)).unwrap();
        a.sync(&p, &c, c.instant_at(BeatTime(7 * B)).unwrap());
        close(value(&a, &c, 7 * B).0, if start == 1 { 0.1 } else { 0.5 });
    }
}

#[test]
fn unlinked_after_clip_loop_off_inherits_continuation_and_delayed_phase() {
    let mut project = fixture(None, &[(0, 0.0), (10, 1.0)], 1000.0);
    project.clip_mut(TrackId(1), 0).unwrap().loop_end = BeatTime(4 * B);
    let (mut p, c, mut a, _) = rig(project);
    let off = c.instant_at(BeatTime(6 * B)).unwrap();
    let mut project = (*p.project).clone();
    project.clip_mut(TrackId(1), 0).unwrap().looping = false;
    p.update_project(project, &c, off).unwrap();
    a.sync(&p, &c, off);
    let at = c.instant_at(BeatTime(7 * B)).unwrap();
    let markers = p.project.clip(TrackId(1), 0).unwrap().region();
    let samples = [7 * B, 8 * B, 10 * B, 20 * B, 21 * B];
    let before = samples.map(|t| value(&a, &c, t));
    set_region(&mut p, &c, &mut a, at, Some(markers));
    for (ticks, expected) in samples.into_iter().zip(before) {
        let actual = value(&a, &c, ticks);
        close(actual.0, expected.0);
        assert_eq!(actual.1, expected.1);
    }
    close(value(&a, &c, 7 * B).0, 0.3);
    let key = a.values(at)[0].field.parameter.clone();
    let delayed = c.instant_at(BeatTime(8 * B)).unwrap();
    let sample = a.sample(&key, delayed, 1000.0).unwrap();
    let super::parameter_controls::SendValue::Float(v) = sample.0[0] else {
        panic!("float")
    };
    assert!((v - 0.2).abs() < 1e-6);
    let mut project = (*p.project).clone();
    project.tracks[0].name = "renamed".into();
    p.update_project(project, &c, at).unwrap();
    a.sync(&p, &c, at);
    close(value(&a, &c, 8 * B).0, 0.4);
}
