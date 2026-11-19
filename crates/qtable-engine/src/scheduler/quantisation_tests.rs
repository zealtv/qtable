//! Deterministic launch/stop decisions, independent of thread scheduling.
use super::*;
use crate::model::{NoteValue, QuantisationValue, TimeSignature};

#[test]
fn stopped_clip_scene_and_group_launches_ignore_every_grid_and_arm_from_zero() {
    use crate::model::{Track, TrackId};
    use NoteValue::*;
    use QuantisationValue::*;
    let grids = [
        GlobalQuantisation::None,
        GlobalQuantisation::Value(Bars(1)),
        GlobalQuantisation::Value(Bars(2)),
        GlobalQuantisation::Value(Bars(4)),
        GlobalQuantisation::Value(Bars(8)),
        GlobalQuantisation::Value(Note(Half)),
        GlobalQuantisation::Value(Note(Quarter)),
        GlobalQuantisation::Value(Note(Eighth)),
        GlobalQuantisation::Value(Note(Sixteenth)),
        GlobalQuantisation::Value(Note(ThirtySecond)),
        GlobalQuantisation::Value(Triplet(Half)),
        GlobalQuantisation::Value(Triplet(Quarter)),
        GlobalQuantisation::Value(Triplet(Eighth)),
        GlobalQuantisation::Value(Triplet(Sixteenth)),
    ];
    for grid in grids {
        let explicit = match grid {
            GlobalQuantisation::None => LaunchQuantisation::None,
            GlobalQuantisation::Value(value) => LaunchQuantisation::Value(value),
        };
        for (global, clip_grid) in [
            (grid, LaunchQuantisation::Global),
            (GlobalQuantisation::Value(Bars(4)), explicit),
        ] {
            for scope in 0..3 {
                let (start, mut clock, mut p) = setup();
                // A stopped clock can retain a nonzero position; restart still
                // launches at zero, not on the grid of that frozen position.
                let now = start + Duration::from_secs(3);
                clock.stop(now);
                let project = Arc::make_mut(&mut p.project);
                project.global_quantisation = global;
                for track in &mut project.tracks {
                    for slot in &mut track.slots {
                        let clip = slot.clip.as_mut().unwrap();
                        clip.quantisation = clip_grid;
                        clip.follow_actions.actions.enabled = true;
                        clip.follow_actions.linked = false;
                        clip.follow_actions.time = BeatTime(960);
                    }
                }
                project.scenes[0].follow_actions.actions.enabled = true;
                project.scenes[0].follow_actions.time = BeatTime(960);
                {
                    let mut group = Track::new_group(TrackId(101), "group", project.scenes.len());
                    group.colour = crate::model::LIVE_CLIP_PALETTE[0];
                    project.insert_group(group, &[TrackId(1), TrackId(2)]);
                }
                match scope {
                    0 => {
                        p.launch(TrackId(1), 0, &mut clock, now).unwrap();
                    }
                    1 => {
                        p.launch_scene(SceneId(1), &mut clock, now).unwrap();
                    }
                    _ => {
                        p.launch_group_slot(TrackId(101), 0, &mut clock, now)
                            .unwrap();
                    }
                }
                assert_eq!(clock.instant_at(BeatTime::ZERO), Some(now));
                assert_eq!(p.voices.len(), if scope == 0 { 1 } else { 2 });
                for voice in &p.voices {
                    assert_eq!(voice.launch, BeatTime::ZERO);
                    assert_eq!(p.follow_beat(voice), Some(BeatTime(960)));
                }
                assert_eq!(
                    p.scene_timer,
                    (scope == 1).then_some((SceneId(1), BeatTime(960)))
                );
            }
        }
    }
}

fn setup() -> (Instant, InternalClock, Planner) {
    let start = Instant::now();
    let mut project = super::tests::project(0.0, true);
    let mut second = crate::scheduler::tests::copy_track(&project.tracks[0], TrackId(2));
    for (slot, item) in second.slots.iter_mut().enumerate() {
        item.clip.as_mut().unwrap().id = crate::model::ClipId(slot as u64 + 3);
    }
    project.tracks.push(second);
    project.tempo = Tempo::new(120.0).unwrap();
    let mut clock = InternalClock::new(project.tempo, project.time_signature);
    clock.play(start);
    (
        start,
        clock,
        Planner::new(project, Duration::from_secs(20)).unwrap(),
    )
}

fn at(clock: &InternalClock, tick: u64) -> Instant {
    clock.instant_at(BeatTime(tick)).unwrap()
}

fn sends(events: &[DueEvent]) -> Vec<&DueEvent> {
    events
        .iter()
        .filter(|e| matches!(e.action, PlannedAction::Send { .. }))
        .collect()
}

