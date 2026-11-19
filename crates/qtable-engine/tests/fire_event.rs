use std::time::{Duration, Instant};

use qtable_engine::edit::{ProjectEdit, TargetEdit};
use qtable_engine::engine::{Command, Engine, EngineError};
use qtable_engine::model::*;
use qtable_test_support::TestReceiver;
use rosc::OscType;

fn project(receiver: &TestReceiver) -> (Project, EventRef) {
    let mut track = Track::new(TrackId(1), "Track", 0);
    track.route = Route::single(TrackId(2));
    track.path = Path::new("/track").unwrap();
    let hit = track.hit(
        BeatTime::ZERO,
        OscAddress::Relative("/cue".into()),
        vec![
            OscArgument::Int(7),
            OscArgument::Float(0.25),
            OscArgument::Bool(true),
            OscArgument::String("fire".into()),
        ],
        || DeviceId(1),
    );
    let definition = track.event_def_mut(&hit.definition).unwrap();
    for (field, value) in definition.fields.iter_mut().zip(hit.arguments) {
        field.default = value;
        field.value = None;
        field.text = None;
        field.seed_current();
    }
    let target = track.devices[0].target_mut().unwrap();
    target.path = Path::new("/target").unwrap();
    let mut output = Track::new_output(TrackId(2), "Out", "127.0.0.1", receiver.addr().port());
    output.path = Path::new("/out").unwrap();
    (
        Project {
            tracks: vec![track],
            outputs: vec![output],
            ..Project::default()
        },
        hit.definition,
    )
}

fn fire(engine: &mut Engine, event: &EventRef) {
    let revision = engine.project_revision();
    let report = engine
        .execute(Command::FireEvent {
            track: TrackId(1),
            event: event.clone(),
        })
        .unwrap();
    assert_eq!(engine.project_revision(), revision);
    assert_eq!(report, Default::default());
}

fn silent(receiver: &TestReceiver) {
    assert!(
        receiver
            .wait_for_messages(1, Duration::from_millis(80))
            .is_err()
    );
}

#[test]
fn fires_current_values_once_while_stopped_without_changing_the_set_or_transport() {
    let receiver = TestReceiver::bind().unwrap();
    let (p, event) = project(&receiver);
    let saved = ron::to_string(&p).unwrap();
    let mut engine = Engine::new(p).unwrap();
    fire(&mut engine, &event);
    let messages = receiver
        .wait_for_messages(1, Duration::from_secs(2))
        .unwrap();
    assert_eq!(messages[0].1.addr, "/out/track/target/cue");
    assert_eq!(
        messages[0].1.args,
        vec![
            OscType::Int(7),
            OscType::Float(0.25),
            OscType::Bool(true),
            OscType::String("fire".into())
        ]
    );
    assert!(!engine.snapshot().playing);
    assert_eq!(engine.snapshot().current_beat, BeatTime::ZERO);
    assert_eq!(ron::to_string(engine.project()).unwrap(), saved);
    silent(&receiver);
}

#[test]
fn fans_out_with_track_main_and_output_delays_and_clamps_negative_delay() {
    let receiver = TestReceiver::bind().unwrap();
    let second = TestReceiver::bind().unwrap();
    let (mut p, event) = project(&receiver);
    p.main.delay_ms = 25.0;
    p.tracks[0].delay_ms = 175.0;
    p.outputs[0].delay_ms = 50.0;
    p.outputs.push(Track::new_output(
        TrackId(3),
        "Early",
        "127.0.0.1",
        second.addr().port(),
    ));
    p.outputs[1].delay_ms = -500.0;
    p.tracks[0].route = Route::Outputs(vec![TrackId(2), TrackId(3)]);
    let mut engine = Engine::with_diagnostics(p, true).unwrap();
    let started = Instant::now();
    fire(&mut engine, &event);
    let early = second.wait_for_messages(1, Duration::from_secs(2)).unwrap();
    let delayed = receiver
        .wait_for_messages(1, Duration::from_secs(2))
        .unwrap();
    assert!(delayed[0].0.duration_since(started) >= Duration::from_millis(245));
    assert!(early[0].0 < delayed[0].0);
    assert_eq!(early[0].1.addr, "/track/target/cue");
    let due: Vec<_> = engine
        .scheduler_events()
        .try_iter()
        .filter_map(|e| match e {
            qtable_engine::scheduler::SchedulerEvent::Sent { due, .. } => Some(due),
            _ => None,
        })
        .collect();
    assert_eq!(due.len(), 2);
    assert_eq!(due[1].duration_since(due[0]), Duration::from_millis(250));
    silent(&receiver);
    silent(&second);
}

