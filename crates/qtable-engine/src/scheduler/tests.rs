//! Core planner regression tests and shared fixture.
use super::*;
use crate::model::*;

pub(super) fn project(delay: f64, looping: bool) -> Project {
    let mut track = Track::new(TrackId(1), "track", 2);
    let mut clip = Clip::new(
        ClipId(1),
        "clip",
        BeatTime(960),
        crate::model::LIVE_CLIP_PALETTE[0],
    )
    .unwrap();
    clip.looping = looping;
    clip.events = [480, 0, 480]
        .into_iter()
        .enumerate()
        .map(|(i, position)| {
            let path = OscAddress::Absolute(format!("/e{i}"));
            track.hit(BeatTime(position), path, vec![], || DeviceId(1))
        })
        .collect();
    let mut second = clip.clone();
    second.id = ClipId(2);
    track.delay_ms = delay;
    track.route = Route::single(TrackId(201));
    track.slots[0].clip = Some(clip);
    track.slots[1].clip = Some(second);
    Project {
        tracks: vec![track],
        scenes: (1..=2)
            .map(|i| Scene {
                id: SceneId(i),
                name: "scene".into(),
                follow_actions: SceneFollowActions::default(),
            })
            .collect(),
        outputs: vec![Track::new_output(TrackId(201), "target", "127.0.0.1", 9999)],

        ..Project::default()
    }
}
/// A copy of `track` as track `id`, its devices renumbered (ids are project-wide) and its
/// clips following them.
pub(super) fn copy_track(track: &Track, id: TrackId) -> Track {
    let mut copy = track.clone();
    copy.id = id;
    for device in &mut copy.devices {
        device.id = DeviceId(device.id.0 + 100 * id.0);
    }
    for clip in copy.slots.iter_mut().filter_map(|s| s.clip.as_mut()) {
        for lane in clip.lanes.iter_mut().filter_map(Lane::event_mut) {
            lane.definition.device.0 += 100 * id.0;
        }
        for event in &mut clip.events {
            event.definition.device.0 += 100 * id.0;
        }
    }
    copy
}
fn clock(start: Instant) -> InternalClock {
    let mut c = InternalClock::default();
    c.play(start);
    c
}
fn address(e: &DueEvent) -> &str {
    match &e.action {
        PlannedAction::Send {
            packet: Some(OscPacket::Message(m)),
            ..
        } => &m.addr,
        _ => "silent",
    }
}

#[test]
fn order_wrap_and_consumption() {
    let start = Instant::now();
    let clock = clock(start);
    let mut p = Planner::new(project(0.0, true), Duration::from_millis(800)).unwrap();
    p.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
    let events = p.plan(&clock, start);
    assert_eq!(
        events.iter().map(address).collect::<Vec<_>>(),
        ["/e1", "/e0", "/e2", "/e1", "/e0", "/e2"]
    );
    assert_eq!(events[3].at, start + Duration::from_millis(500));
    for e in events {
        p.consume(&e);
    }
    assert!(p.plan(&clock, start).is_empty());
}
#[test]
fn delays_and_overdue() {
    let start = Instant::now();
    let clock = clock(start);
    for delay in [-100.0, 100.0] {
        let mut p = Planner::new(project(delay, false), Duration::from_millis(800)).unwrap();
        p.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
        let events = p.plan(&clock, start);
        let expected = if delay < 0.0 {
            start - Duration::from_millis(100)
        } else {
            start + Duration::from_millis(100)
        };
        assert_eq!(events[0].at, expected);
        assert_eq!(
            events.last().unwrap().at,
            start + Duration::from_millis(500)
        );
        assert!(matches!(
            events.last().unwrap().action,
            PlannedAction::TrackSilent(TrackId(1))
        ));
    }
    let p = Planner::new(project(-100.0, true), Duration::ZERO).unwrap();
    assert_eq!(p.lookahead(), Duration::from_millis(100));
}
#[test]
fn next_bar_replace_and_end() {
    let start = Instant::now();
    let mut clock = clock(start);
    let mut p = Planner::new(project(0.0, true), Duration::from_secs(3)).unwrap();
    p.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
    assert_eq!(
        p.launch(TrackId(1), 1, &mut clock, start).unwrap(),
        BeatTime(3840)
    );
    let events = p.plan(&clock, start);
    assert_eq!(
        events
            .iter()
            .filter(|e| e.at == start + Duration::from_secs(2)
                && matches!(e.action, PlannedAction::Send { .. }))
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .find(|e| e.at == start + Duration::from_secs(2)
                && matches!(e.action, PlannedAction::Send { .. }))
            .unwrap()
            .voice,
        2
    );
    let mut p = Planner::new(project(0.0, false), Duration::from_secs(3)).unwrap();
    p.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
    let events = p.plan(&clock, start);
    assert_eq!(events.len(), 4);
    for e in events {
        p.consume(&e);
    }
    assert!(p.plan(&clock, start).is_empty());
}
#[test]
fn silence_precedes_long_positive_delay_without_losing_packets() {
    let start = Instant::now();
    let mut clock = clock(start);
    let mut p = Planner::new(project(1000.0, false), Duration::from_secs(2)).unwrap();
    p.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
    let events = p.plan(&clock, start);
    assert_eq!(events.len(), 4);
    assert!(matches!(events[0].action, PlannedAction::TrackSilent(_)));
    assert_eq!(events[0].at, start + Duration::from_millis(500));
    assert_eq!(events[1].at, start + Duration::from_secs(1));
    // An overdue end survives the clock's discarded tempo segment too.
    assert!(p.set_tempo(
        &mut clock,
        Tempo::new(60.0).unwrap(),
        start + Duration::from_millis(600)
    ));
    let events = p.plan(&clock, start + Duration::from_millis(600));
    assert_eq!(events.len(), 4);
    for event in events {
        p.consume(&event);
    }
    assert!(
        p.plan(&clock, start + Duration::from_millis(600))
            .is_empty()
    );
}

