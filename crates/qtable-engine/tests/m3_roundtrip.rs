use std::fs;
use std::time::Duration;

use qtable_engine::edit::{Carried, ProjectEdit};
use qtable_engine::engine::{Command, Engine};
use qtable_engine::model::{
    BeatTime, ClipFollowActions, DeviceId, FollowAction, FollowActions, FollowMultiplier,
    GlobalQuantisation, LaunchQuantisation, OscAddress, OscArgument, QuantisationValue,
    SceneFollowActions, SceneId, Track, TrackId,
};
use qtable_test_support::TestReceiver;
use rosc::OscType;

/// A hit at beat 0 whose definition is carried in, as a hit pasted from another track's
/// clip: the engine adopts it into the track's Target.
fn hit(track: TrackId, slot: usize, path: OscAddress, arguments: Vec<OscArgument>) -> ProjectEdit {
    let mut source = Track::new(TrackId(999), "source", 0);
    let event = source.hit(BeatTime::ZERO, path, arguments, || DeviceId(999));
    let (_, definition) = source.event_def(&event.definition).unwrap();
    ProjectEdit::AddEvent {
        track,
        slot,
        carried: Some(Carried::Event {
            from: event.definition.clone(),
            definition: definition.clone(),
        }),
        event,
    }
}

fn edit(engine: &mut Engine, edit: ProjectEdit) {
    engine.execute(Command::Edit(edit)).unwrap();
}