#[test]
fn muted_unsoloed_inactive_and_unrouted_events_send_nothing() {
    let receiver = TestReceiver::bind().unwrap();
    for case in 0..5 {
        let (mut p, event) = project(&receiver);
        match case {
            0 => p.tracks[0].mixer.active = false,
            1 => {
                let mut other = Track::new(TrackId(3), "Solo", 0);
                other.mixer.solo = true;
                p.tracks.push(other);
            }
            2 => p.tracks[0].devices[0].active = false,
            3 => p.outputs[0].mixer.active = false,
            4 => p.tracks[0].route = Route::Outputs(vec![]),
            _ => unreachable!(),
        }
        let mut engine = Engine::new(p).unwrap();
        fire(&mut engine, &event);
        silent(&receiver);
        assert!(!engine.snapshot().playing);
    }
}

#[test]
fn bundled_manual_hits_report_sent_once_each_after_they_go() {
    let receiver = TestReceiver::bind().unwrap();
    let (mut p, event) = project(&receiver);
    p.outputs[0].output.as_mut().unwrap().stage.bundle = true;
    let mut engine = Engine::with_diagnostics(p, true).unwrap();
    for _ in 0..3 {
        fire(&mut engine, &event);
    }
    receiver
        .wait_for_messages(3, Duration::from_secs(2))
        .unwrap();
    // The flush's Sent reports follow its datagrams; allow the worker a moment to queue them.
    std::thread::sleep(Duration::from_millis(50));
    let sent = engine
        .scheduler_events()
        .try_iter()
        .filter(|e| matches!(e, qtable_engine::scheduler::SchedulerEvent::Sent { .. }))
        .count();
    assert_eq!(sent, 3);
    silent(&receiver);
}

#[test]
fn absolute_zero_field_event_uses_event_budget_and_bundle_while_stopped() {
    let receiver = TestReceiver::bind().unwrap();
    let (mut p, event) = project(&receiver);
    let definition = p.tracks[0].event_def_mut(&event).unwrap();
    definition.path = OscAddress::Absolute("/bang".into());
    definition.fields.clear();
    let settings = &mut p.outputs[0].output.as_mut().unwrap().stage;
    settings.frame_rate = 1.0;
    settings.bundle = true;
    settings.budget_on = true;
    settings.budget = 10;
    let mut engine = Engine::new(p).unwrap();
    for _ in 0..5 {
        fire(&mut engine, &event);
    }
    let messages = receiver
        .wait_for_messages(5, Duration::from_secs(2))
        .unwrap();
    assert!(
        messages
            .iter()
            .all(|(_, m)| m.addr == "/bang" && m.args.is_empty())
    );
    // Publication follows dispatch on the next worker pass.
    let until = Instant::now() + Duration::from_secs(1);
    loop {
        let snapshot = engine.snapshot();
        if let Some((_, counts)) = snapshot
            .output_throughput
            .iter()
            .find(|(id, _)| *id == TrackId(2))
            && counts.messages == 5
        {
            assert_eq!(counts.deferred, 0);
            assert_eq!(counts.skipped, 0);
            assert!(snapshot.active_tracks.contains(&TrackId(1)));
            break;
        }
        assert!(
            Instant::now() < until,
            "one-shots must count in output throughput"
        );
        std::thread::yield_now();
    }
    silent(&receiver);
}

#[test]
fn rejects_unknown_track_device_key_and_cross_track_refs_without_mutating() {
    let receiver = TestReceiver::bind().unwrap();
    let (mut p, event) = project(&receiver);
    p.tracks.push(Track::new(TrackId(3), "Other", 0));
    let mut engine = Engine::new(p).unwrap();
    for (track, reference) in [
        (TrackId(99), event.clone()),
        (TrackId(3), event.clone()),
        (
            TrackId(1),
            EventRef {
                device: DeviceId(99),
                ..event.clone()
            },
        ),
        (
            TrackId(1),
            EventRef {
                key: EntryKey("missing".into()),
                ..event.clone()
            },
        ),
    ] {
        assert!(matches!(
            engine.execute(Command::FireEvent {
                track,
                event: reference
            }),
            Err(EngineError::UnknownEvent { .. })
        ));
    }
    assert_eq!(engine.project_revision(), 0);
    silent(&receiver);
}