#[test]
fn tempo_replans_future_but_keeps_elapsed_delayed_events() {
    let start = Instant::now();
    let mut clock = clock(start);
    let mut p = Planner::new(project(100.0, false), Duration::from_secs(2)).unwrap();
    p.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
    assert!(p.set_tempo(
        &mut clock,
        Tempo::new(60.0).unwrap(),
        start + Duration::from_millis(50)
    ));
    let events = p.plan(&clock, start + Duration::from_millis(50));
    assert_eq!(events[0].at, start + Duration::from_millis(100));
    assert!(events[1].at > start + Duration::from_millis(500));
}

#[test]
fn hits_on_an_inactive_target_send_nothing_and_still_advance_the_voice() {
    let start = Instant::now();
    let clock = clock(start);
    let mut project = project(0.0, true);
    project.device_mut(DeviceId(1)).unwrap().active = false;
    let mut p = Planner::new(project, Duration::from_millis(800)).unwrap();
    p.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
    let events = p.plan(&clock, start);
    let sends: Vec<_> = events
        .iter()
        .filter(|e| matches!(e.action, PlannedAction::Send { .. }))
        .collect();
    // The same six deadlines as an active Target (`order_wrap_and_consumption`), none sending.
    assert_eq!(sends.len(), 6);
    assert!(sends.iter().all(|e| address(e) == "silent"));
    for event in &events {
        p.consume(event);
    }
    assert!(p.voices[0].cursor >= 6, "the voice kept its place");
    // Turned back on, the next hit sends at its own time with no backlog.
    let mut on = (*p.project).clone();
    on.device_mut(DeviceId(1)).unwrap().active = true;
    let now = start + Duration::from_millis(800);
    p.update_project(on, &clock, now).unwrap();
    let next = p.plan(&clock, now);
    assert!(next.iter().all(|e| e.at >= now));
    assert_eq!(address(&next[0]), "/e1");
}

#[test]
fn retained_hits_outside_markers_do_not_enter_the_zero_based_playback_order() {
    let mut project = project(0.0, true);
    let track = &mut project.tracks[0];
    let definition = track.slots[0].clip.as_ref().unwrap().events[0]
        .definition
        .clone();
    let clip = track.slots[0].clip.as_mut().unwrap();
    for position in [clip.length(), BeatTime(10_000)] {
        clip.events.push(Event {
            position,
            definition: definition.clone(),
            arguments: vec![],
        });
    }
    project.validate().unwrap();
    let start = Instant::now();
    let clock = clock(start);
    let mut p = Planner::new(project, Duration::from_millis(800)).unwrap();
    p.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
    let events = p.plan(&clock, start);
    assert_eq!(
        events.iter().map(address).collect::<Vec<_>>(),
        ["/e1", "/e0", "/e2", "/e1", "/e0", "/e2"]
    );
}

#[test]
fn upgraded_v8_fixture_keeps_hit_times_and_wire_arguments() {
    let fixture =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/v8-envelopes.ron");
    for looping in [true, false] {
        let mut project = crate::file::load(&fixture).unwrap();
        let clip = project.tracks[1].slots[0].clip.as_mut().unwrap();
        clip.looping = looping;
        let expected_arguments = event_packet(&clip.events[0], "");
        let start = Instant::now();
        let clock = clock(start);
        let mut p = Planner::new(project, Duration::from_millis(2900)).unwrap();
        p.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
        let events = p.plan(&clock, start);
        let sends: Vec<_> = events
            .iter()
            .filter(|e| matches!(e.action, PlannedAction::Send { .. }))
            .collect();
        let expected_ms: &[u64] = if looping {
            &[0, 0, 500, 500, 2000, 2000, 2500, 2500]
        } else {
            &[0, 0, 500, 500]
        };
        assert_eq!(
            sends
                .iter()
                .map(|e| e.at.duration_since(start).as_millis() as u64)
                .collect::<Vec<_>>(),
            expected_ms
        );
        let OscPacket::Message(expected) = &expected_arguments else {
            unreachable!()
        };
        for e in sends.iter().filter(|e| address(e).ends_with("/cue")) {
            let PlannedAction::Send {
                packet: Some(OscPacket::Message(message)),
                ..
            } = &e.action
            else {
                unreachable!()
            };
            assert_eq!(message.args, expected.args);
        }
    }
}
