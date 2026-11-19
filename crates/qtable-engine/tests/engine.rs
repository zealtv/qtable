use qtable_engine::edit::ProjectEdit;
use qtable_engine::engine::{Command, Engine, EngineError};
use qtable_engine::model::{GlobalQuantisation, QuantisationValue, TrackId};
use qtable_test_support::TestReceiver;
use std::time::Duration;

#[path = "support/scheduler_project.rs"]
mod fixture;

#[test]
fn command_validation_is_synchronous() {
    let receiver = TestReceiver::bind().unwrap();
    let mut project = fixture::project(receiver.addr().port(), &[0.0]);
    project.tracks[0].slots[1].clip = None;
    let mut engine = Engine::new(project).unwrap();
    assert!(matches!(
        engine.execute(Command::LaunchClip {
            track: TrackId(99),
            slot: 0
        }),
        Err(EngineError::UnknownTrack(_))
    ));
    assert_eq!(
        engine
            .execute(Command::LaunchClip {
                track: TrackId(99),
                slot: 0
            })
            .unwrap_err()
            .to_string(),
        "unknown track with id 99"
    );
    assert!(matches!(
        engine.execute(Command::LaunchClip {
            track: TrackId(1),
            slot: 99
        }),
        Err(EngineError::UnknownSlot { .. })
    ));
    engine
        .execute(Command::LaunchClip {
            track: TrackId(1),
            slot: 1,
        })
        .unwrap();
    assert!(matches!(
        engine.execute(Command::StopGroup {
            group: qtable_engine::model::TrackId(1099)
        }),
        Err(EngineError::UnknownGroup(_))
    ));
    assert!(matches!(
        engine.execute(Command::SetTempo(f64::NAN)),
        Err(EngineError::InvalidTempo(_))
    ));
    assert!(
        engine
            .execute(Command::SetTempo(f64::NAN))
            .unwrap_err()
            .to_string()
            .contains("invalid tempo: tempo must be positive and finite")
    );
    assert!(matches!(
        engine.execute(Command::SetGlobalQuantisation(GlobalQuantisation::Value(
            QuantisationValue::Bars(3)
        ))),
        Err(EngineError::InvalidQuantisation(_))
    ));
    assert_eq!(600.0, engine.snapshot().tempo_bpm);
}