#[test]
fn every_menu_value_uses_the_song_grid_and_strict_boundaries() {
    use NoteValue::*;
    use QuantisationValue::*;
    for signature in [TimeSignature::default(), TimeSignature::new(6, 8).unwrap()] {
        let bar = signature.bar_length().ticks();
        for (value, length) in [
            (Bars(1), bar),
            (Bars(2), 2 * bar),
            (Bars(4), 4 * bar),
            (Bars(8), 8 * bar),
            (Note(Half), 1920),
            (Triplet(Half), 1280),
            (Note(Quarter), 960),
            (Triplet(Quarter), 640),
            (Note(Eighth), 480),
            (Triplet(Eighth), 320),
            (Note(Sixteenth), 240),
            (Triplet(Sixteenth), 160),
            (Note(ThirtySecond), 120),
        ] {
            for tick in [0, 3 * length, 3 * length + 17, 4 * length - 1] {
                let (_, mut clock, mut planner) = setup();
                clock.set_time_signature(signature);
                Arc::make_mut(&mut planner.project).tracks[0].slots[0]
                    .clip
                    .as_mut()
                    .unwrap()
                    .quantisation = LaunchQuantisation::Value(value);
                let now = at(&clock, tick);
                let launch = planner.launch(TrackId(1), 0, &mut clock, now).unwrap();
                assert_eq!(
                    launch,
                    BeatTime((tick / length + 1) * length),
                    "{value:?}, {signature:?}, tick {tick}"
                );
            }
        }
    }
}

#[test]
fn global_clip_override_and_none_are_resolved_at_request_time() {
    let (_, mut clock, mut planner) = setup();
    let now = at(&clock, 123);
    planner
        .set_global_quantisation(GlobalQuantisation::Value(QuantisationValue::Note(
            NoteValue::Quarter,
        )))
        .unwrap();
    assert_eq!(
        planner.launch(TrackId(1), 0, &mut clock, now).unwrap(),
        BeatTime(960)
    );
    planner.stop_all();
    Arc::make_mut(&mut planner.project).tracks[0].slots[0]
        .clip
        .as_mut()
        .unwrap()
        .quantisation = LaunchQuantisation::Value(QuantisationValue::Bars(2));
    assert_eq!(
        planner.launch(TrackId(1), 0, &mut clock, now).unwrap(),
        BeatTime(7680)
    );
    planner.stop_all();
    Arc::make_mut(&mut planner.project).tracks[0].slots[0]
        .clip
        .as_mut()
        .unwrap()
        .quantisation = LaunchQuantisation::None;
    assert_eq!(
        planner.launch(TrackId(1), 0, &mut clock, now).unwrap(),
        BeatTime(123)
    );
    assert_eq!(sends(&planner.plan(&clock, now))[0].at, now);
    planner.stop_all();
    Arc::make_mut(&mut planner.project).tracks[0].slots[0]
        .clip
        .as_mut()
        .unwrap()
        .quantisation = LaunchQuantisation::Global;
    planner
        .set_global_quantisation(GlobalQuantisation::None)
        .unwrap();
    assert_eq!(
        planner.launch(TrackId(1), 0, &mut clock, now).unwrap(),
        BeatTime(123)
    );
}

#[test]
fn retrigger_retains_original_boundary_even_after_global_change() {
    let (_, mut clock, mut planner) = setup();
    let now = at(&clock, 100);
    assert_eq!(
        planner.launch(TrackId(1), 0, &mut clock, now).unwrap(),
        BeatTime(3840)
    );
    planner
        .set_global_quantisation(GlobalQuantisation::Value(QuantisationValue::Bars(8)))
        .unwrap();
    let later = at(&clock, 1000);
    assert_eq!(
        planner.launch(TrackId(1), 0, &mut clock, later).unwrap(),
        BeatTime(3840)
    );
    assert_eq!(planner.voices.len(), 1);
}

#[test]
fn latest_trigger_replaces_earlier_trigger_with_its_own_grid() {
    for grid in [
        QuantisationValue::Note(NoteValue::Quarter),
        QuantisationValue::Bars(2),
    ] {
        let (_, mut clock, mut planner) = setup();
        planner.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
        let now = at(&clock, 1000);
        planner.launch(TrackId(1), 0, &mut clock, now).unwrap();
        Arc::make_mut(&mut planner.project).tracks[0].slots[1]
            .clip
            .as_mut()
            .unwrap()
            .quantisation = LaunchQuantisation::Value(grid);
        let launch = planner.launch(TrackId(1), 1, &mut clock, now).unwrap();
        assert_eq!(
            launch,
            if matches!(grid, QuantisationValue::Bars(_)) {
                BeatTime(7680)
            } else {
                BeatTime(1920)
            }
        );
        assert_eq!(planner.voices.len(), 2);
        assert_eq!(planner.voices[0].cutoff, Some(launch));
        assert_eq!(planner.voices[1].slot, 1);
        let boundary = at(&clock, launch.ticks());
        let events = planner.plan(&clock, now);
        let packets = sends(&events);
        assert!(
            packets
                .iter()
                .filter(|e| e.at < boundary)
                .all(|e| e.voice == 1)
        );
        assert!(
            packets
                .iter()
                .filter(|e| e.at >= boundary)
                .all(|e| e.voice == 3)
        );
        assert_eq!(packets.iter().find(|e| e.voice == 3).unwrap().at, boundary);
    }
}