#[test]
fn queued_values_survive_edits_and_transport_stop() {
    let receiver = TestReceiver::bind().unwrap();
    let (mut p, event) = project(&receiver);
    p.tracks[0].delay_ms = 200.0;
    let mut engine = Engine::new(p).unwrap();
    engine.execute(Command::Play).unwrap();
    fire(&mut engine, &event);
    engine
        .execute(Command::Edit(
            qtable_engine::edit::ProjectEdit::EditTarget {
                device: event.device,
                edit: qtable_engine::edit::TargetEdit::SetFieldDefault {
                    entry: event.key.clone(),
                    field: FieldKey("f1".into()),
                    default: OscArgument::Int(99),
                },
            },
        ))
        .unwrap();
    engine.execute(Command::Stop).unwrap();
    let messages = receiver
        .wait_for_messages(1, Duration::from_secs(2))
        .unwrap();
    assert_eq!(messages[0].1.args[0], OscType::Int(7));
    assert!(!engine.snapshot().playing);
    fire(&mut engine, &event);
    let messages = receiver
        .wait_for_messages(1, Duration::from_secs(2))
        .unwrap();
    // A changed default leaves the current value alone.
    assert_eq!(messages[0].1.args[0], OscType::Int(7));
    silent(&receiver);
}

fn set_value(engine: &mut Engine, event: &EventRef, field: &str, value: OscArgument) {
    engine
        .execute(Command::Edit(ProjectEdit::EditTarget {
            device: event.device,
            edit: TargetEdit::SetFieldValue {
                entry: event.key.clone(),
                field: FieldKey(field.into()),
                value,
            },
        }))
        .unwrap();
}

#[test]
fn the_bang_sends_edited_current_values_and_leaves_the_defaults() {
    let receiver = TestReceiver::bind().unwrap();
    let (p, event) = project(&receiver);
    let mut engine = Engine::new(p).unwrap();
    set_value(&mut engine, &event, "f1", OscArgument::Int(42));
    set_value(&mut engine, &event, "f2", OscArgument::Float(0.75));
    set_value(&mut engine, &event, "f3", OscArgument::Bool(false));
    set_value(
        &mut engine,
        &event,
        "f4",
        OscArgument::String("edited".into()),
    );
    let (_, definition) = engine.project().tracks[0].event_def(&event).unwrap();
    let defaults: Vec<_> = definition
        .fields
        .iter()
        .map(|f| f.default.clone())
        .collect();
    assert_eq!(
        defaults,
        [
            OscArgument::Int(7),
            OscArgument::Float(0.25),
            OscArgument::Bool(true),
            OscArgument::String("fire".into())
        ]
    );
    fire(&mut engine, &event);
    let messages = receiver
        .wait_for_messages(1, Duration::from_secs(2))
        .unwrap();
    assert_eq!(
        messages[0].1.args,
        vec![
            OscType::Int(42),
            OscType::Float(0.75),
            OscType::Bool(false),
            OscType::String("edited".into())
        ]
    );
    silent(&receiver);
}

#[test]
fn a_value_edit_is_checked_against_the_field() {
    let receiver = TestReceiver::bind().unwrap();
    let (p, event) = project(&receiver);
    let mut engine = Engine::new(p).unwrap();
    for (field, value) in [
        ("f1", OscArgument::Float(1.0)),
        ("f4", OscArgument::Int(1)),
        ("missing", OscArgument::Int(1)),
    ] {
        assert!(
            engine
                .execute(Command::Edit(ProjectEdit::EditTarget {
                    device: event.device,
                    edit: TargetEdit::SetFieldValue {
                        entry: event.key.clone(),
                        field: FieldKey(field.into()),
                        value,
                    },
                }))
                .is_err()
        );
    }
}

#[test]
fn deleting_event_cancels_delayed_hit_even_when_its_key_is_reused() {
    for recreate in [false, true] {
        let receiver = TestReceiver::bind().unwrap();
        let (mut p, event) = project(&receiver);
        p.tracks[0].delay_ms = 500.0;
        let mut engine = Engine::new(p).unwrap();
        fire(&mut engine, &event);
        engine
            .execute(Command::Edit(ProjectEdit::EditTarget {
                device: event.device,
                edit: TargetEdit::RemoveEntries {
                    keys: vec![event.key.clone()],
                },
            }))
            .unwrap();
        if recreate {
            engine
                .execute(Command::Edit(ProjectEdit::EditTarget {
                    device: event.device,
                    edit: TargetEdit::AddEvent {
                        path: OscAddress::Relative("/cue".into()),
                        name: String::new(),
                    },
                }))
                .unwrap();
            assert!(engine.project().tracks[0].event_def(&event).is_some());
        }
        assert!(
            receiver
                .wait_for_messages(1, Duration::from_millis(650))
                .is_err()
        );
        if recreate {
            fire(&mut engine, &event);
            let messages = receiver
                .wait_for_messages(1, Duration::from_secs(2))
                .unwrap();
            assert!(messages[0].1.args.is_empty());
            silent(&receiver);
        }
    }
}