#[test]
fn engine_drives_transport_and_publishes_launch_state() {
    let receiver = TestReceiver::bind().unwrap();
    let mut engine = Engine::new(fixture::project(receiver.addr().port(), &[0.0])).unwrap();
    engine.execute(Command::Play).unwrap();
    engine
        .execute(Command::LaunchClip {
            track: TrackId(1),
            slot: 0,
        })
        .unwrap();
    let queued = (0..100)
        .find_map(|_| {
            let snapshot = engine.snapshot();
            if snapshot.tracks[0].triggered.is_some() {
                Some(snapshot)
            } else {
                std::thread::sleep(Duration::from_millis(2));
                None
            }
        })
        .expect("scheduler should publish the queued launch");
    let triggered = queued.tracks[0].triggered.as_ref().unwrap();
    assert_eq!(triggered.slot, 0);
    assert!(triggered.launch_beat > queued.current_beat);
    let received = receiver
        .wait_for_messages(3, Duration::from_secs(4))
        .unwrap();
    assert_eq!(received.len(), 3);
    let started = engine.snapshot();
    assert_eq!(started.tracks[0].playing_slot, Some(0));
    assert!(started.tracks[0].play_position_beats.unwrap() >= 0.0);
    engine
        .execute(Command::StopTrack { track: TrackId(1) })
        .unwrap();
    let stop_pending = (0..100)
        .find_map(|_| {
            let state = engine.snapshot();
            if !state.pending_stops.is_empty() {
                Some(state)
            } else {
                std::thread::sleep(Duration::from_millis(1));
                None
            }
        })
        .expect("scheduler should publish the global-quantised stop");
    assert_eq!(stop_pending.pending_stops[0].0, TrackId(1));
    assert!(stop_pending.pending_stops[0].1 > stop_pending.current_beat);
    assert_eq!(stop_pending.tracks[0].playing_slot, Some(0));
    // A track stop now waits for the global bar boundary (400 ms here).
    for _ in 0..100 {
        if engine.snapshot().tracks[0].playing_slot.is_none() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(engine.snapshot().tracks[0].playing_slot.is_none());
    engine
        .execute(Command::LaunchClip {
            track: TrackId(1),
            slot: 0,
        })
        .unwrap();
    for _ in 0..100 {
        if engine.snapshot().tracks[0].triggered.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    engine.execute(Command::StopAll).unwrap();
    for _ in 0..100 {
        let track = &engine.snapshot().tracks[0];
        if track.playing_slot.is_none() && track.triggered.is_none() {
            return;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    panic!("stop all should clear published track state");
}

#[test]
fn tempo_and_quantisation_edits_reach_the_saved_project() {
    let receiver = TestReceiver::bind().unwrap();
    let mut engine = Engine::new(fixture::project(receiver.addr().port(), &[0.0])).unwrap();
    engine.execute(Command::SetTempo(97.5)).unwrap();
    let quarter = GlobalQuantisation::Value(QuantisationValue::Note(
        qtable_engine::model::NoteValue::Quarter,
    ));
    engine
        .execute(Command::SetGlobalQuantisation(quarter))
        .unwrap();
    assert_eq!(engine.project().tempo.bpm(), 97.5);
    assert_eq!(engine.project().global_quantisation, quarter);
}

#[test]
fn model_edits_validate_before_committing() {
    let receiver = TestReceiver::bind().unwrap();
    let project = fixture::project(receiver.addr().port(), &[0.0]);
    let mut engine = Engine::new(project.clone()).unwrap();
    let mut invalid = project.outputs[0].clone();
    invalid.destination_mut().unwrap().port = 0;
    assert!(
        engine
            .execute(Command::Edit(ProjectEdit::SetDestination {
                output: invalid.id,
                destination: invalid.output
            }))
            .is_err()
    );
    assert_eq!(engine.project(), &project);
    assert!(
        engine
            .execute(Command::Edit(ProjectEdit::SetTrackDelay {
                track: TrackId(1),
                delay_ms: f64::NAN
            }))
            .is_err()
    );
    assert_eq!(engine.project(), &project);
    engine
        .execute(Command::Edit(ProjectEdit::SetTrackDelay {
            track: TrackId(1),
            delay_ms: -3.5,
        }))
        .unwrap();
    assert_eq!(engine.project().tracks[0].delay_ms, -3.5);
}

#[test]
fn editing_an_untouched_track_keeps_playing_clip_active() {
    use qtable_engine::edit::ProjectEdit;
    let receiver = TestReceiver::bind().unwrap();
    let mut engine = Engine::new(fixture::project(receiver.addr().port(), &[0.0, 0.0])).unwrap();
    engine
        .execute(Command::LaunchClip {
            track: TrackId(1),
            slot: 0,
        })
        .unwrap();
    for _ in 0..100 {
        if engine.snapshot().tracks[0].playing_slot == Some(0) {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    engine
        .execute(Command::Edit(ProjectEdit::RenameTrack {
            track: TrackId(2),
            name: "edited".into(),
        }))
        .unwrap();
    assert_eq!(engine.project().tracks[1].name, "edited");
    assert_eq!(engine.snapshot().tracks[0].playing_slot, Some(0));
}

#[test]
fn project_revision_counts_successful_mutations_only() {
    let receiver = TestReceiver::bind().unwrap();
    let project = fixture::project(receiver.addr().port(), &[0.0]);
    let mut engine = Engine::new(project.clone()).unwrap();
    let start = engine.project_revision();
    let expect = |engine: &Engine, bumps: u64| {
        assert_eq!(engine.project_revision(), start + bumps);
    };
    // Playback commands and rejected mutations leave the revision alone.
    engine.execute(Command::Play).unwrap();
    engine.execute(Command::StopAll).unwrap();
    assert!(engine.execute(Command::SetTempo(-1.0)).is_err());
    assert!(
        engine
            .execute(Command::Edit(ProjectEdit::SetTrackDelay {
                track: TrackId(99),
                delay_ms: 1.0
            }))
            .is_err()
    );
    expect(&engine, 0);
    // Each accepted mutation, including the direct-field commands, counts once.
    engine.execute(Command::SetTempo(97.5)).unwrap();
    engine
        .execute(Command::SetFollowActionsEnabled(false))
        .unwrap();
    engine
        .execute(Command::SetGlobalQuantisation(GlobalQuantisation::None))
        .unwrap();
    engine
        .execute(Command::Edit(ProjectEdit::AddTrack { name: "New".into() }))
        .unwrap();
    expect(&engine, 4);
    engine.execute(Command::LoadProject(project)).unwrap();
    expect(&engine, 5);
}

#[test]
fn pasting_a_group_of_tracks_bumps_the_revision_once() {
    let receiver = TestReceiver::bind().unwrap();
    let project = fixture::project(receiver.addr().port(), &[0.0]);
    let mut engine = Engine::new(project).unwrap();
    let start = engine.project_revision();
    let tracks = vec![engine.project().tracks[0].clone(); 1];
    let count = engine.project().tracks.len();
    engine
        .execute(Command::Edit(ProjectEdit::PasteTracks {
            tracks,
            before: None,
            group: None,
        }))
        .unwrap();
    assert_eq!(engine.project().tracks.len(), count + 1);
    assert_eq!(engine.project_revision(), start + 1);
}

#[test]
fn pasting_tracks_that_restore_scenes_bumps_the_revision_only_once() {
    let receiver = TestReceiver::bind().unwrap();
    let project = fixture::project(receiver.addr().port(), &[0.0]);
    let mut engine = Engine::new(project).unwrap();
    let mut copied = engine.project().tracks[0].clone();
    copied.slots.push(copied.slots[0].clone());
    let scenes = engine.project().scenes.len();
    let start = engine.project_revision();
    engine
        .execute(Command::Edit(ProjectEdit::PasteTracks {
            tracks: vec![copied],
            before: None,
            group: None,
        }))
        .unwrap();
    assert_eq!(engine.project().scenes.len(), scenes + 1);
    assert_eq!(engine.project_revision(), start + 1);
    engine.project().validate().unwrap();
}