#[test]
fn relaunching_playing_clip_restarts_on_next_boundary() {
    let (_, mut clock, mut planner) = setup();
    planner
        .set_global_quantisation(GlobalQuantisation::Value(QuantisationValue::Note(
            NoteValue::Quarter,
        )))
        .unwrap();
    planner.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
    let now = at(&clock, 1000);
    assert_eq!(
        planner.launch(TrackId(1), 0, &mut clock, now).unwrap(),
        BeatTime(1920)
    );
    let events = planner.plan(&clock, now);
    let restarted = sends(&events).into_iter().find(|e| e.voice == 2).unwrap();
    assert_eq!(restarted.at, at(&clock, 1920));
    for event in events {
        planner.consume(&event);
    }
    assert!(planner.voices.iter().all(|v| v.id == 2));
}

#[test]
fn stopping_trigger_only_cancels_immediately() {
    let (_, mut clock, mut planner) = setup();
    let now = at(&clock, 100);
    planner.launch(TrackId(1), 0, &mut clock, now).unwrap();
    assert!(planner.request_stop(TrackId(1), &clock, now).unwrap());
    assert!(planner.voices.is_empty());
    assert!(planner.stops.is_empty());
    assert!(planner.plan(&clock, now).is_empty());
}

#[test]
fn playing_stop_uses_global_cancels_trigger_and_has_no_boundary_packet() {
    let (_, mut clock, mut planner) = setup();
    planner.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
    Arc::make_mut(&mut planner.project).tracks[0].slots[1]
        .clip
        .as_mut()
        .unwrap()
        .quantisation = LaunchQuantisation::Value(QuantisationValue::Bars(8));
    let now = at(&clock, 1000);
    planner.launch(TrackId(1), 1, &mut clock, now).unwrap();
    planner
        .set_global_quantisation(GlobalQuantisation::Value(QuantisationValue::Note(
            NoteValue::Quarter,
        )))
        .unwrap();
    assert!(!planner.request_stop(TrackId(1), &clock, now).unwrap());
    assert_eq!(planner.voices.len(), 1);
    assert_eq!(planner.stops[&TrackId(1)].beat, BeatTime(1920));
    let events = planner.plan(&clock, now);
    assert!(sends(&events).iter().all(|e| e.at < at(&clock, 1920)));
    assert!(matches!(
        events.last().unwrap().action,
        PlannedAction::StopTrack(TrackId(1))
    ));
    for event in events {
        planner.consume(&event);
    }
    assert!(planner.voices.is_empty());
    assert!(planner.stops.is_empty());
}

#[test]
fn stop_all_quantises_playing_tracks_and_cancels_idle_triggers() {
    let (_, mut clock, mut planner) = setup();
    planner.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
    let now = at(&clock, 1000);
    planner.launch(TrackId(2), 0, &mut clock, now).unwrap();
    assert_eq!(
        planner.request_stop_all(&clock, now).unwrap(),
        vec![TrackId(2)]
    );
    assert_eq!(planner.stops.len(), 1);
    assert_eq!(planner.stops[&TrackId(1)].beat, BeatTime(3840));
    let events = planner.plan(&clock, now);
    for event in events {
        planner.consume(&event);
    }
    assert!(planner.voices.is_empty());
    assert!(planner.stops.is_empty());
}

#[test]
fn none_stop_is_immediate_and_launch_cancels_pending_stop() {
    let (_, mut clock, mut planner) = setup();
    planner.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
    let now = at(&clock, 1000);
    planner.request_stop(TrackId(1), &clock, now).unwrap();
    assert!(!planner.stops.is_empty());
    planner.launch(TrackId(1), 1, &mut clock, now).unwrap();
    assert!(planner.stops.is_empty());
    planner
        .set_global_quantisation(GlobalQuantisation::None)
        .unwrap();
    assert!(planner.request_stop(TrackId(1), &clock, now).unwrap());
    assert!(planner.voices.is_empty());
}