#[test]
fn replacing_device_with_reused_id_cancels_delayed_hit() {
    let receiver = TestReceiver::bind().unwrap();
    let (mut p, event) = project(&receiver);
    p.tracks[0].delay_ms = 500.0;
    let device = p.tracks[0].devices[0].clone();
    let mut engine = Engine::new(p).unwrap();
    fire(&mut engine, &event);
    engine
        .execute(Command::Edit(ProjectEdit::RemoveDevices {
            devices: vec![event.device],
        }))
        .unwrap();
    engine
        .execute(Command::Edit(ProjectEdit::PasteDevices {
            track: TrackId(1),
            devices: vec![device],
            before: None,
        }))
        .unwrap();
    assert!(engine.project().tracks[0].event_def(&event).is_some());
    assert!(
        receiver
            .wait_for_messages(1, Duration::from_millis(650))
            .is_err()
    );
    fire(&mut engine, &event);
    let messages = receiver
        .wait_for_messages(1, Duration::from_secs(2))
        .unwrap();
    assert_eq!(messages[0].1.args[0], OscType::Int(7));
    silent(&receiver);
}

#[test]
fn replacing_output_with_reused_id_cancels_delayed_hit() {
    let receiver = TestReceiver::bind().unwrap();
    let (mut p, event) = project(&receiver);
    p.tracks[0].delay_ms = 500.0;
    let mut engine = Engine::new(p).unwrap();
    fire(&mut engine, &event);
    engine
        .execute(Command::Edit(ProjectEdit::RemoveTracks {
            tracks: vec![TrackId(2)],
        }))
        .unwrap();
    engine
        .execute(Command::Edit(ProjectEdit::AddOutput {
            name: "Replacement".into(),
            host: "127.0.0.1".into(),
            port: receiver.addr().port(),
        }))
        .unwrap();
    assert_eq!(engine.project().outputs[0].id, TrackId(2));
    engine
        .execute(Command::Edit(ProjectEdit::SetTrackRoutes {
            routes: vec![(TrackId(1), Route::single(TrackId(2)))],
        }))
        .unwrap();
    assert!(
        receiver
            .wait_for_messages(1, Duration::from_millis(650))
            .is_err()
    );
    fire(&mut engine, &event);
    let messages = receiver
        .wait_for_messages(1, Duration::from_secs(2))
        .unwrap();
    assert_eq!(messages[0].1.args[0], OscType::Int(7));
    silent(&receiver);
}

#[test]
fn a_set_saved_without_event_values_loads_with_them_seeded_from_the_defaults() {
    let receiver = TestReceiver::bind().unwrap();
    let (mut p, event) = project(&receiver);
    let (_, definition) = p.tracks[0].event_def(&event).unwrap();
    assert_eq!(definition.fields[0].value, Some(7.0));
    let dir = std::env::temp_dir().join(format!("qtable-seed-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("set.qtable");
    p.tracks[0].event_def_mut(&event).unwrap().fields[2].default = OscArgument::Bool(false);
    p.tracks[0].event_def_mut(&event).unwrap().fields[2].value = Some(0.0);
    qtable_engine::file::save(&path, &p).unwrap();
    let stored = std::fs::read_to_string(&path).unwrap();
    let mut dropped = 0;
    let stripped: String = stored
        .lines()
        .filter(|line| {
            let drop = [
                "value: 7.0,",
                "value: 0.25,",
                "value: 0.0,",
                "text: \"fire\",",
            ]
            .contains(&line.trim());
            dropped += usize::from(drop);
            !drop
        })
        .map(|line| format!("{line}\n"))
        .collect();
    assert_eq!(dropped, 4, "{stored}");
    std::fs::write(&path, stripped).unwrap();
    let loaded = qtable_engine::file::load(&path).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    let (_, definition) = loaded.tracks[0].event_def(&event).unwrap();
    let current: Vec<_> = definition
        .fields
        .iter()
        .map(|f| f.current_argument())
        .collect();
    assert_eq!(
        current,
        [
            OscArgument::Int(7),
            OscArgument::Float(0.25),
            OscArgument::Bool(false),
            OscArgument::String("fire".into())
        ]
    );
}
