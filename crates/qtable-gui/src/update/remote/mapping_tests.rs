//! Saved mappings through the real host, engine, scheduler and UDP boundary.
use super::*;
use crate::Message;
use qtable_engine::{
    edit::ProjectEdit,
    engine::{Engine, Snapshot},
    model::*,
};
use qtable_remote::Endpoint;
use qtable_test_support::TestReceiver;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime};

fn volume() -> FieldRef {
    FieldRef::volume(TrackId(1))
}

fn argument(key: &str) -> FieldRef {
    FieldRef {
        parameter: ParameterRef {
            track: TrackId(1),
            device: DeviceRef::Chain(DeviceId(5)),
            key: ParameterKey("values".into()),
        },
        field: FieldKey(key.into()),
    }
}

fn saved_project(port: u16) -> Project {
    let mut fields = vec![
        FieldDef::new(FieldKey("float".into()), FieldType::Float, true),
        FieldDef::new(FieldKey("int".into()), FieldType::Int, true),
        FieldDef::new(FieldKey("toggle".into()), FieldType::Bool, true),
        FieldDef::new(FieldKey("enum".into()), FieldType::Int, true),
    ];
    fields[1].range = Some([0.0, 10.0]);
    fields[3].control = Control::Enum;
    fields[3].options = (0..3)
        .map(|value| EnumOption {
            label: value.to_string(),
            value: OscArgument::Int(value),
        })
        .collect();
    let mut target = TargetDevice::default();
    target.parameters.push(ParameterDef {
        key: EntryKey("values".into()),
        name: "Values".into(),
        path: OscAddress::Relative("/values".into()),
        fields,
        send_mode: SendMode::OnRelease,
    });
    target.events.push(EventDef {
        key: EntryKey("ping".into()),
        name: "Ping".into(),
        path: OscAddress::Relative("/ping".into()),
        fields: Vec::new(),
    });
    let mut device = NewDevice::Target.build(DeviceId(5));
    device.kind = DeviceKind::Target(target);
    let mut track = Track::new(TrackId(1), "Mapped", 1);
    track.route = Route::single(TrackId(2));
    track.devices.push(device);
    track.mixer_device.volume.output.enabled = true;
    track.mixer_device.volume.output.send_mode = SendMode::OnRelease;
    track.slots[0].clip = Some(
        Clip::new(
            ClipId(1),
            "Clip",
            BeatTime::from_beats(4).unwrap(),
            track.colour,
        )
        .unwrap(),
    );
    let mut project = Project {
        tracks: vec![track],
        outputs: vec![Track::new_output(TrackId(2), "Out", "127.0.0.1", port)],
        scenes: vec![Scene {
            id: SceneId(10),
            name: "Scene".into(),
            follow_actions: Default::default(),
        }],
        global_quantisation: GlobalQuantisation::None,
        ..Project::default()
    };
    let mut add = |message, control: ControlRef, min, max| {
        let mut mapping = RemoteMapping::new(
            MappingId(project.mappings.len() as u64 + 1),
            Source::Midi(MidiSource {
                channel: 1,
                message,
            }),
            control,
            min,
            max,
        );
        mapping.takeover = Takeover::None;
        project.mappings.push(mapping);
    };
    add(MidiMessage::Cc(1), ControlRef::Argument(volume()), 0.0, 1.0);
    add(
        MidiMessage::Cc(1),
        ControlRef::Argument(argument("float")),
        0.0,
        1.0,
    );
    add(
        MidiMessage::Cc(2),
        ControlRef::Argument(argument("int")),
        0.0,
        10.0,
    );
    add(
        MidiMessage::Cc(3),
        ControlRef::Argument(argument("toggle")),
        0.0,
        1.0,
    );
    add(
        MidiMessage::Cc(4),
        ControlRef::Argument(argument("enum")),
        0.0,
        2.0,
    );
    add(
        MidiMessage::Cc(5),
        ControlRef::Event {
            track: TrackId(1),
            event: EventRef {
                device: DeviceId(5),
                key: EntryKey("ping".into()),
            },
        },
        0.0,
        1.0,
    );
    add(
        MidiMessage::Note(60),
        ControlRef::ClipSlot {
            track: TrackId(1),
            scene: SceneId(10),
        },
        0.0,
        1.0,
    );
    add(
        MidiMessage::Note(61),
        ControlRef::Scene(SceneId(10)),
        0.0,
        1.0,
    );
    for (note, control) in [
        (62, TransportControl::Play),
        (63, TransportControl::Stop),
        (64, TransportControl::StopAll),
        (65, TransportControl::ReEnableAutomation),
    ] {
        add(
            MidiMessage::Note(note),
            ControlRef::Transport(control),
            0.0,
            1.0,
        );
    }
    add(
        MidiMessage::Cc(6),
        ControlRef::Transport(TransportControl::Tempo),
        60.0,
        180.0,
    );
    // Use the real format-10 writer/reader so the host consumes saved rows.
    static NEXT_FILE: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "qtable-mapping-host-{}-{}-{}.ron",
        std::process::id(),
        NEXT_FILE.fetch_add(1, Ordering::Relaxed),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    qtable_engine::file::save(&path, &project).unwrap();
    let loaded = qtable_engine::file::load(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    loaded
}

fn input(app: &mut App, messages: &[[u8; 3]]) {
    for message in messages {
        app.remote.ingress.push_midi(
            &Endpoint::Midi {
                id: "test".into(),
                name: "Controller".into(),
            },
            message,
        );
    }
    let _ = crate::update::update(app, Message::Remote(Action::Wake));
}

fn wait(app: &mut App, condition: impl Fn(&Snapshot) -> bool) {
    let started = Instant::now();
    loop {
        app.engine.poll_events();
        app.snapshot = app.engine.snapshot();
        if condition(&app.snapshot) {
            return;
        }
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "scheduler state did not arrive: {:?}",
            app.snapshot
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

mod feedback_tests;

#[test]
fn saved_mappings_drive_values_bangs_slots_scenes_and_transport() {
    let receiver = TestReceiver::bind().unwrap();
    let project = saved_project(receiver.addr().port());
    let mut app = App::new(Engine::new(project).unwrap(), "120.00".into(), None);
    let _ = crate::update::update(&mut app, Message::TempoChanged("131".into()));
    input(&mut app, &[[0xb0, 1, 64]]);
    assert_eq!(app.project_revision(), 1, "fan-out is one atomic batch");
    assert_eq!(app.project().field(&volume()).unwrap().value, 64.0 / 127.0);
    assert_eq!(
        app.project().field(&argument("float")).unwrap().value,
        64.0 / 127.0
    );
    assert_eq!(
        app.tempo_input, "131",
        "remote wake must preserve the typing draft"
    );
    assert!(app.is_dirty());
    assert!(
        app.parameter_gesture
            .just_submitted(&volume(), Instant::now())
    );
    assert!(
        app.parameter_gesture
            .just_submitted(&argument("float"), Instant::now())
    );
    let packets = receiver
        .wait_for_messages(2, Duration::from_secs(2))
        .unwrap();
    assert!(packets.iter().any(|(_, message)| message.addr == "/volume"));
    assert!(packets.iter().any(|(_, message)| message.addr == "/values"));
    // Hardware edits bypass the GUI-only On Release hold on BOTH parameter kinds.
    input(&mut app, &[[0xb0, 2, 127], [0xb0, 3, 127], [0xb0, 4, 64]]);
    assert_eq!(app.project().field(&argument("int")).unwrap().value, 10.0);
    assert_eq!(app.project().field(&argument("toggle")).unwrap().value, 1.0);
    assert_eq!(app.project().field(&argument("enum")).unwrap().value, 1.0);
    receiver
        .wait_for_messages(1, Duration::from_secs(2))
        .unwrap();
    input(&mut app, &[[0xb0, 5, 0], [0xb0, 5, 127], [0xb0, 5, 0]]);
    let packets = receiver
        .wait_for_messages(1, Duration::from_secs(2))
        .unwrap();
    assert_eq!(packets[0].1.addr, "/ping");
    input(&mut app, &[[0x90, 60, 100]]);
    wait(&mut app, |snapshot| {
        snapshot
            .tracks
            .iter()
            .any(|track| track.id == TrackId(1) && track.playing_slot == Some(0))
    });
    input(&mut app, &[[0x90, 64, 100]]);
    wait(&mut app, |snapshot| {
        snapshot
            .tracks
            .iter()
            .all(|track| track.playing_slot.is_none())
    });
    input(&mut app, &[[0x90, 61, 100]]);
    wait(&mut app, |snapshot| {
        snapshot.last_triggered_scene == Some(SceneId(10))
    });
    input(&mut app, &[[0x90, 63, 100]]);
    wait(&mut app, |snapshot| !snapshot.playing);
    input(&mut app, &[[0x90, 62, 100]]);
    wait(&mut app, |snapshot| snapshot.playing);
    input(&mut app, &[[0x90, 65, 100], [0xb0, 6, 127]]);
    assert_eq!(app.project().tempo.bpm(), 180.0);
    assert_eq!(
        app.tempo_input, "131",
        "mapped tempo must not replace an active draft"
    );
}

#[test]
fn project_replacement_clears_pickup_even_when_mapping_ids_are_reused() {
    let mut project = saved_project(9000);
    project.outputs.clear();
    project.tracks[0].route = Route::empty();
    project.mappings.truncate(1);
    project.mappings[0].takeover = Takeover::PickUp;
    project.tracks[0].mixer_device.volume.value = 0.5;
    let mut app = App::new(Engine::new(project.clone()).unwrap(), "120".into(), None);
    input(&mut app, &[[0xb0, 1, 0], [0xb0, 1, 127]]);
    assert_eq!(app.project().field(&volume()).unwrap().value, 1.0);
    app.engine.execute(Command::LoadProject(project)).unwrap();
    app.reset_for_project();
    input(&mut app, &[[0xb0, 1, 0]]);
    assert_eq!(app.project().field(&volume()).unwrap().value, 0.5);
    // A rejected batch must still report the engine's error through the usual path.
    let revision = app.project_revision();
    crate::edit_actions::execute_edit(
        &mut app,
        Command::Edit(ProjectEdit::SetParameters(vec![(
            ParameterRef::volume(TrackId(1)),
            vec![(FieldKey("value".into()), 2.0)],
        )])),
    );
    assert_eq!(app.project_revision(), revision);
    assert!(app.status.message().is_some());
}

#[test]
fn pickup_holds_a_fader_until_it_crosses_the_value_an_undo_restored() {
    let mut project = saved_project(9000);
    project.outputs.clear();
    project.tracks[0].route = Route::empty();
    project.mappings.truncate(1);
    project.mappings[0].takeover = Takeover::PickUp;
    project.tracks[0].mixer_device.volume.value = 0.5;
    let mut app = App::new(Engine::new(project).unwrap(), "120".into(), None);
    let stored = |app: &App| app.project().field(&volume()).unwrap().value;
    // The fader crosses 0.5, takes hold, and moves the value up and back down to 0. These moves
    // of one control are one undo step.
    for value in [0, 127, 0] {
        input(&mut app, &[[0xb0, 1, value]]);
    }
    assert_eq!(stored(&app), 0.0);

    // Undo restores the value before the first move, 0.5, under a fader resting at 0.
    let _ = crate::update::update(
        &mut app,
        Message::KeyPressed {
            key: iced::keyboard::Key::Character("z".into()),
            modifiers: iced::keyboard::Modifiers::COMMAND,
            repeat: false,
            ignored: true,
        },
    );
    assert_eq!(stored(&app), 0.5);

    // The fader has not crossed 0.5 yet, so it takes no hold.
    input(&mut app, &[[0xb0, 1, 20]]);
    assert_eq!(
        stored(&app),
        0.5,
        "a fader short of the restored value moves nothing"
    );
    // It crosses the value and takes hold again.
    input(&mut app, &[[0xb0, 1, 127], [0xb0, 1, 64]]);
    assert!((stored(&app) - 64.0 / 127.0).abs() < 1e-9);
}

#[test]
fn map_mode_leaves_pickup_and_hardware_history_at_the_last_applied_drain() {
    for caught in [false, true] {
        let mut project = saved_project(9000);
        project.outputs.clear();
        project.tracks[0].route = Route::empty();
        project.mappings.truncate(1);
        project.mappings[0].takeover = Takeover::PickUp;
        project.tracks[0].mixer_device.volume.value = 0.5;
        let mut app = App::new(Engine::new(project).unwrap(), "120".into(), None);
        input(&mut app, &[[0xb0, 1, 0]]);
        if caught {
            input(&mut app, &[[0xb0, 1, 127]]);
        }
        let value = app.project().field(&volume()).unwrap().value;
        let revision = app.project_revision();
        app.map_mode.on = true;
        input(&mut app, &[[0xb0, 1, if caught { 64 } else { 127 }]]);
        assert_eq!(app.project_revision(), revision);
        assert_eq!(app.project().field(&volume()).unwrap().value, value);
        app.map_mode.on = false;
        let hardware = if caught { 32 } else { 64 };
        input(&mut app, &[[0xb0, 1, hardware]]);
        assert_eq!(
            app.project().field(&volume()).unwrap().value,
            f64::from(hardware) / 127.0,
            "Map Mode must preserve both caught={caught} and pre-learn hardware history"
        );
    }
}

#[test]
fn mapped_edits_do_not_erase_the_ingress_overload_error() {
    let mut project = saved_project(9000);
    project.outputs.clear();
    project.tracks[0].route = Route::empty();
    let mut app = App::new(Engine::new(project).unwrap(), "120".into(), None);
    for _ in 0..qtable_remote::ingress::CONTINUOUS_CAPACITY + 9 {
        app.remote
            .ingress
            .push_midi(&Endpoint::Keyboard, &[0xb0, 1, 64]);
    }
    update(&mut app, Action::Wake);
    assert_eq!(app.project().field(&volume()).unwrap().value, 64.0 / 127.0);
    assert_eq!(
        app.status.message(),
        Some("MIDI input overloaded: 9 messages dropped")
    );
}

#[test]
fn pickup_uses_effective_automation_and_same_stored_value_still_overrides() {
    use qtable_engine::scheduler::AutomationState;
    let mut project = saved_project(9000);
    project.outputs.clear();
    project.tracks[0].route = Route::empty();
    project.mappings.retain(|mapping| {
        matches!(mapping.control,
        ControlRef::Argument(ref field) if *field == volume())
            || matches!(
                mapping.control,
                ControlRef::ClipSlot { .. }
                    | ControlRef::Transport(TransportControl::ReEnableAutomation)
            )
    });
    project.mappings[0].takeover = Takeover::PickUp;
    project.tracks[0].slots[0]
        .clip
        .as_mut()
        .unwrap()
        .lanes
        .push(Lane::Envelope(AutomationEnvelope {
            target: ArgumentRef::volume(),
            points: vec![Breakpoint {
                time: BeatTime::ZERO,
                value: 0.75,
                curve: 0.0,
            }],
            region: None,
        }));
    let mut app = App::new(Engine::new(project).unwrap(), "120".into(), None);
    input(&mut app, &[[0x90, 60, 100]]);
    let state = |snapshot: &Snapshot, expected| {
        snapshot
            .automation
            .iter()
            .any(|value| value.field == volume() && value.state == expected)
    };
    wait(&mut app, |snapshot| {
        state(snapshot, AutomationState::Following)
    });
    let revision = app.project_revision();
    input(&mut app, &[[0xb0, 1, 0]]);
    assert_eq!(
        app.project_revision(),
        revision,
        "hardware has not crossed the effective 0.75"
    );
    input(&mut app, &[[0xb0, 1, 127]]);
    assert_eq!(
        app.project_revision(),
        revision + 1,
        "writing the unchanged stored 1.0 must still override automation"
    );
    wait(&mut app, |snapshot| {
        state(snapshot, AutomationState::Overridden)
    });
    input(&mut app, &[[0x90, 65, 100]]);
    wait(&mut app, |snapshot| {
        state(snapshot, AutomationState::Following)
    });
    input(&mut app, &[[0xb0, 1, 100]]);
    assert_eq!(
        app.project_revision(),
        revision + 1,
        "re-enable released Pick-Up"
    );
    input(&mut app, &[[0xb0, 1, 0]]);
    assert_eq!(app.project().field(&volume()).unwrap().value, 0.0);
    wait(&mut app, |snapshot| {
        state(snapshot, AutomationState::Overridden)
    });
}