#[test]
fn gui_command_workflow_round_trips_and_launches_osc() {
    let receiver = TestReceiver::bind().unwrap();
    let mut engine = Engine::new(Default::default()).unwrap();
    engine
        .execute(Command::SetGlobalQuantisation(GlobalQuantisation::Value(
            QuantisationValue::Note(qtable_engine::model::NoteValue::Quarter),
        )))
        .unwrap();
    edit(
        &mut engine,
        ProjectEdit::AddTrack {
            name: "Lead".into(),
        },
    );
    edit(
        &mut engine,
        ProjectEdit::AddTrack {
            name: "Bass".into(),
        },
    );
    edit(
        &mut engine,
        ProjectEdit::CreateGroup {
            tracks: vec![TrackId(1), TrackId(2)],
        },
    );
    edit(
        &mut engine,
        ProjectEdit::RenameTrack {
            track: TrackId(3),
            name: "Band".into(),
        },
    );
    edit(
        &mut engine,
        ProjectEdit::SetGroupFolded {
            group: TrackId(3),
            folded: true,
        },
    );
    // Ids share one counter: tracks 1 and 2, the group 3, so the output is 4.
    engine
        .execute(Command::Edit(ProjectEdit::AddOutput {
            name: "Loopback".into(),
            host: "127.0.0.1".into(),
            port: receiver.addr().port(),
        }))
        .unwrap();
    edit(
        &mut engine,
        ProjectEdit::RenameTrack {
            track: TrackId(4),
            name: "Main".into(),
        },
    );
    edit(
        &mut engine,
        ProjectEdit::SetTrackPath {
            track: TrackId(4),
            path: "/qtable".into(),
        },
    );
    edit(
        &mut engine,
        ProjectEdit::AddScene {
            at: 0,
            name: "Verse".into(),
        },
    );
    edit(
        &mut engine,
        ProjectEdit::AddScene {
            at: 1,
            name: "Chorus".into(),
        },
    );
    edit(
        &mut engine,
        ProjectEdit::CreateClip {
            track: TrackId(1),
            slot: 0,
            length: BeatTime::from_beats(1).unwrap(),
        },
    );
    edit(
        &mut engine,
        ProjectEdit::RenameClip {
            track: TrackId(1),
            slot: 0,
            name: "Verse cue".into(),
        },
    );
    edit(
        &mut engine,
        ProjectEdit::SetClipLooping {
            track: TrackId(1),
            slot: 0,
            looping: false,
        },
    );
    edit(
        &mut engine,
        ProjectEdit::SetClipQuantisation {
            track: TrackId(1),
            slot: 0,
            quantisation: LaunchQuantisation::None,
        },
    );
    edit(
        &mut engine,
        ProjectEdit::SetClipFollowActions {
            track: TrackId(1),
            slot: 0,
            follow_actions: ClipFollowActions {
                actions: FollowActions {
                    enabled: true,
                    action_a: FollowAction::Next,
                    action_b: FollowAction::Jump(SceneId(2)),
                    chance_a: 3,
                    chance_b: 1,
                },
                linked: false,
                multiplier: FollowMultiplier::new(2, 1).unwrap(),
                time: BeatTime::from_beats(2).unwrap(),
            },
        },
    );
    edit(
        &mut engine,
        hit(
            TrackId(1),
            0,
            OscAddress::Relative("/cue".into()),
            vec![
                OscArgument::Int(7),
                OscArgument::Float(0.25),
                OscArgument::String("hello".into()),
                OscArgument::Bool(true),
            ],
        ),
    );
    edit(
        &mut engine,
        ProjectEdit::CreateClip {
            track: TrackId(2),
            slot: 1,
            length: BeatTime::from_beats(1).unwrap(),
        },
    );
    edit(
        &mut engine,
        ProjectEdit::SetClipQuantisation {
            track: TrackId(2),
            slot: 1,
            quantisation: LaunchQuantisation::None,
        },
    );
    edit(
        &mut engine,
        hit(
            TrackId(2),
            1,
            OscAddress::Absolute("/scene".into()),
            vec![
                OscArgument::Int(9),
                OscArgument::Float(0.5),
                OscArgument::String("scene".into()),
                OscArgument::Bool(false),
            ],
        ),
    );
    edit(
        &mut engine,
        ProjectEdit::SetSlotStopButton {
            track: TrackId(2),
            slot: 0,
            enabled: false,
        },
    );
    edit(
        &mut engine,
        ProjectEdit::SetSceneFollowActions {
            scene: SceneId(1),
            follow_actions: SceneFollowActions {
                actions: FollowActions {
                    enabled: true,
                    action_a: FollowAction::Next,
                    action_b: FollowAction::Stop,
                    chance_a: 2,
                    chance_b: 1,
                },
                time: BeatTime::from_beats(3).unwrap(),
            },
        },
    );
    engine
        .execute(Command::Edit(ProjectEdit::SetTrackRoutes {
            routes: vec![(TrackId(3), qtable_engine::model::Route::single(TrackId(4)))],
        }))
        .unwrap();
    engine
        .execute(Command::Edit(ProjectEdit::SetTrackRoutes {
            routes: vec![(TrackId(1), qtable_engine::model::Route::single(TrackId(4)))],
        }))
        .unwrap();
    engine
        .execute(Command::Edit(ProjectEdit::SetTrackDelay {
            track: TrackId(1),
            delay_ms: -2.5,
        }))
        .unwrap();
    engine
        .execute(Command::Edit(ProjectEdit::SetTrackRoutes {
            routes: vec![(TrackId(2), qtable_engine::model::Route::single(TrackId(4)))],
        }))
        .unwrap();
    engine
        .execute(Command::Edit(ProjectEdit::SetTrackDelay {
            track: TrackId(2),
            delay_ms: 1.25,
        }))
        .unwrap();

    engine.execute(Command::SetTempo(600.0)).unwrap();
    engine.execute(Command::Play).unwrap();
    engine
        .execute(Command::LaunchClip {
            track: TrackId(1),
            slot: 0,
        })
        .unwrap();
    let clip_message = receiver
        .wait_for_messages(1, Duration::from_secs(2))
        .unwrap();
    assert_eq!(clip_message[0].1.addr, "/qtable/cue");
    assert_eq!(
        clip_message[0].1.args,
        vec![
            OscType::Int(7),
            OscType::Float(0.25),
            OscType::String("hello".into()),
            OscType::Bool(true)
        ]
    );
    engine
        .execute(Command::LaunchScene { scene: SceneId(2) })
        .unwrap();
    let scene_message = receiver
        .wait_for_messages(1, Duration::from_secs(2))
        .unwrap();
    assert_eq!(scene_message[0].1.addr, "/scene");
    assert_eq!(
        scene_message[0].1.args,
        vec![
            OscType::Int(9),
            OscType::Float(0.5),
            OscType::String("scene".into()),
            OscType::Bool(false)
        ]
    );
    engine.execute(Command::StopAll).unwrap();

    let expected = engine.project().clone();
    let path = std::env::temp_dir().join(format!("qtable-m3-roundtrip-{}.ron", std::process::id()));
    qtable_engine::file::save(&path, &expected).unwrap();
    let loaded = qtable_engine::file::load(&path).unwrap();
    fs::remove_file(&path).unwrap();
    assert_eq!(loaded, expected);
    let mut reopened = Engine::new(Default::default()).unwrap();
    reopened
        .execute(Command::LoadProject(loaded.clone()))
        .unwrap();
    assert_eq!(reopened.project(), &loaded);
}