#[test]
fn tempo_change_moves_triggered_launch_and_pending_stop_in_wall_time() {
    let (start, mut clock, mut planner) = setup();
    let now = at(&clock, 100);
    planner.launch(TrackId(1), 0, &mut clock, now).unwrap();
    planner.launch_at(TrackId(2), 0, BeatTime::ZERO).unwrap();
    planner.request_stop(TrackId(2), &clock, now).unwrap();
    assert!(planner.set_tempo(
        &mut clock,
        Tempo::new(60.0).unwrap(),
        start + Duration::from_millis(500)
    ));
    assert_eq!(
        planner
            .voices
            .iter()
            .find(|v| v.track == TrackId(1))
            .unwrap()
            .launch,
        BeatTime(3840)
    );
    assert_eq!(planner.stops[&TrackId(2)].beat, BeatTime(3840));
    let events = planner.plan(&clock, start + Duration::from_millis(500));
    let launch = sends(&events).into_iter().find(|e| e.voice == 1).unwrap();
    assert_eq!(launch.at, start + Duration::from_millis(3500));
    assert_eq!(
        events
            .iter()
            .find(|e| matches!(e.action, PlannedAction::StopTrack(_)))
            .unwrap()
            .at,
        launch.at
    );
}

#[test]
fn launch_while_stopped_starts_transport_and_clip_at_zero() {
    let (start, mut clock, mut planner) = setup();
    clock.stop(start + Duration::from_millis(500));
    let now = start + Duration::from_secs(1);
    assert_eq!(
        planner.launch(TrackId(1), 0, &mut clock, now).unwrap(),
        BeatTime::ZERO
    );
    assert!(clock.is_playing());
    assert_eq!(clock.beat_at(now), Some(BeatTime::ZERO));
    assert_eq!(sends(&planner.plan(&clock, now))[0].at, now);
}

#[test]
fn immediate_negative_delay_remains_overdue_for_immediate_dispatch() {
    let (start, mut clock, mut planner) = setup();
    Arc::make_mut(&mut planner.project).tracks[0].delay_ms = -20.0;
    planner
        .set_global_quantisation(GlobalQuantisation::None)
        .unwrap();
    let now = start + Duration::from_millis(500);
    planner.launch(TrackId(1), 0, &mut clock, now).unwrap();
    assert_eq!(
        sends(&planner.plan(&clock, now))[0].at,
        now - Duration::from_millis(20)
    );
}

#[test]
fn replacing_dummy_clip_retires_it_at_boundary_without_packets() {
    let (_, mut clock, mut planner) = setup();
    Arc::make_mut(&mut planner.project).tracks[0].slots[0]
        .clip
        .as_mut()
        .unwrap()
        .events
        .clear();
    planner.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
    let now = at(&clock, 1000);
    planner.launch(TrackId(1), 1, &mut clock, now).unwrap();
    let events = planner.plan(&clock, now);
    assert!(matches!(events[0].action, PlannedAction::RetireVoice));
    assert_eq!(events[0].at, at(&clock, 3840));
    planner.consume(&events[0]);
    assert_eq!(planner.voices.len(), 1);
    assert_eq!(planner.voices[0].slot, 1);
}

#[test]
fn unsupported_values_and_overflow_are_errors() {
    use NoteValue::*;
    use QuantisationValue::*;
    for value in [
        Bars(0),
        Bars(3),
        Bars(16),
        Note(Whole),
        Note(SixtyFourth),
        Triplet(Whole),
        Triplet(ThirtySecond),
        Triplet(SixtyFourth),
    ] {
        assert!(value.validate().is_err(), "{value:?}");
    }
    let quantisation = GlobalQuantisation::Value(Note(Quarter));
    assert!(
        quantisation
            .next_boundary(BeatTime(u64::MAX), TimeSignature::default())
            .is_err()
    );
    assert_eq!(
        GlobalQuantisation::None
            .next_boundary(BeatTime(u64::MAX), TimeSignature::default())
            .unwrap(),
        BeatTime(u64::MAX)
    );
}

#[test]
fn repeated_stop_retains_boundary_and_invalid_launch_does_not_start_transport() {
    let (start, mut clock, mut planner) = setup();
    planner.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
    planner
        .request_stop(TrackId(1), &clock, at(&clock, 100))
        .unwrap();
    planner
        .set_global_quantisation(GlobalQuantisation::Value(QuantisationValue::Bars(8)))
        .unwrap();
    planner
        .request_stop(TrackId(1), &clock, at(&clock, 1000))
        .unwrap();
    assert_eq!(planner.stops[&TrackId(1)].beat, BeatTime(3840));
    planner.stop_all();
    clock.stop(start);
    assert!(planner.launch(TrackId(1), 99, &mut clock, start).is_err());
    assert!(!clock.is_playing());
    assert!(planner.voices.is_empty());
}
