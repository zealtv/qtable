use super::*;
use crate::Endpoint;
use qtable_engine::{
    edit::TargetEdit,
    engine::{Engine, Receipt},
};
use std::time::Instant;

fn volume(track: u64) -> FieldRef {
    FieldRef::volume(TrackId(track))
}

fn source(cc: u8) -> Source {
    Source::Midi(MidiSource {
        channel: 1,
        message: MidiMessage::Cc(cc),
    })
}

fn binding(id: u64, source: Source, control: ControlRef) -> RemoteMapping {
    let mut mapping = RemoteMapping::new(MappingId(id), source, control, 0.0, 1.0);
    mapping.takeover = Takeover::None;
    mapping
}

fn project() -> Project {
    let mut target = TargetDevice::default();
    target.events.push(EventDef {
        key: EntryKey("ping".into()),
        name: "Ping".into(),
        path: OscAddress::Relative("/ping".into()),
        fields: Vec::new(),
    });
    let mut track = Track::new(TrackId(1), "First", 1);
    let mut device = NewDevice::Target.build(DeviceId(5));
    device.kind = DeviceKind::Target(target);
    track.devices.push(device);
    Project {
        tracks: vec![track, Track::new(TrackId(2), "Second", 1)],
        scenes: vec![Scene {
            id: SceneId(10),
            name: String::new(),
            follow_actions: Default::default(),
        }],
        ..Project::default()
    }
}

fn port(id: &str) -> Endpoint {
    Endpoint::Midi {
        id: id.into(),
        name: "Same Name".into(),
    }
}

fn midi_on(id: &str, bytes: &[u8]) -> Envelope {
    Envelope {
        endpoint: port(id),
        received_at: Instant::now(),
        sequence: 1,
        input: Input::Midi(bytes.into()),
    }
}

fn midi(bytes: &[u8]) -> Envelope {
    midi_on("first", bytes)
}

fn resolve(resolver: &mut MappingResolver, project: &Project, events: &[Envelope]) -> Vec<Command> {
    let snapshot = Engine::new(project.clone()).unwrap().snapshot();
    resolve_with_snapshot(resolver, project, &snapshot, events)
}

fn resolve_with_snapshot(
    resolver: &mut MappingResolver,
    project: &Project,
    snapshot: &Snapshot,
    events: &[Envelope],
) -> Vec<Command> {
    let mut commands = Vec::new();
    resolver.receive(events, project, snapshot, &mut commands);
    commands.into_iter().map(|command| command.value).collect()
}

fn values(command: &Command) -> Vec<(FieldRef, f64)> {
    let Command::Edit(ProjectEdit::SetParameters(parameters)) = command else {
        panic!("expected parameter batch: {command:?}");
    };
    parameters
        .iter()
        .flat_map(|(parameter, fields)| {
            fields.iter().map(|(field, value)| {
                (
                    FieldRef {
                        parameter: parameter.clone(),
                        field: field.clone(),
                    },
                    *value,
                )
            })
        })
        .collect()
}

#[test]
fn fanout_is_one_batch_in_list_order_and_repeated_values_are_folded() {
    let mut project = project();
    project.mappings = vec![
        binding(1, source(22), ControlRef::Argument(volume(2))),
        binding(2, source(22), ControlRef::Argument(volume(1))),
    ];
    let commands = resolve(
        &mut MappingResolver::default(),
        &project,
        &[midi(&[0xb0, 22, 0]), midi(&[0xb0, 22, 64])],
    );
    assert_eq!(commands.len(), 1);
    assert_eq!(
        values(&commands[0]),
        [(volume(2), 64.0 / 127.0), (volume(1), 64.0 / 127.0)]
    );
    let mut engine = Engine::new(project).unwrap();
    for command in commands {
        engine.execute(command).unwrap();
    }
    assert_eq!(
        engine.project().field(&volume(1)).unwrap().value,
        64.0 / 127.0
    );
    assert_eq!(engine.project_revision(), 1);
}

#[test]
fn source_index_reconciles_source_changes_removal_and_list_order_each_drain() {
    let mut project = project();
    project.mappings = vec![
        binding(1, source(1), ControlRef::Argument(volume(1))),
        binding(2, source(1), ControlRef::Argument(volume(2))),
    ];
    let mut resolver = MappingResolver::default();
    resolve(&mut resolver, &project, &[midi(&[0xb0, 1, 64])]);
    project.mappings.swap(0, 1);
    project.mappings[1].source = source(2);
    let commands = resolve(&mut resolver, &project, &[midi(&[0xb0, 1, 127])]);
    assert_eq!(values(&commands[0]), [(volume(2), 1.0)]);
    project.mappings[0].source = source(2);
    let commands = resolve(&mut resolver, &project, &[midi(&[0xb0, 2, 127])]);
    assert_eq!(values(&commands[0]), [(volume(2), 1.0), (volume(1), 1.0)]);
    project.mappings.remove(0);
    let commands = resolve(&mut resolver, &project, &[midi(&[0xb0, 2, 127])]);
    assert_eq!(values(&commands[0]), [(volume(1), 1.0)]);
    assert!(resolve(&mut resolver, &project, &[midi(&[0xb0, 1, 127])]).is_empty());
}

#[test]
fn relative_reversal_at_both_bounds_is_interpreted_before_reduction() {
    let mut project = project();
    let mut mapping = binding(1, source(1), ControlRef::Argument(volume(1)));
    mapping.mode = SourceMode::Relative(RelativeEncoding::TwosComplement);
    mapping.takeover = Takeover::PickUp; // inactive for relative
    project.mappings.push(mapping);
    for (start, messages, expected) in [
        (0.95, [8, 120], 1.0 - 50.0 / 256.0),
        (0.05, [120, 8], 50.0 / 256.0),
    ] {
        project.tracks[0].mixer_device.volume.value = start;
        let commands = resolve(
            &mut MappingResolver::default(),
            &project,
            &messages.map(|value| midi(&[0xb0, 1, value])),
        );
        assert_eq!(commands.len(), 1);
        assert_eq!(values(&commands[0]), [(volume(1), expected)]);
    }
}

fn bang() -> ControlRef {
    ControlRef::Event {
        track: TrackId(1),
        event: EventRef {
            device: DeviceId(5),
            key: EntryKey("ping".into()),
        },
    }
}

#[test]
fn bang_cross_and_return_fires_once_and_history_is_per_port() {
    let mut project = project();
    project.mappings.push(binding(1, source(1), bang()));
    let mut resolver = MappingResolver::default();
    let commands = resolve(
        &mut resolver,
        &project,
        &[
            midi(&[0xb0, 1, 0]),
            midi(&[0xb0, 1, 127]),
            midi(&[0xb0, 1, 0]),
        ],
    );
    assert_eq!(commands.len(), 1);
    assert!(matches!(commands[0], Command::FireEvent { .. }));
    let commands = resolve(
        &mut resolver,
        &project,
        &[
            midi_on("a", &[0xb0, 1, 127]),
            midi_on("b", &[0xb0, 1, 127]),
            midi_on("a", &[0xb0, 1, 127]),
        ],
    );
    assert_eq!(commands.len(), 2);
}

#[test]
fn slot_cc_crossings_cover_63_64_fanout_groups_and_history_between_drains() {
    let mut project = project();
    project.insert_group(Track::new_group(TrackId(3), "Group", 1), &[TrackId(2)]);
    project.mappings = vec![
        binding(
            1,
            source(1),
            ControlRef::ClipSlot {
                track: TrackId(1),
                scene: SceneId(10),
            },
        ),
        binding(
            2,
            source(1),
            ControlRef::ClipSlot {
                track: TrackId(3),
                scene: SceneId(10),
            },
        ),
    ];
    let mut resolver = MappingResolver::default();
    let commands = resolve(
        &mut resolver,
        &project,
        &[
            midi(&[0xb0, 1, 0]),
            midi(&[0xb0, 1, 63]),
            midi(&[0xb0, 1, 64]),
            midi(&[0xb0, 1, 65]),
            midi(&[0xb0, 1, 127]),
            midi(&[0xb0, 1, 64]),
        ],
    );
    assert_eq!(commands.len(), 2);
    assert!(matches!(
        commands[0],
        Command::LaunchClip {
            track: TrackId(1),
            slot: 0
        }
    ));
    assert!(matches!(
        commands[1],
        Command::LaunchGroupSlot {
            group: TrackId(3),
            slot: 0
        }
    ));
    assert!(resolve(&mut resolver, &project, &[midi(&[0xb0, 1, 127])]).is_empty());
    assert!(resolve(&mut resolver, &project, &[midi(&[0xb0, 1, 63])]).is_empty());
    assert_eq!(
        resolve(&mut resolver, &project, &[midi(&[0xb0, 1, 64])]).len(),
        2
    );
    // An unseen endpoint's first high value acts as Note On; another held-high value does not.
    assert_eq!(
        resolve(
            &mut resolver,
            &project,
            &[
                midi_on("second", &[0xb0, 1, 64]),
                midi_on("second", &[0xb0, 1, 127]),
                midi(&[0xb0, 1, 65]),
            ]
        )
        .len(),
        2
    );
}

#[test]
fn discrete_actions_are_barriers_and_fanout_preserves_mapping_order() {
    let mut project = project();
    project.mappings = vec![
        binding(1, source(1), ControlRef::Argument(volume(1))),
        binding(
            2,
            Source::Midi(MidiSource {
                channel: 1,
                message: MidiMessage::Note(60),
            }),
            bang(),
        ),
        binding(
            3,
            Source::Midi(MidiSource {
                channel: 1,
                message: MidiMessage::Note(61),
            }),
            ControlRef::Transport(TransportControl::ReEnableAutomation),
        ),
    ];
    let commands = resolve(
        &mut MappingResolver::default(),
        &project,
        &[
            midi(&[0xb0, 1, 10]),
            midi(&[0x90, 60, 100]),
            midi(&[0xb0, 1, 20]),
            midi(&[0x90, 61, 100]),
            midi(&[0xb0, 1, 30]),
        ],
    );
    assert_eq!(commands.len(), 5);
    assert_eq!(values(&commands[0]), [(volume(1), 10.0 / 127.0)]);
    assert!(matches!(commands[1], Command::FireEvent { .. }));
    assert_eq!(values(&commands[2]), [(volume(1), 20.0 / 127.0)]);
    assert!(matches!(commands[3], Command::ReEnableAutomation { .. }));
    assert_eq!(values(&commands[4]), [(volume(1), 30.0 / 127.0)]);
    project.mappings = vec![
        binding(1, source(1), ControlRef::Argument(volume(1))),
        binding(2, source(1), bang()),
        binding(3, source(1), ControlRef::Argument(volume(2))),
    ];
    let commands = resolve(
        &mut MappingResolver::default(),
        &project,
        &[midi(&[0xb0, 1, 127])],
    );
    assert_eq!(commands.len(), 3);
    assert_eq!(values(&commands[0])[0].0, volume(1));
    assert!(matches!(commands[1], Command::FireEvent { .. }));
    assert_eq!(values(&commands[2])[0].0, volume(2));
}

#[test]
fn pickup_catches_a_crossing_and_return_inside_one_drain() {
    let mut project = project();
    project.tracks[0].mixer_device.volume.value = 0.5;
    let mut mapping = binding(1, source(1), ControlRef::Argument(volume(1)));
    mapping.takeover = Takeover::PickUp;
    project.mappings.push(mapping);
    let commands = resolve(
        &mut MappingResolver::default(),
        &project,
        &[
            midi(&[0xb0, 1, 0]),
            midi(&[0xb0, 1, 127]),
            midi(&[0xb0, 1, 0]),
        ],
    );
    assert_eq!(values(&commands[0]), [(volume(1), 0.0)]);
}

#[test]
fn port_collision_does_not_catch_and_fanout_caught_state_is_per_row() {
    let mut project = project();
    project.tracks[0].mixer_device.volume.value = 0.25;
    project.tracks[1].mixer_device.volume.value = 0.75;
    project.mappings = [1, 2]
        .map(|id| {
            let mut mapping = binding(id, source(1), ControlRef::Argument(volume(id)));
            mapping.takeover = Takeover::PickUp;
            mapping
        })
        .into();
    let mut resolver = MappingResolver::default();
    assert!(
        resolve(
            &mut resolver,
            &project,
            &[midi_on("a", &[0xb0, 1, 0]), midi_on("b", &[0xb0, 1, 127])]
        )
        .is_empty()
    );
    let commands = resolve(&mut resolver, &project, &[midi_on("a", &[0xb0, 1, 64])]);
    assert_eq!(values(&commands[0]), [(volume(1), 64.0 / 127.0)]);
}

#[test]
fn effective_values_external_edits_rejection_and_reload_reconcile() {
    use qtable_engine::scheduler::{AutomationState, AutomationValue};
    let mut project = project();
    project.tracks[0].mixer_device.volume.value = 0.1;
    let mut mapping = binding(1, source(1), ControlRef::Argument(volume(1)));
    mapping.mode = SourceMode::Relative(RelativeEncoding::TwosComplement);
    project.mappings.push(mapping);
    let mut snapshot = Engine::new(project.clone()).unwrap().snapshot();
    snapshot.automation.push(AutomationValue {
        field: volume(1),
        value: 0.7,
        state: AutomationState::Following,
    });
    let mut resolver = MappingResolver::default();
    let commands =
        resolve_with_snapshot(&mut resolver, &project, &snapshot, &[midi(&[0xb0, 1, 1])]);
    assert_eq!(values(&commands[0]), [(volume(1), 0.7 + 1.0 / 256.0)]);
    // Simulate rejection: no command was applied; working state must not carry over.
    let commands = resolve(&mut resolver, &project, &[midi(&[0xb0, 1, 1])]);
    assert_eq!(values(&commands[0]), [(volume(1), 0.1 + 1.0 / 256.0)]);
    project.mappings[0].mode = SourceMode::Absolute;
    project.mappings[0].takeover = Takeover::PickUp;
    project.tracks[0].mixer_device.volume.value = 0.5;
    resolve(&mut resolver, &project, &[midi(&[0xb0, 1, 0])]);
    let commands = resolve(&mut resolver, &project, &[midi(&[0xb0, 1, 127])]);
    assert_eq!(values(&commands[0]), [(volume(1), 1.0)]);
    project.tracks[0].mixer_device.volume.value = 0.2; // external change releases catch
    assert!(resolve(&mut resolver, &project, &[midi(&[0xb0, 1, 100])]).is_empty());
    resolver.reset();
    assert!(resolve(&mut resolver, &project, &[midi(&[0xb0, 1, 0])]).is_empty());
}

#[test]
fn key_repeats_are_suppressed_and_switch_state_folds_between_barriers() {
    let mut project = project();
    project.mappings.push(binding(
        1,
        Source::Key(KeySource {
            key: KeyName("a".into()),
        }),
        ControlRef::TrackSolo(TrackId(1)),
    ));
    let key = |pressed, repeat| Envelope {
        endpoint: Endpoint::Keyboard,
        received_at: Instant::now(),
        sequence: 1,
        input: Input::Key {
            key: "a".into(),
            pressed,
            repeat,
        },
    };
    let commands = resolve(
        &mut MappingResolver::default(),
        &project,
        &[
            key(true, false),
            key(true, true),
            key(true, false),
            key(false, false),
            key(true, false),
        ],
    );
    assert_eq!(commands.len(), 2);
    assert!(matches!(
        &commands[0],
        Command::Edit(ProjectEdit::SetSolo { solo: true, .. })
    ));
    assert!(matches!(
        &commands[1],
        Command::Edit(ProjectEdit::SetSolo { solo: false, .. })
    ));
}

#[test]
fn view_actions_preserve_parameter_segments_and_input_order() {
    use crate::ViewAction;
    let mut project = project();
    project.mappings = vec![
        binding(1, source(1), ControlRef::Argument(volume(1))),
        binding(
            2,
            source(2),
            ControlRef::SceneNavigation(SceneNavigation::Down),
        ),
        binding(
            3,
            source(3),
            ControlRef::SceneNavigation(SceneNavigation::Launch),
        ),
    ];
    let snapshot = Engine::new(project.clone()).unwrap().snapshot();
    let mut actions = Vec::new();
    MappingResolver::default().receive_actions(
        &[
            midi(&[0xb0, 1, 20]),
            midi(&[0xb0, 2, 127]),
            midi(&[0xb0, 3, 127]),
            midi(&[0xb0, 1, 30]),
        ],
        &project,
        &snapshot,
        &mut actions,
    );
    assert_eq!(actions.len(), 4);
    assert!(
        matches!(&actions[0].value, Action::Command(c) if values(c) == [(volume(1), 20.0 / 127.0)])
    );
    assert!(matches!(
        actions[1].value,
        Action::View(ViewAction::MoveScene(1))
    ));
    assert!(matches!(
        actions[2].value,
        Action::View(ViewAction::LaunchScene)
    ));
    assert!(
        matches!(&actions[3].value, Action::Command(c) if values(c) == [(volume(1), 30.0 / 127.0)])
    );
}

#[test]
fn discrete_receipts_survive_fanout_view_actions_and_both_router_drains() {
    use crate::{Ingress, Router, ViewAction};
    let mut project = project();
    project.mappings = vec![
        binding(1, source(1), ControlRef::Argument(volume(1))),
        binding(
            2,
            source(1),
            ControlRef::ClipSlot {
                track: TrackId(1),
                scene: SceneId(10),
            },
        ),
        binding(
            3,
            source(1),
            ControlRef::ClipSlot {
                track: TrackId(2),
                scene: SceneId(10),
            },
        ),
        binding(
            4,
            source(1),
            ControlRef::SceneNavigation(SceneNavigation::Launch),
        ),
    ];
    let engine = Engine::new(project.clone()).unwrap();
    let snapshot = engine.snapshot();
    for commands_only in [false, true] {
        let ingress = Ingress::default();
        ingress.push_midi(&port("first"), &[0xb0, 1, 127]);
        let mut router = Router::default();
        router.subscribe(MappingResolver::default());
        let (drain, receipts) = if commands_only {
            let (drain, commands) = router.drain(&ingress, &project, &snapshot);
            assert_eq!(commands.len(), 3);
            assert!(matches!(
                commands[1].value,
                Command::LaunchClip {
                    track: TrackId(1),
                    ..
                }
            ));
            assert!(matches!(
                commands[2].value,
                Command::LaunchClip {
                    track: TrackId(2),
                    ..
                }
            ));
            (
                drain,
                commands.into_iter().map(|c| c.receipt).collect::<Vec<_>>(),
            )
        } else {
            let (drain, actions) = router.drain_actions(&ingress, &project, &snapshot, None);
            assert_eq!(actions.len(), 4);
            assert!(matches!(
                actions[3].value,
                Action::View(ViewAction::LaunchScene)
            ));
            (
                drain,
                actions.into_iter().map(|a| a.receipt).collect::<Vec<_>>(),
            )
        };
        assert_eq!(
            receipts[0], None,
            "a value batch uses P, never a constituent R"
        );
        let expected = qtable_engine::engine::Receipt {
            received_at: drain.events[0].received_at,
            sequence: Some(drain.events[0].sequence),
        };
        assert!(
            receipts[1..]
                .iter()
                .all(|receipt| *receipt == Some(expected))
        );
    }
}

fn received(
    resolver: &mut MappingResolver,
    project: &Project,
    events: &[Envelope],
) -> Vec<Received<Command>> {
    let snapshot = Engine::new(project.clone()).unwrap().snapshot();
    let mut commands = Vec::new();
    resolver.receive(events, project, &snapshot, &mut commands);
    commands
}

#[test]
fn a_cc_moving_a_number_is_continuous_and_a_fanout_from_one_source_stays_one_batch() {
    let mut project = project();
    project.mappings = vec![
        binding(1, source(22), ControlRef::Argument(volume(1))),
        binding(2, source(22), ControlRef::Argument(volume(2))),
    ];
    let commands = received(
        &mut MappingResolver::default(),
        &project,
        &[midi(&[0xb0, 22, 10]), midi(&[0xb0, 22, 90])],
    );
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].continuous, Some(source(22)));
    assert_eq!(values(&commands[0].value).len(), 2);
}

#[test]
fn two_sources_in_one_drain_make_two_batches_each_with_its_own_source() {
    let mut project = project();
    project.mappings = vec![
        binding(1, source(1), ControlRef::Argument(volume(1))),
        binding(2, source(2), ControlRef::Argument(volume(2))),
    ];
    let commands = received(
        &mut MappingResolver::default(),
        &project,
        &[
            midi(&[0xb0, 1, 40]),
            midi(&[0xb0, 2, 50]),
            midi(&[0xb0, 1, 60]),
        ],
    );
    assert_eq!(commands.len(), 2);
    assert_eq!(commands[0].continuous, Some(source(1)));
    assert_eq!(values(&commands[0].value), [(volume(1), 60.0 / 127.0)]);
    assert_eq!(commands[1].continuous, Some(source(2)));
    assert_eq!(values(&commands[1].value), [(volume(2), 50.0 / 127.0)]);
}

#[test]
fn relative_pitch_bend_and_tempo_moves_are_continuous_but_switches_notes_and_keys_are_not() {
    let note = Source::Midi(MidiSource {
        channel: 1,
        message: MidiMessage::Note(60),
    });
    let bend = Source::Midi(MidiSource {
        channel: 1,
        message: MidiMessage::PitchBend,
    });
    let key = Source::Key(KeySource {
        key: KeyName("a".into()),
    });
    let mut project = project();
    let mut tempo = RemoteMapping::new(
        MappingId(9),
        source(7),
        ControlRef::Transport(TransportControl::Tempo),
        60.0,
        180.0,
    );
    tempo.takeover = Takeover::None;
    let mut relative = binding(1, source(3), ControlRef::Argument(volume(1)));
    relative.mode = SourceMode::Relative(RelativeEncoding::TwosComplement);
    project.mappings = vec![
        relative,
        binding(2, bend.clone(), ControlRef::Argument(volume(2))),
        tempo,
        // Switches: a CC crossing a window, a note and a key press.
        binding(3, source(4), ControlRef::TrackSolo(TrackId(1))),
        binding(4, note, ControlRef::TrackSolo(TrackId(2))),
        binding(5, key, ControlRef::TrackActivator(TrackId(1))),
    ];
    let commands = received(
        &mut MappingResolver::default(),
        &project,
        &[
            midi(&[0xb0, 3, 1]),
            midi(&[0xe0, 0, 64]),
            midi(&[0xb0, 7, 64]),
            midi(&[0xb0, 4, 127]),
            midi(&[0x90, 60, 100]),
            Envelope {
                endpoint: Endpoint::Keyboard,
                received_at: Instant::now(),
                sequence: 1,
                input: Input::Key {
                    key: "a".into(),
                    pressed: true,
                    repeat: false,
                },
            },
        ],
    );
    let continuous: Vec<_> = commands.iter().map(|c| c.continuous.clone()).collect();
    assert_eq!(
        continuous,
        [
            Some(source(3)),
            Some(bend),
            Some(source(7)),
            None,
            None,
            None
        ],
        "{commands:?}"
    );
}

// --- A toggling pad, as of 2026-11-15 -------------------------
//
// A pad in toggle mode sends note-on on press 1 and note-off on press 2, then repeats. These
// tests drive that pair of messages, one drain per message as a host would, through a resolver
// and a real engine whose accepted state feeds the next drain, and pin what each control kind
// does today.

const PAD_ON: [u8; 3] = [0x90, 60, 100];
const PAD_OFF: [u8; 3] = [0x80, 60, 0];
/// The other way to say note-off: note-on with velocity 0.
const PAD_OFF_VELOCITY_0: [u8; 3] = [0x90, 60, 0];

fn pad_source() -> Source {
    Source::Midi(MidiSource {
        channel: 1,
        message: MidiMessage::Note(60),
    })
}

fn gate() -> FieldRef {
    FieldRef {
        parameter: ParameterRef {
            track: TrackId(1),
            device: DeviceRef::Chain(DeviceId(5)),
            key: ParameterKey("gate".into()),
        },
        field: FieldKey("value".into()),
    }
}

fn pad_project() -> Project {
    let mut project = project();
    let DeviceKind::Target(target) = &mut project.tracks[0].devices[0].kind;
    target.parameters.push(ParameterDef {
        key: EntryKey("gate".into()),
        name: "Gate".into(),
        path: OscAddress::Relative("/gate".into()),
        fields: vec![FieldDef::new(
            FieldKey("value".into()),
            FieldType::Bool,
            true,
        )],
        send_mode: SendMode::Continuous,
    });
    project
}

/// Every control kind the table covers, with the range Map Mode's learn gives a new mapping
/// (the Activator's window, Tempo's 20 to 999; the rest start from the whole control).
fn pad_controls() -> Vec<(&'static str, ControlRef, f64, f64)> {
    use TransportControl::*;
    vec![
        ("Bang event", bang(), 0.0, 1.0),
        (
            "Argument, Float (volume)",
            ControlRef::Argument(volume(1)),
            0.0,
            1.0,
        ),
        (
            "Argument, Bool (gate)",
            ControlRef::Argument(gate()),
            0.0,
            1.0,
        ),
        (
            "Track Activator",
            ControlRef::TrackActivator(TrackId(1)),
            64.0 / 127.0,
            1.0,
        ),
        ("Track Solo", ControlRef::TrackSolo(TrackId(1)), 0.0, 1.0),
        (
            "Clip slot",
            ControlRef::ClipSlot {
                track: TrackId(1),
                scene: SceneId(10),
            },
            0.0,
            1.0,
        ),
        ("Scene", ControlRef::Scene(SceneId(10)), 0.0, 1.0),
        ("Track Stop", ControlRef::TrackStop(TrackId(1)), 0.0, 1.0),
        (
            "Track Select",
            ControlRef::TrackSelect(TrackId(1)),
            0.0,
            1.0,
        ),
        (
            "Scene Launch",
            ControlRef::SceneNavigation(SceneNavigation::Launch),
            0.0,
            1.0,
        ),
        (
            "Scene Select",
            ControlRef::SceneNavigation(SceneNavigation::Select),
            0.0,
            1.0,
        ),
        ("Transport Play", ControlRef::Transport(Play), 0.0, 1.0),
        ("Transport Stop", ControlRef::Transport(Stop), 0.0, 1.0),
        ("Transport Tempo", ControlRef::Transport(Tempo), 20.0, 999.0),
        (
            "Transport Quantisation",
            ControlRef::Transport(Quantisation),
            0.0,
            13.0,
        ),
        (
            "Transport Follow Actions",
            ControlRef::Transport(FollowActions),
            0.0,
            1.0,
        ),
    ]
}

fn pad_label(action: &Action) -> String {
    match action {
        Action::View(view) => format!("View::{view:?}"),
        Action::Command(Command::Edit(ProjectEdit::SetActive { active, .. })) => {
            format!("SetActive({active})")
        }
        Action::Command(Command::Edit(ProjectEdit::SetSolo { solo, .. })) => {
            format!("SetSolo({solo})")
        }
        Action::Command(Command::Edit(ProjectEdit::SetParameters(parameters))) => {
            let values: Vec<_> = parameters
                .iter()
                .flat_map(|(_, fields)| fields.iter().map(|(_, value)| format!("{value:.2}")))
                .collect();
            format!("SetParameters({})", values.join(","))
        }
        Action::Command(
            command @ (Command::SetTempo(_)
            | Command::SetGlobalQuantisation(_)
            | Command::SetFollowActionsEnabled(_)),
        ) => format!("{command:?}"),
        Action::Command(command) => format!("{command:?}")
            .split(['(', ' '])
            .next()
            .unwrap()
            .to_owned(),
    }
}

/// What a host does with a drain: execute the commands, so the next drain starts from the
/// accepted state. Returns each message's labels, and the control's value after it.
fn drive_pad(
    project: &Project,
    mapping: RemoteMapping,
    messages: &[&[u8]],
) -> Vec<(Vec<String>, Option<f64>)> {
    let mut project = project.clone();
    project.mappings.push(mapping);
    let control = project.mappings[0].control.clone();
    let mut engine = Engine::new(project).unwrap();
    let mut resolver = MappingResolver::default();
    let mut rows = Vec::new();
    for message in messages {
        let project = engine.project().clone();
        let snapshot = engine.snapshot();
        let mut actions = Vec::new();
        resolver.receive_actions(&[midi(message)], &project, &snapshot, &mut actions);
        let labels: Vec<_> = actions.iter().map(|a| pad_label(&a.value)).collect();
        for action in actions {
            if let Action::Command(command) = action.value {
                engine.execute(command).unwrap();
            }
        }
        let value = describe_value(engine.project(), &control).and_then(|d| d.value);
        rows.push((labels, value));
    }
    rows
}

fn pad_mapping(id: u64, source: Source, control: ControlRef, min: f64, max: f64) -> RemoteMapping {
    let mut mapping = RemoteMapping::new(MappingId(id), source, control, min, max);
    mapping.takeover = Takeover::None;
    mapping
}

/// One line per message: the actions it produced ("-" for none) and the control's value after.
fn pad_summary(rows: &[(Vec<String>, Option<f64>)]) -> String {
    rows.iter()
        .map(|(labels, value)| {
            let actions = if labels.is_empty() {
                "-".to_owned()
            } else {
                labels.join("+")
            };
            match value {
                Some(value) => format!("{actions} => {value}"),
                None => actions,
            }
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn pad_run(name: &str, source: &Source, messages: &[&[u8]]) -> String {
    let (_, control, min, max) = pad_controls()
        .into_iter()
        .find(|(candidate, ..)| *candidate == name)
        .unwrap();
    pad_summary(&drive_pad(
        &pad_project(),
        pad_mapping(1, source.clone(), control, min, max),
        messages,
    ))
}

/// Today (2026-11-15) a note-off changes nothing for any control kind: `Gesture::Release` is
/// dropped in `translate`, so a toggling pad acts on every second press only, the one that
/// sends note-on. Each line: message 1 note-on, 2 note-off, 3 note-on, 4 note-off.
#[test]
fn toggling_pad_today_acts_on_note_on_and_ignores_every_note_off() {
    let on_off = [&PAD_ON[..], &PAD_OFF, &PAD_ON, &PAD_OFF];
    let fires = |action: &str| format!("{action} | - | {action} | -");
    for (name, expected) in [
        ("Bang event", fires("FireEvent")),
        ("Clip slot", fires("LaunchClip")),
        ("Scene", fires("LaunchScene")),
        ("Track Stop", fires("StopTrack")),
        ("Track Select", fires("View::SelectTrack(TrackId(1))")),
        ("Scene Launch", fires("View::LaunchScene")),
        ("Scene Select", fires("View::NextScene")),
        ("Transport Play", fires("Play")),
        ("Transport Stop", fires("Stop")),
        (
            "Argument, Float (volume)",
            "SetParameters(0.00) => 0 | - => 0 | SetParameters(1.00) => 1 | - => 1".into(),
        ),
        (
            "Argument, Bool (gate)",
            "SetParameters(1.00) => 1 | - => 1 | SetParameters(0.00) => 0 | - => 0".into(),
        ),
        (
            "Track Activator",
            "SetActive(false) => 0 | - => 0 | SetActive(true) => 1 | - => 1".into(),
        ),
        (
            "Track Solo",
            "SetSolo(true) => 1 | - => 1 | SetSolo(false) => 0 | - => 0".into(),
        ),
        (
            "Transport Tempo",
            "SetTempo(999.0) => 999 | - => 999 | SetTempo(20.0) => 20 | - => 20".into(),
        ),
        (
            "Transport Quantisation",
            "SetGlobalQuantisation(Value(Note(Half))) => 5 | - => 5 | \
             SetGlobalQuantisation(Value(Triplet(Half))) => 6 | - => 6"
                .into(),
        ),
        (
            "Transport Follow Actions",
            "SetFollowActionsEnabled(false) => 0 | - => 0 | \
             SetFollowActionsEnabled(true) => 1 | - => 1"
                .into(),
        ),
    ] {
        assert_eq!(pad_run(name, &pad_source(), &on_off), expected, "{name}");
    }
}

/// Note-on with velocity 0 and a note-off are the same release, so a pad that ends its
/// press with either behaves identically.
#[test]
fn a_velocity_zero_note_on_releases_like_a_note_off() {
    for (name, ..) in pad_controls() {
        assert_eq!(
            pad_run(
                name,
                &pad_source(),
                &[&PAD_ON, &PAD_OFF_VELOCITY_0, &PAD_ON, &PAD_OFF_VELOCITY_0]
            ),
            pad_run(name, &pad_source(), &[&PAD_ON, &PAD_OFF, &PAD_ON, &PAD_OFF]),
            "{name}"
        );
    }
}

/// A pad that starts in the other phase (note-off first, with no preceding note-on, as after
/// the controller or the app restarts) changes nothing for that message, and the sequence
/// that follows acts as it would from the start.
#[test]
fn a_note_off_without_a_preceding_note_on_changes_nothing_today() {
    for (name, ..) in pad_controls() {
        let after_a_stray_off = drive_pad_rows(name, &[&PAD_OFF, &PAD_ON, &PAD_OFF, &PAD_ON]);
        let from_the_start = drive_pad_rows(name, &[&PAD_ON, &PAD_OFF, &PAD_ON, &PAD_OFF]);
        assert!(
            after_a_stray_off[0].0.is_empty(),
            "{name}: stray note-off acted"
        );
        assert_eq!(after_a_stray_off[1..], from_the_start[..3], "{name}");
    }
}

fn drive_pad_rows(name: &str, messages: &[&[u8]]) -> Vec<(Vec<String>, Option<f64>)> {
    let (_, control, min, max) = pad_controls()
        .into_iter()
        .find(|(candidate, ..)| *candidate == name)
        .unwrap();
    drive_pad(
        &pad_project(),
        pad_mapping(1, pad_source(), control, min, max),
        messages,
    )
}

/// What already works for a latching pad: one that sends CC 127 then CC 0 sets switches,
/// numbers and choices to their two ends in step with the pad, because an Absolute gesture
/// carries the level and a CC 0 is a value. Triggers fire on the rise only, so the CC 0
/// does nothing there (as the note-off does today).
#[test]
fn a_latching_pad_sending_cc_127_then_0_already_follows_the_pad() {
    let (on, off) = ([0xb0u8, 64, 127], [0xb0u8, 64, 0]);
    let cc = [&on[..], &off, &on, &off];
    let follows = |a: &str, b: &str| format!("{a} | {b} | {a} | {b}");
    let rows = [
        (
            "Track Solo",
            follows("SetSolo(true) => 1", "SetSolo(false) => 0"),
        ),
        (
            "Track Activator",
            follows("SetActive(true) => 1", "SetActive(false) => 0"),
        ),
        (
            "Argument, Bool (gate)",
            follows("SetParameters(1.00) => 1", "SetParameters(0.00) => 0"),
        ),
        (
            "Argument, Float (volume)",
            follows("SetParameters(1.00) => 1", "SetParameters(0.00) => 0"),
        ),
        (
            "Transport Follow Actions",
            follows(
                "SetFollowActionsEnabled(true) => 1",
                "SetFollowActionsEnabled(false) => 0",
            ),
        ),
        (
            "Transport Tempo",
            follows("SetTempo(999.0) => 999", "SetTempo(20.0) => 20"),
        ),
        ("Bang event", "FireEvent | - | FireEvent | -".into()),
        ("Clip slot", "LaunchClip | - | LaunchClip | -".into()),
        ("Transport Play", "Play | - | Play | -".into()),
    ];
    for (name, expected) in rows {
        assert_eq!(pad_run(name, &source(64), &cc), expected, "{name}");
    }
}

// Event arguments (Option A): a mapped control sets the value the event's face holds and
// sends nothing; OSC goes out only when the event's Bang fires.

fn ping() -> EventRef {
    EventRef {
        device: DeviceId(5),
        key: EntryKey("ping".into()),
    }
}

fn argument_control(key: &str) -> ControlRef {
    ControlRef::EventArgument {
        track: TrackId(1),
        event: ping(),
        field: FieldKey(key.into()),
    }
}

/// `project()` with one argument of each mappable kind on its "Ping" event, plus Text.
fn project_with_arguments() -> Project {
    let mut project = project();
    let field = |key: &str, value_type, control, range: Option<[f64; 2]>| {
        let mut field = FieldDef::new(FieldKey(key.into()), value_type, false);
        field.control = control;
        field.range = range;
        field
    };
    let mut choice = field("choice", FieldType::String, Control::Enum, None);
    choice.options = ["a", "b", "c"]
        .map(|name| EnumOption {
            label: name.into(),
            value: OscArgument::String(name.into()),
        })
        .into();
    choice.default = OscArgument::String("a".into());
    let mut ranged = field(
        "ranged",
        FieldType::Float,
        Control::Slider,
        Some([2.0, 4.0]),
    );
    ranged.default = OscArgument::Float(2.0);
    ranged.value = Some(2.0);
    let fields = vec![
        ranged,
        field("free", FieldType::Float, Control::NumberBox, None),
        field(
            "count",
            FieldType::Int,
            Control::NumberBox,
            Some([0.0, 10.0]),
        ),
        field("flag", FieldType::Bool, Control::Toggle, None),
        choice,
        field("text", FieldType::String, Control::Text, None),
    ];
    let mut fields = fields;
    for field in &mut fields {
        // The control and options changed after `new` seeded the value: seed it again.
        field.value = None;
        field.text = None;
        field.seed_current();
    }
    project.tracks[0].devices[0].target_mut().unwrap().events[0].fields = fields;
    project
}

fn argument_value(project: &Project, key: &str) -> f64 {
    let (_, event) = project.tracks[0].event_def(&ping()).unwrap();
    let field = event.fields.iter().find(|f| f.key.0 == key).unwrap();
    field.value.unwrap_or_else(|| field.default_value())
}

/// The value an `EditTarget` command sets, with its device, entry and field.
fn field_edit(command: &Command) -> (DeviceId, String, String, OscArgument) {
    let Command::Edit(ProjectEdit::EditTarget {
        device,
        edit:
            TargetEdit::SetFieldValue {
                entry,
                field,
                value,
            },
    }) = command
    else {
        panic!("expected a face edit: {command:?}");
    };
    (*device, entry.0.clone(), field.0.clone(), value.clone())
}

fn apply_all(project: &Project, commands: Vec<Command>) -> Engine {
    let mut engine = Engine::new(project.clone()).unwrap();
    for command in commands {
        engine.execute(command).unwrap();
    }
    engine
}

/// The mapping a learn would store for the argument: its range, or Float 0 to 1, Int 0 to 127.
fn argument_binding(id: u64, source: Source, project: &Project, key: &str) -> RemoteMapping {
    let control = argument_control(key);
    let (min, max) = learn_range(
        &control,
        describe_value(project, &control).as_ref().unwrap(),
    );
    let mut mapping = RemoteMapping::new(MappingId(id), source, control, min, max);
    mapping.takeover = Takeover::None;
    mapping
}

#[test]
fn each_argument_kind_sets_the_face_value_and_sends_no_event() {
    let base = project_with_arguments();
    // (argument, CC value, the value it sets, the OSC argument the edit stores)
    let cases = [
        ("ranged", 127, 4.0, OscArgument::Float(4.0)),
        ("ranged", 0, 2.0, OscArgument::Float(2.0)),
        ("free", 127, 1.0, OscArgument::Float(1.0)),
        ("count", 127, 10.0, OscArgument::Int(10)),
        ("flag", 127, 1.0, OscArgument::Bool(true)),
        ("choice", 127, 2.0, OscArgument::String("c".into())),
        ("choice", 0, 0.0, OscArgument::String("a".into())),
    ];
    for (key, cc, expected, stored) in cases {
        let mut project = base.clone();
        project.mappings = vec![argument_binding(1, source(22), &project, key)];
        let commands = resolve(
            &mut MappingResolver::default(),
            &project,
            &[midi(&[0xb0, 22, cc])],
        );
        assert_eq!(commands.len(), 1, "{key} {cc}: {commands:?}");
        assert!(
            !matches!(commands[0], Command::FireEvent { .. }),
            "{key} {cc} fired the event"
        );
        let (device, entry, field, value) = field_edit(&commands[0]);
        assert_eq!(
            (device, entry.as_str(), field.as_str()),
            (DeviceId(5), "ping", key)
        );
        assert_eq!(value, stored, "{key} {cc}");
        let engine = apply_all(&project, commands);
        assert_eq!(
            argument_value(engine.project(), key),
            expected,
            "{key} {cc}"
        );
    }
}

#[test]
fn a_midi_note_and_a_key_drive_arguments_the_same_way() {
    let base = project_with_arguments();
    let key = Source::Key(KeySource {
        key: KeyName("k".into()),
    });
    let note = Source::Midi(MidiSource {
        channel: 1,
        message: MidiMessage::Note(60),
    });
    let press = Envelope {
        endpoint: port("first"),
        received_at: Instant::now(),
        sequence: 1,
        input: Input::Key {
            key: "k".into(),
            pressed: true,
            repeat: false,
        },
    };
    for (source, press) in [(key, press), (note, midi(&[0x90, 60, 100]))] {
        // A switch toggles; a number goes to its Max, and back to Min on the next press.
        for (name, expected) in [
            ("flag", [1.0, 0.0]),
            ("ranged", [4.0, 2.0]),
            ("count", [127.0f64.min(10.0), 0.0]),
        ] {
            let mut project = base.clone();
            project.mappings = vec![argument_binding(1, source.clone(), &project, name)];
            let mut resolver = MappingResolver::default();
            for expected in expected {
                let commands = resolve(&mut resolver, &project, std::slice::from_ref(&press));
                assert_eq!(commands.len(), 1, "{name}: {commands:?}");
                let engine = apply_all(&project, commands);
                assert_eq!(argument_value(engine.project(), name), expected, "{name}");
                project = engine.project().clone();
                // The decoder holds the key until released; let go for the next press.
                if let Input::Key { key, .. } = &press.input {
                    let up = Envelope {
                        input: Input::Key {
                            key: key.clone(),
                            pressed: false,
                            repeat: false,
                        },
                        ..press.clone()
                    };
                    resolve(&mut resolver, &project, &[up]);
                }
            }
        }
    }
}

#[test]
fn a_text_argument_cannot_be_driven() {
    let mut project = project_with_arguments();
    project.mappings = vec![binding(1, source(22), argument_control("text"))];
    assert!(
        describe_value(&project, &project.mappings[0].control)
            .unwrap()
            .kind
            == ControlKind::Text
    );
    let commands = resolve(
        &mut MappingResolver::default(),
        &project,
        &[midi(&[0xb0, 22, 100])],
    );
    assert!(commands.is_empty(), "{commands:?}");
}

#[test]
fn a_sweep_in_one_drain_is_one_edit_per_argument_with_the_last_value() {
    let mut project = project_with_arguments();
    project.mappings = vec![
        argument_binding(1, source(22), &project, "ranged"),
        argument_binding(2, source(23), &project, "free"),
    ];
    let commands = resolve(
        &mut MappingResolver::default(),
        &project,
        &[
            midi(&[0xb0, 22, 0]),
            midi(&[0xb0, 23, 127]),
            midi(&[0xb0, 22, 64]),
            midi(&[0xb0, 22, 127]),
            midi(&[0xb0, 23, 0]),
        ],
    );
    let edits: Vec<_> = commands.iter().map(field_edit).collect();
    // One edit each, in the order the arguments first moved.
    assert_eq!(edits.len(), 2, "{commands:?}");
    assert_eq!(
        (edits[0].2.as_str(), &edits[0].3),
        ("ranged", &OscArgument::Float(4.0))
    );
    assert_eq!(
        (edits[1].2.as_str(), &edits[1].3),
        ("free", &OscArgument::Float(0.0))
    );
}

#[test]
fn relative_moves_in_one_drain_accumulate_from_the_stored_value() {
    let mut project = project_with_arguments();
    let mut mapping = argument_binding(1, source(22), &project, "free");
    mapping.mode = SourceMode::Relative(RelativeEncoding::TwosComplement);
    project.mappings = vec![mapping];
    // Three clicks up of 1/256 each, from the default 0.
    let commands = resolve(
        &mut MappingResolver::default(),
        &project,
        &[
            midi(&[0xb0, 22, 1]),
            midi(&[0xb0, 22, 1]),
            midi(&[0xb0, 22, 1]),
        ],
    );
    assert_eq!(commands.len(), 1);
    let (_, _, _, OscArgument::Float(value)) = field_edit(&commands[0]) else {
        panic!()
    };
    assert_eq!(value, (3.0 / 256.0) as f32);
}

#[test]
fn the_value_is_written_before_the_bang_of_the_same_drain() {
    let mut project = project_with_arguments();
    project.mappings = vec![
        argument_binding(1, source(22), &project, "ranged"),
        binding(2, source(23), bang()),
    ];
    // Knob, then Bang: the Bang observes the value just set.
    let commands = resolve(
        &mut MappingResolver::default(),
        &project,
        &[midi(&[0xb0, 22, 127]), midi(&[0xb0, 23, 127])],
    );
    assert_eq!(commands.len(), 2, "{commands:?}");
    assert!(matches!(commands[0], Command::Edit(_)), "{commands:?}");
    assert!(matches!(commands[1], Command::FireEvent { .. }));
    // Bang, then knob: the Bang came first and goes first, as a parameter's batch would.
    let commands = resolve(
        &mut MappingResolver::default(),
        &project,
        &[midi(&[0xb0, 23, 127]), midi(&[0xb0, 22, 127])],
    );
    assert!(
        matches!(commands[0], Command::FireEvent { .. }),
        "{commands:?}"
    );
    assert!(matches!(commands[1], Command::Edit(_)));
}

#[test]
fn a_value_edit_carries_its_controller_so_a_sweep_is_one_undo_step() {
    let mut project = project_with_arguments();
    project.mappings = vec![
        argument_binding(1, source(22), &project, "ranged"),
        argument_binding(
            2,
            Source::Midi(MidiSource {
                channel: 1,
                message: MidiMessage::Note(60),
            }),
            &project,
            "flag",
        ),
    ];
    let mut resolver = MappingResolver::default();
    let snapshot = Engine::new(project.clone()).unwrap().snapshot();
    let mut received = Vec::new();
    resolver.receive(
        &[midi(&[0xb0, 22, 64]), midi(&[0x90, 60, 100])],
        &project,
        &snapshot,
        &mut received,
    );
    assert_eq!(received.len(), 2);
    assert_eq!(received[0].continuous, Some(source(22)));
    assert_eq!(
        received[1].continuous, None,
        "a note makes a step of its own"
    );

    // Three drains of one knob, all under the token the host keeps for it: one step.
    let mut engine = Engine::new(project.clone()).unwrap();
    let token = engine.gesture();
    for cc in [10, 70, 120] {
        let mut commands = Vec::new();
        resolver.receive(
            &[midi(&[0xb0, 22, cc])],
            engine.project(),
            &engine.snapshot(),
            &mut commands,
        );
        for command in commands {
            let context = engine
                .command_context(Receipt {
                    received_at: Instant::now(),
                    sequence: None,
                })
                .with_gesture(Some(token));
            engine.execute_with_context(command.value, context).unwrap();
        }
    }
    // The face stores an f32.
    assert_eq!(
        argument_value(engine.project(), "ranged") as f32,
        (2.0 + 2.0 * 120.0 / 127.0) as f32
    );
    engine.execute(Command::Undo).unwrap();
    assert_eq!(
        argument_value(engine.project(), "ranged"),
        2.0,
        "one undo undoes the sweep"
    );
    assert!(!engine.can_undo());
}

#[test]
fn deleting_the_argument_drops_the_mapping_and_undo_brings_it_back() {
    let mut project = project_with_arguments();
    project.mappings = vec![argument_binding(1, source(22), &project, "ranged")];
    let mut engine = Engine::new(project).unwrap();
    engine
        .execute(Command::Edit(ProjectEdit::EditTarget {
            device: DeviceId(5),
            edit: TargetEdit::RemoveField {
                entry: EntryKey("ping".into()),
                field: FieldKey("ranged".into()),
            },
        }))
        .unwrap();
    assert!(engine.project().mappings.is_empty());
    // A row that no longer resolves does nothing.
    engine.execute(Command::Undo).unwrap();
    assert_eq!(engine.project().mappings.len(), 1);
}

// --- Gate (note and key down go to Max or On, up to Min or Off) ---------------------------------

fn gate_mapping(name: &str, min: f64, max: f64) -> RemoteMapping {
    let (_, control, ..) = pad_controls()
        .into_iter()
        .find(|(candidate, ..)| *candidate == name)
        .unwrap();
    RemoteMapping {
        mode: SourceMode::Gate,
        ..pad_mapping(1, pad_source(), control, min, max)
    }
}

fn gate_rows(mapping: RemoteMapping, messages: &[&[u8]]) -> Vec<(Vec<String>, Option<f64>)> {
    drive_pad(&pad_project(), mapping, messages)
}

fn gate_run(mapping: RemoteMapping, messages: &[&[u8]]) -> String {
    pad_summary(&gate_rows(mapping, messages))
}

const GATE_ORDERS: [[&[u8]; 4]; 2] = [
    [&PAD_ON, &PAD_OFF, &PAD_ON, &PAD_OFF],
    [&PAD_OFF, &PAD_ON, &PAD_OFF, &PAD_ON],
];

/// Gate sets a level: after each message the control holds Max (or On) following a note-on and
/// Min (or Off) following a note-off, in both press orders, and a command goes out exactly when
/// that changed the value. `ends` are the values after a note-on and after a note-off.
fn assert_gate_follows(name: &str, mapping: RemoteMapping, ends: (f64, f64)) {
    for messages in GATE_ORDERS {
        let rows = gate_rows(mapping.clone(), &messages);
        // The control's own starting value decides whether the first message changes anything.
        let mut held = None;
        for (message, (labels, value)) in messages.iter().zip(&rows) {
            let level = if *message == &PAD_ON[..] {
                ends.0
            } else {
                ends.1
            };
            assert_eq!(*value, Some(level), "{name}: {}", pad_summary(&rows));
            if let Some(held) = held {
                assert_eq!(
                    labels.is_empty(),
                    held == level,
                    "{name}: a command goes out only when the level changes: {}",
                    pad_summary(&rows)
                );
            }
            held = Some(level);
        }
    }
}

#[test]
fn gate_follows_the_pad_for_switches_in_both_orders() {
    for name in [
        "Track Solo",
        "Track Activator",
        "Argument, Bool (gate)",
        "Transport Follow Actions",
    ] {
        let (_, _, min, max) = pad_controls().into_iter().find(|c| c.0 == name).unwrap();
        assert_gate_follows(name, gate_mapping(name, min, max), (1.0, 0.0));
    }
}

#[test]
fn an_inverted_switch_or_activator_reverses_on_and_off() {
    // Only the order of the ends counts: a note-on switches Off, a note-off switches On.
    for (name, min, max) in [
        ("Track Solo", 1.0, 0.0),
        ("Track Activator", 1.0, 64.0 / 127.0),
        ("Argument, Bool (gate)", 1.0, 0.0),
        ("Transport Follow Actions", 1.0, 0.0),
    ] {
        assert_gate_follows(name, gate_mapping(name, min, max), (0.0, 1.0));
    }
    // An Activator window that excludes 1.0 changes nothing: Gate reads the order, not the ends.
    assert_gate_follows(
        "Track Activator",
        gate_mapping("Track Activator", 0.1, 0.3),
        (1.0, 0.0),
    );
    assert_gate_follows(
        "Track Activator",
        gate_mapping("Track Activator", 0.3, 0.1),
        (0.0, 1.0),
    );
}

#[test]
fn gate_sends_a_number_to_max_and_min_including_a_narrowed_and_an_inverted_range() {
    for (name, min, max) in [
        ("Argument, Float (volume)", 0.0, 1.0),
        ("Argument, Float (volume)", 0.2, 0.8),
        ("Argument, Float (volume)", 0.8, 0.2),
        ("Argument, Float (volume)", 1.0, 0.0),
        ("Transport Tempo", 20.0, 999.0),
        ("Transport Tempo", 120.0, 60.0),
    ] {
        assert_gate_follows(name, gate_mapping(name, min, max), (max, min));
    }
}

#[test]
fn gate_sends_a_choice_to_its_max_and_min_choice() {
    // Quantisation: choices 0 to 13.
    let name = "Transport Quantisation";
    assert_gate_follows(name, gate_mapping(name, 0.0, 13.0), (13.0, 0.0));
    assert_gate_follows(name, gate_mapping(name, 2.0, 5.0), (5.0, 2.0));
    assert_gate_follows(name, gate_mapping(name, 5.0, 2.0), (2.0, 5.0));
    // A range that excludes the end choices stays inside them; a fractional end keeps to
    // the whole choices within it.
    assert_gate_follows(name, gate_mapping(name, 1.5, 4.5), (4.0, 2.0));
}

#[test]
fn a_gate_stray_note_off_or_repeated_note_on_sends_nothing_and_adds_no_undo_step() {
    for (name, _, min, max) in pad_controls() {
        if [
            "Argument, Float (volume)",
            "Track Solo",
            "Transport Tempo",
            "Argument, Bool (gate)",
        ]
        .contains(&name)
        {
            // From a rest at Min: a stray note-off, then on, on again (a pad that resends), off.
            let mut project = pad_project();
            project.mappings.push(gate_mapping(name, min, max));
            let control = project.mappings[0].control.clone();
            let mut engine = Engine::new(project).unwrap();
            let mut resolver = MappingResolver::default();
            let mut steps = 0;
            let mut now = Vec::new();
            for message in [&PAD_OFF, &PAD_OFF, &PAD_ON, &PAD_ON, &PAD_OFF, &PAD_OFF] {
                let mut actions = Vec::new();
                resolver.receive_actions(
                    &[midi(message)],
                    &engine.project().clone(),
                    &engine.snapshot(),
                    &mut actions,
                );
                steps += actions.len();
                for action in actions {
                    if let Action::Command(command) = action.value {
                        engine.execute(command).unwrap();
                    }
                }
                now.push(describe_value(engine.project(), &control).unwrap().value);
            }
            // Off, off, on, on, off, off: the level changes at most at the first message, the
            // note-on and the first note-off after it.
            assert!(steps <= 3, "{name}: {steps} commands for 6 messages");
            assert_eq!(now[1], now[0], "{name}");
            assert_eq!(now[3], now[2], "{name}");
            assert_eq!(now[5], now[4], "{name}");
            assert_ne!(now[2], now[4], "{name}");
        }
    }
}

#[test]
fn a_gate_note_off_with_no_preceding_note_on_sets_min_even_when_the_control_is_on() {
    // Volume starts at 1.0: a stray note-off, with no note-on before it, brings it down. A Bool
    // set on by mouse or by a previous session is switched off the same way.
    let rows = gate_rows(
        gate_mapping("Argument, Float (volume)", 0.0, 1.0),
        &[&PAD_OFF],
    );
    assert_eq!((rows[0].0.len(), rows[0].1), (1, Some(0.0)));
    let mut project = pad_project();
    project
        .mappings
        .push(gate_mapping("Argument, Bool (gate)", 0.0, 1.0));
    let on = apply_all(
        &project,
        resolve(&mut MappingResolver::default(), &project, &[midi(&PAD_ON)]),
    );
    let commands = resolve(
        &mut MappingResolver::default(),
        on.project(),
        &[midi(&PAD_OFF)],
    );
    assert_eq!(commands.len(), 1);
    let off = apply_all(on.project(), commands);
    assert_eq!(
        describe_value(off.project(), &ControlRef::Argument(gate()))
            .unwrap()
            .value,
        Some(0.0)
    );
}

#[test]
fn a_gate_velocity_zero_note_on_is_a_note_off() {
    for name in ["Track Solo", "Argument, Float (volume)", "Transport Tempo"] {
        let (_, _, min, max) = pad_controls().into_iter().find(|c| c.0 == name).unwrap();
        assert_eq!(
            gate_run(
                gate_mapping(name, min, max),
                &[&PAD_ON, &PAD_OFF_VELOCITY_0, &PAD_ON, &PAD_OFF_VELOCITY_0]
            ),
            gate_run(
                gate_mapping(name, min, max),
                &[&PAD_ON, &PAD_OFF, &PAD_ON, &PAD_OFF]
            ),
            "{name}"
        );
    }
    // A note-off with a release velocity is still a note-off.
    let rows = gate_rows(
        gate_mapping("Track Solo", 0.0, 1.0),
        &[&PAD_ON, &[0x80, 60, 90]],
    );
    assert_eq!(rows[1].1, Some(0.0));
}

#[test]
fn a_gate_on_and_off_in_one_drain_end_in_the_last_message_state() {
    for (messages, last) in [
        ([&PAD_ON[..], &PAD_OFF], 0.0),
        ([&PAD_OFF[..], &PAD_ON], 1.0),
    ] {
        let mut project = pad_project();
        project.mappings.push(gate_mapping("Track Solo", 0.0, 1.0));
        let events: Vec<_> = messages.iter().map(|m| midi(m)).collect();
        let commands = resolve(&mut MappingResolver::default(), &project, &events);
        let engine = apply_all(&project, commands);
        let solo = describe_value(engine.project(), &ControlRef::TrackSolo(TrackId(1)));
        assert_eq!(solo.unwrap().value, Some(last));
    }
    // The same drain on a number: the intermediate edits are ordered, the last one wins.
    let mut project = pad_project();
    project
        .mappings
        .push(gate_mapping("Argument, Float (volume)", 0.2, 0.8));
    let commands = resolve(
        &mut MappingResolver::default(),
        &project,
        &[midi(&PAD_OFF), midi(&PAD_ON), midi(&PAD_OFF)],
    );
    let engine = apply_all(&project, commands);
    assert_eq!(
        describe_value(engine.project(), &ControlRef::Argument(volume(1)))
            .unwrap()
            .value,
        Some(0.2)
    );
}

#[test]
fn gate_leaves_triggers_and_latch_neighbours_alone() {
    // A Latch and a Gate mapping on one note, on different controls, act independently; and
    // two Gate mappings on one note both follow it.
    let mut project = pad_project();
    let mut latch = pad_mapping(1, pad_source(), ControlRef::TrackSolo(TrackId(1)), 0.0, 1.0);
    latch.mode = SourceMode::Absolute;
    let mut gate_a = gate_mapping("Argument, Bool (gate)", 0.0, 1.0);
    gate_a.id = MappingId(2);
    let mut gate_b = gate_mapping("Transport Follow Actions", 0.0, 1.0);
    gate_b.id = MappingId(3);
    project.mappings = vec![latch, gate_a, gate_b];
    let mut engine = Engine::new(project).unwrap();
    let mut resolver = MappingResolver::default();
    let mut states = Vec::new();
    for message in [&PAD_ON, &PAD_OFF, &PAD_ON, &PAD_OFF] {
        let mut actions = Vec::new();
        resolver.receive_actions(
            &[midi(message)],
            &engine.project().clone(),
            &engine.snapshot(),
            &mut actions,
        );
        for action in actions {
            if let Action::Command(command) = action.value {
                engine.execute(command).unwrap();
            }
        }
        let read = |control| describe_value(engine.project(), &control).unwrap().value;
        states.push((
            read(ControlRef::TrackSolo(TrackId(1))),
            read(ControlRef::Argument(gate())),
            read(ControlRef::Transport(TransportControl::FollowActions)),
        ));
    }
    // Solo (Latch) flips on each note-on and ignores note-offs; the two Gate rows follow the pad.
    assert_eq!(
        states,
        vec![
            (Some(1.0), Some(1.0), Some(1.0)),
            (Some(1.0), Some(0.0), Some(0.0)),
            (Some(0.0), Some(1.0), Some(1.0)),
            (Some(0.0), Some(0.0), Some(0.0)),
        ]
    );
}

#[test]
fn gate_follows_only_its_own_note_and_channel() {
    let mut project = pad_project();
    project.mappings.push(gate_mapping("Track Solo", 0.0, 1.0));
    let mut resolver = MappingResolver::default();
    for other in [
        &[0x91u8, 60, 100][..], // channel 2
        &[0x90, 61, 100],       // another note
        &[0xb0, 60, 127],       // a CC of the same number
    ] {
        let commands = resolve(&mut resolver, &project, &[midi(other)]);
        assert!(commands.is_empty(), "{other:?}: {commands:?}");
    }
}

fn key_event(key: &str, pressed: bool, repeat: bool) -> Envelope {
    Envelope {
        endpoint: Endpoint::Keyboard,
        received_at: Instant::now(),
        sequence: 1,
        input: Input::Key {
            key: key.into(),
            pressed,
            repeat,
        },
    }
}

#[test]
fn a_gate_key_holds_down_and_releases_up_and_ignores_auto_repeat() {
    let key = Source::Key(KeySource {
        key: KeyName("a".into()),
    });
    for (control, min, max, on, off) in [
        (ControlRef::TrackSolo(TrackId(1)), 0.0, 1.0, 1.0, 0.0),
        (ControlRef::TrackSolo(TrackId(1)), 1.0, 0.0, 0.0, 1.0),
        (ControlRef::Argument(volume(2)), 0.2, 0.8, 0.8, 0.2),
        (ControlRef::Argument(volume(2)), 0.8, 0.2, 0.2, 0.8),
    ] {
        let mut project = project();
        project.mappings = vec![RemoteMapping {
            mode: SourceMode::Gate,
            ..pad_mapping(1, key.clone(), control.clone(), min, max)
        }];
        let mut engine = Engine::new(project).unwrap();
        let mut resolver = MappingResolver::default();
        for (event, expected) in [
            (key_event("a", true, false), on),
            (key_event("a", true, true), on), // auto-repeat
            (key_event("a", false, false), off),
            (key_event("a", false, false), off), // a stray release
            (key_event("a", true, false), on),
            (key_event("a", false, false), off),
        ] {
            let mut commands = Vec::new();
            resolver.receive(
                &[event],
                &engine.project().clone(),
                &engine.snapshot(),
                &mut commands,
            );
            for command in commands {
                engine.execute(command.value).unwrap();
            }
            assert_eq!(
                describe_value(engine.project(), &control).unwrap().value,
                Some(expected),
                "{control:?} {min}..{max}"
            );
        }
    }
}

#[test]
fn a_gate_event_argument_takes_max_down_and_min_up_and_sends_no_osc() {
    let base = project_with_arguments();
    // (argument, the value on a note-on, the value on a note-off)
    for (key, down, up) in [
        ("ranged", 4.0, 2.0),
        ("free", 1.0, 0.0),
        ("count", 10.0, 0.0),
        ("flag", 1.0, 0.0),
        ("choice", 2.0, 0.0),
    ] {
        for source in [pad_source(), key_source("a")] {
            let mut project = base.clone();
            let mut mapping = argument_binding(1, source.clone(), &project, key);
            mapping.mode = SourceMode::Gate;
            project.mappings = vec![mapping];
            let mut engine = Engine::new(project).unwrap();
            let mut resolver = MappingResolver::default();
            let events = |on| match &source {
                Source::Key(_) => key_event("a", on, false),
                _ => midi(if on { &PAD_ON } else { &PAD_OFF }),
            };
            for (on, expected) in [(true, down), (false, up), (true, down), (false, up)] {
                let mut commands = Vec::new();
                resolver.receive(
                    &[events(on)],
                    &engine.project().clone(),
                    &engine.snapshot(),
                    &mut commands,
                );
                for command in commands {
                    assert!(
                        matches!(command.value, Command::Edit(ProjectEdit::EditTarget { .. })),
                        "{key}: only a face edit goes out, never a send: {:?}",
                        command.value
                    );
                    engine.execute(command.value).unwrap();
                }
                assert_eq!(argument_value(engine.project(), key), expected, "{key}");
            }
        }
    }
}

fn key_source(name: &str) -> Source {
    Source::Key(KeySource {
        key: KeyName(name.into()),
    })
}

#[test]
fn each_gate_message_is_a_discrete_undo_step() {
    let mut project = pad_project();
    project
        .mappings
        .push(gate_mapping("Argument, Float (volume)", 0.0, 1.0));
    let mut resolver = MappingResolver::default();
    let mut received = Vec::new();
    resolver.receive(
        &[midi(&PAD_OFF)],
        &project,
        &Engine::new(project.clone()).unwrap().snapshot(),
        &mut received,
    );
    assert_eq!(received.len(), 1);
    assert_eq!(
        received[0].continuous, None,
        "a note makes a step of its own"
    );
}

// Encoder sensitivity (design 4.2): the decoded steps of a relative CC, after the encoding's own
// curve, are multiplied by the mapping's sensitivity.

const ENCODINGS: [RelativeEncoding; 8] = [
    RelativeEncoding::SignedBit,
    RelativeEncoding::SignedBit2,
    RelativeEncoding::BinOffset,
    RelativeEncoding::TwosComplement,
    RelativeEncoding::SignedBitLinear,
    RelativeEncoding::SignedBit2Linear,
    RelativeEncoding::BinOffsetLinear,
    RelativeEncoding::TwosComplementLinear,
];

/// `project_with_arguments()` with the master-free targets sitting mid-range: track 1's volume
/// at 0.5 and the Int argument "count" (0 to 10) at 5.
fn sensitivity_project() -> Project {
    let mut project = project_with_arguments();
    project.tracks[0].mixer_device.volume.value = 0.5;
    let fields = &mut project.tracks[0].devices[0].target_mut().unwrap().events[0].fields;
    fields
        .iter_mut()
        .find(|f| f.key.0 == "count")
        .unwrap()
        .value = Some(5.0);
    project
}

fn relative_row(
    control: ControlRef,
    min: f64,
    max: f64,
    encoding: RelativeEncoding,
    sensitivity: f64,
) -> RemoteMapping {
    let mut mapping = RemoteMapping::new(MappingId(1), source(22), control, min, max);
    mapping.takeover = Takeover::None;
    mapping.mode = SourceMode::Relative(encoding);
    mapping.sensitivity = sensitivity;
    mapping
}

/// The view actions a run of messages produces, for Scene Select.
fn view_actions(
    resolver: &mut MappingResolver,
    project: &Project,
    events: &[Envelope],
) -> Vec<crate::ViewAction> {
    let snapshot = Engine::new(project.clone()).unwrap().snapshot();
    let mut actions = Vec::new();
    resolver.receive_actions(events, project, &snapshot, &mut actions);
    actions
        .into_iter()
        .filter_map(|action| match action.value {
            Action::View(view) => Some(view),
            Action::Command(_) => None,
        })
        .collect()
}

/// What a message moves track 1's volume to: `None` when nothing is sent.
fn volume_after(project: &Project, wire: u8) -> Option<f64> {
    let commands = resolve(
        &mut MappingResolver::default(),
        project,
        &[midi(&[0xb0, 22, wire])],
    );
    commands.first().map(|command| values(command)[0].1)
}

/// What a message moves the Int argument "count" to: `None` when nothing is sent.
fn count_after(project: &Project, wire: u8) -> Option<f64> {
    let commands = resolve(
        &mut MappingResolver::default(),
        project,
        &[midi(&[0xb0, 22, wire])],
    );
    commands.first().map(|command| match field_edit(command).3 {
        OscArgument::Int(value) => f64::from(value),
        other => panic!("expected an Int: {other:?}"),
    })
}

/// 100 % must reproduce, bit for bit, what the resolver did before sensitivity existed. The
/// expected values restate that old rule: a Float moves `steps × (max − min) / 256`, an Int or
/// Choice one unit per step (reversed when min > max), Scene Select `steps` scenes, all clamped.
#[test]
fn at_100_percent_every_encoding_reproduces_todays_values_exactly() {
    let base = sensitivity_project();
    let mut checked = 0;
    for encoding in ENCODINGS {
        for wire in 0..=127u8 {
            let steps = f64::from(crate::midi::decode::relative(wire, encoding));
            // Float volume: full, narrowed and inverted ranges.
            for (min, max) in [(0.0, 1.0), (0.2, 0.8), (0.8, 0.2)] {
                let mut project = base.clone();
                project.mappings = vec![relative_row(
                    ControlRef::Argument(volume(1)),
                    min,
                    max,
                    encoding,
                    1.0,
                )];
                let expected = (steps != 0.0).then(|| {
                    (0.5 + steps * (max - min) / 256.0)
                        .clamp(f64::min(min, max), f64::max(min, max))
                });
                assert_eq!(
                    volume_after(&project, wire),
                    expected,
                    "{encoding:?} {wire} {min}-{max}"
                );
                checked += 1;
            }
            // Int argument: one unit per step, reversed when min > max.
            for (min, max) in [(0.0, 10.0), (10.0, 0.0), (2.0, 7.0)] {
                let mut project = base.clone();
                project.mappings = vec![relative_row(
                    argument_control("count"),
                    min,
                    max,
                    encoding,
                    1.0,
                )];
                let sign = if min > max { -1.0 } else { 1.0 };
                let expected = (steps != 0.0)
                    .then(|| (5.0 + steps * sign).clamp(f64::min(min, max), f64::max(min, max)));
                assert_eq!(
                    count_after(&project, wire),
                    expected,
                    "{encoding:?} {wire} count {min}-{max}"
                );
                checked += 1;
            }
            // Scene Select: the decoded steps, as they are.
            let mut project = base.clone();
            project.mappings = vec![relative_row(
                ControlRef::SceneNavigation(SceneNavigation::Select),
                0.0,
                1.0,
                encoding,
                1.0,
            )];
            let moved = view_actions(
                &mut MappingResolver::default(),
                &project,
                &[midi(&[0xb0, 22, wire])],
            );
            let expected: Vec<_> = (steps != 0.0)
                .then_some(crate::ViewAction::MoveScene(steps as i32))
                .into_iter()
                .collect();
            assert_eq!(moved, expected, "{encoding:?} {wire} scene select");
            checked += 1;
        }
    }
    assert_eq!(checked, 8 * 128 * 7);
}

const TWOS: RelativeEncoding = RelativeEncoding::TwosComplement;
/// Wire values for one click up and one click down under `TWOS`.
const UP: u8 = 1;
const DOWN: u8 = 127;

fn close(left: f64, right: f64) -> bool {
    (left - right).abs() < 1e-12
}

/// Track 1's volume after one message, from `start`, on a fresh resolver.
fn volume_from(start: f64, row: RemoteMapping, wire: u8) -> Option<f64> {
    let mut project = sensitivity_project();
    project.tracks[0].mixer_device.volume.value = start;
    project.mappings = vec![row];
    volume_after(&project, wire)
}

fn volume_row(min: f64, max: f64, encoding: RelativeEncoding, sensitivity: f64) -> RemoteMapping {
    relative_row(
        ControlRef::Argument(volume(1)),
        min,
        max,
        encoding,
        sensitivity,
    )
}

#[test]
fn a_float_moves_sensitivity_times_its_share_of_the_span() {
    // From 0.5, one click up at 200 % and at 50 %: twice and half of 1/256 of the span.
    for (sensitivity, steps) in [(2.0, 2.0), (0.5, 0.5), (10.0, 10.0), (0.01, 0.01)] {
        let row = volume_row(0.0, 1.0, TWOS, sensitivity);
        let moved = volume_from(0.5, row.clone(), UP).unwrap();
        assert!(close(moved, 0.5 + steps / 256.0), "{sensitivity}: {moved}");
        let moved = volume_from(0.5, row, DOWN).unwrap();
        assert!(close(moved, 0.5 - steps / 256.0), "{sensitivity}: {moved}");
    }
}

#[test]
fn a_floats_step_follows_a_narrowed_and_an_inverted_range() {
    // A narrowed span makes the step finer in absolute terms; sensitivity scales that step.
    let moved = volume_from(0.5, volume_row(0.2, 0.8, TWOS, 2.0), UP).unwrap();
    assert!(close(moved, 0.5 + 2.0 * 0.6 / 256.0), "{moved}");
    // Min above Max reverses the direction, and the sensitivity still scales the step.
    let moved = volume_from(0.5, volume_row(0.8, 0.2, TWOS, 2.0), UP).unwrap();
    assert!(close(moved, 0.5 - 2.0 * 0.6 / 256.0), "{moved}");
    let moved = volume_from(0.5, volume_row(0.8, 0.2, TWOS, 0.5), DOWN).unwrap();
    assert!(close(moved, 0.5 + 0.5 * 0.6 / 256.0), "{moved}");
}

#[test]
fn a_float_clamps_at_both_ends_of_its_range_at_any_sensitivity() {
    let high = volume_from(0.999, volume_row(0.0, 1.0, TWOS, 10.0), UP);
    assert_eq!(high, Some(1.0));
    let low = volume_from(0.001, volume_row(0.0, 1.0, TWOS, 10.0), DOWN);
    assert_eq!(low, Some(0.0));
    let narrowed = volume_from(0.79, volume_row(0.2, 0.8, TWOS, 10.0), UP);
    assert_eq!(narrowed, Some(0.8));
}

#[test]
fn sensitivity_multiplies_the_steps_after_the_encodings_curve() {
    // Twos-complement wire value 7 decodes to 20 steps on the measured curve; 3 stays 3.
    let moved = volume_from(0.5, volume_row(0.0, 1.0, TWOS, 0.5), 7).unwrap();
    assert!(close(moved, 0.5 + 10.0 / 256.0), "curved, 50 %: {moved}");
    let moved = volume_from(0.5, volume_row(0.0, 1.0, TWOS, 2.0), 3).unwrap();
    assert!(close(moved, 0.5 + 6.0 / 256.0), "curved, 200 %: {moved}");
    // The Linear encoding keeps 7 as 7.
    let linear = RelativeEncoding::TwosComplementLinear;
    let moved = volume_from(0.5, volume_row(0.0, 1.0, linear, 2.0), 7).unwrap();
    assert!(close(moved, 0.5 + 14.0 / 256.0), "linear, 200 %: {moved}");
    let moved = volume_from(0.5, volume_row(0.0, 1.0, linear, 0.5), 125).unwrap();
    assert!(
        close(moved, 0.5 - 1.5 / 256.0),
        "linear down, 50 %: {moved}"
    );
}

#[test]
fn several_clicks_in_one_drain_accumulate_at_the_set_sensitivity() {
    let mut project = sensitivity_project();
    project.mappings = vec![volume_row(0.0, 1.0, TWOS, 2.0)];
    let click = midi(&[0xb0, 22, UP]);
    let commands = resolve(
        &mut MappingResolver::default(),
        &project,
        &[click.clone(), click.clone(), click],
    );
    assert_eq!(commands.len(), 1);
    assert!(close(values(&commands[0])[0].1, 0.5 + 6.0 / 256.0));
}

fn count_row(min: f64, max: f64, sensitivity: f64) -> RemoteMapping {
    relative_row(argument_control("count"), min, max, TWOS, sensitivity)
}

/// The Int argument "count" (starting at 5) after each wire value, one after another on one
/// resolver, so that a remainder carries between them. The project never applies the edits, so
/// every message starts from `start`.
fn counts(row: RemoteMapping, start: f64, wires: &[u8]) -> Vec<Option<f64>> {
    let mut project = sensitivity_project();
    let fields = &mut project.tracks[0].devices[0].target_mut().unwrap().events[0].fields;
    fields
        .iter_mut()
        .find(|f| f.key.0 == "count")
        .unwrap()
        .value = Some(start);
    project.mappings = vec![row];
    let mut resolver = MappingResolver::default();
    wires
        .iter()
        .map(|&wire| {
            let commands = resolve(&mut resolver, &project, &[midi(&[0xb0, 22, wire])]);
            commands.first().map(|command| match field_edit(command).3 {
                OscArgument::Int(value) => f64::from(value),
                other => panic!("expected an Int: {other:?}"),
            })
        })
        .collect()
}

#[test]
fn at_25_percent_four_clicks_move_an_int_by_one_unit() {
    let up = counts(count_row(0.0, 10.0, 0.25), 5.0, &[UP; 8]);
    assert_eq!(
        up,
        [None, None, None, Some(6.0), None, None, None, Some(6.0)]
    );
    let down = counts(count_row(0.0, 10.0, 0.25), 5.0, &[DOWN; 8]);
    assert_eq!(
        down,
        [None, None, None, Some(4.0), None, None, None, Some(4.0)]
    );
}

#[test]
fn the_remainder_is_signed_so_a_reversal_cancels_what_is_owed() {
    // Three clicks up owe 0.75; a click back leaves 0.5, then 0.25, then nothing: no unit moved.
    let sequence = counts(
        count_row(0.0, 10.0, 0.25),
        5.0,
        &[UP, UP, UP, DOWN, DOWN, DOWN],
    );
    assert_eq!(sequence, [None; 6]);
    // Then four clicks down make exactly one unit down, so the remainder was back at zero.
    let sequence = counts(
        count_row(0.0, 10.0, 0.25),
        5.0,
        &[UP, UP, UP, DOWN, DOWN, DOWN, DOWN, DOWN, DOWN, DOWN],
    );
    assert_eq!(&sequence[6..], [None, None, None, Some(4.0)]);
    // A negative remainder crosses a unit the same way: the fifth click owes a quarter again.
    let sequence = counts(count_row(0.0, 10.0, 0.25), 5.0, &[DOWN; 5]);
    assert_eq!(sequence, [None, None, None, Some(4.0), None]);
}

#[test]
fn spinning_past_max_owes_nothing_so_the_reversal_moves_on_the_fourth_click() {
    // The Int sits at Max (10) at 25 %. Three clicks up would owe 0.75 but can never be paid, so
    // the remainder must not survive: the reversal then needs the usual four clicks, not seven.
    let wires = [UP, UP, UP, DOWN, DOWN, DOWN, DOWN];
    assert_eq!(
        counts(count_row(0.0, 10.0, 0.25), 10.0, &wires),
        [None, None, None, None, None, None, Some(9.0)]
    );
    // The same at Min.
    let wires = [DOWN, DOWN, DOWN, UP, UP, UP, UP];
    assert_eq!(
        counts(count_row(0.0, 10.0, 0.25), 0.0, &wires),
        [None, None, None, None, None, None, Some(1.0)]
    );
    // A click that makes a whole unit against the end also drops the rest it owed. At 175 % the
    // click up from Max owes 0.75 after its unit; the click back then lands on 9 either way, but
    // only without that leftover does the next one owe -1.75 and take two units (to 8), not one.
    assert_eq!(
        counts(count_row(0.0, 10.0, 1.75), 10.0, &[UP, DOWN, DOWN]),
        [Some(10.0), Some(9.0), Some(8.0)]
    );
}

#[test]
fn spinning_past_an_inverted_or_narrowed_end_owes_nothing_either() {
    // Inverted (Min 10, Max 0): a click up moves down, so Max (0) is the end that a spin up hits.
    let wires = [UP, UP, UP, DOWN, DOWN, DOWN, DOWN];
    assert_eq!(
        counts(count_row(10.0, 0.0, 0.25), 0.0, &wires),
        [None, None, None, None, None, None, Some(1.0)]
    );
    // Narrowed to 2..7, with the Int stored at 7.
    assert_eq!(
        counts(
            count_row(2.0, 7.0, 0.25),
            7.0,
            &[UP, UP, UP, DOWN, DOWN, DOWN, DOWN]
        ),
        [None, None, None, None, None, None, Some(6.0)]
    );
}

#[test]
fn a_reversal_in_mid_range_still_cancels_what_is_owed() {
    // Away from an end nothing clamps, so the carry stays: three clicks up owe 0.75, and the three
    // clicks back pay it; the fourth then owes a quarter again, as if the spin never happened.
    let wires = [UP, UP, UP, DOWN, DOWN, DOWN, DOWN, DOWN, DOWN, DOWN];
    assert_eq!(
        counts(count_row(0.0, 10.0, 0.25), 5.0, &wires),
        [
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(4.0)
        ]
    );
    // One unit short of the end it still holds: at 9 three clicks up owe 0.75 and are kept.
    let wires = [UP, UP, UP, DOWN, DOWN, DOWN];
    assert_eq!(counts(count_row(0.0, 10.0, 0.25), 9.0, &wires), [None; 6]);
}

#[test]
fn a_value_outside_a_narrowed_range_still_moves_onto_the_near_end() {
    // Stored 9 above a 2..7 range: four clicks down owe one whole unit, and the clamp lands on 7.
    // Dropping the carry because the clamp matches the stored value would keep it from building.
    assert_eq!(
        counts(count_row(2.0, 7.0, 0.25), 9.0, &[DOWN; 4]),
        [None, None, None, Some(7.0)]
    );
    // Stored 0 below it: four clicks up land on 2.
    assert_eq!(
        counts(count_row(2.0, 7.0, 0.25), 0.0, &[UP; 4]),
        [None, None, None, Some(2.0)]
    );
}

#[test]
fn a_choice_outside_a_narrowed_range_moves_onto_the_near_end() {
    // Quantisation at 25 %, with Min/Max narrowed to start at two entries above the stored value.
    // Four clicks up owe one whole unit, and the clamp lands it on the near end (Min).
    let mut project = sensitivity_project();
    let control = ControlRef::Transport(TransportControl::Quantisation);
    let start = describe_value(&project, &control).unwrap().value.unwrap();
    assert!(start + 2.0 <= 13.0, "default Quantisation moved: {start}");
    project.mappings = vec![relative_row(control, start + 2.0, 13.0, TWOS, 0.25)];
    let mut resolver = MappingResolver::default();
    let chosen: Vec<Option<usize>> = [UP; 4]
        .iter()
        .map(|&wire| {
            let commands = resolve(&mut resolver, &project, &[midi(&[0xb0, 22, wire])]);
            match commands.as_slice() {
                [] => None,
                [Command::SetGlobalQuantisation(c)] => {
                    GlobalQuantisation::CHOICES.iter().position(|x| x == c)
                }
                other => panic!("{other:?}"),
            }
        })
        .collect();
    assert_eq!(chosen, [None, None, None, Some(start as usize + 2)]);
}

#[test]
fn a_non_whole_sensitivity_above_100_percent_carries_its_fraction_too() {
    // 250 % is two and a half units a click: 2, then 3, so two clicks make five.
    let sequence = counts(count_row(0.0, 10.0, 2.5), 3.0, &[UP, UP, UP, UP]);
    assert_eq!(sequence, [Some(5.0), Some(6.0), Some(5.0), Some(6.0)]);
}

#[test]
fn an_int_moves_four_units_a_click_at_400_percent_and_clamps_at_both_ends() {
    assert_eq!(counts(count_row(0.0, 10.0, 4.0), 5.0, &[UP]), [Some(9.0)]);
    assert_eq!(counts(count_row(0.0, 10.0, 4.0), 8.0, &[UP]), [Some(10.0)]);
    assert_eq!(counts(count_row(0.0, 10.0, 4.0), 2.0, &[DOWN]), [Some(0.0)]);
    // At the end the message still goes out, as a click against the end always did.
    assert_eq!(counts(count_row(0.0, 10.0, 4.0), 10.0, &[UP]), [Some(10.0)]);
}

#[test]
fn an_int_follows_a_narrowed_and_an_inverted_range_at_sensitivity() {
    // Narrowed to 2..7: still four units a click, held inside the range.
    assert_eq!(counts(count_row(2.0, 7.0, 4.0), 5.0, &[UP]), [Some(7.0)]);
    assert_eq!(counts(count_row(2.0, 7.0, 4.0), 5.0, &[DOWN]), [Some(2.0)]);
    // Inverted (10 down to 0): a click up moves the value down, and the carry follows the turn.
    assert_eq!(counts(count_row(10.0, 0.0, 4.0), 5.0, &[UP]), [Some(1.0)]);
    assert_eq!(
        counts(count_row(10.0, 0.0, 0.5), 5.0, &[UP, UP, DOWN, DOWN]),
        [None, Some(4.0), None, Some(6.0)]
    );
}

#[test]
fn a_choice_moves_sensitivity_units_a_click() {
    let mut project = sensitivity_project();
    // Quantisation is a Choice target; start from its default and read the engine's reply.
    let control = ControlRef::Transport(TransportControl::Quantisation);
    let descriptor = describe_value(&project, &control).unwrap();
    let start = descriptor.value.unwrap();
    project.mappings = vec![relative_row(control, 0.0, 13.0, TWOS, 3.0)];
    let commands = resolve(
        &mut MappingResolver::default(),
        &project,
        &[midi(&[0xb0, 22, UP])],
    );
    let [Command::SetGlobalQuantisation(chosen)] = commands.as_slice() else {
        panic!("{commands:?}");
    };
    assert_eq!(
        GlobalQuantisation::CHOICES.iter().position(|c| c == chosen),
        Some((start + 3.0) as usize)
    );
}

#[test]
fn an_event_arguments_float_and_int_follow_the_same_rule() {
    let event_float = |key: &str, sensitivity: f64, wire: u8| -> f64 {
        let mut project = sensitivity_project();
        let mut row = argument_binding(1, source(22), &project, key);
        row.mode = SourceMode::Relative(TWOS);
        row.sensitivity = sensitivity;
        project.mappings = vec![row];
        let commands = resolve(
            &mut MappingResolver::default(),
            &project,
            &[midi(&[0xb0, 22, wire])],
        );
        let (_, _, _, OscArgument::Float(value)) = field_edit(&commands[0]) else {
            panic!()
        };
        f64::from(value)
    };
    // Unranged Float learns 0 to 1 and starts at 0: a click is sensitivity / 256 of the span.
    assert!(close(
        event_float("free", 1.0, UP),
        f64::from((1.0f64 / 256.0) as f32)
    ));
    assert!(close(
        event_float("free", 2.0, UP),
        f64::from((2.0f64 / 256.0) as f32)
    ));
    assert!(close(
        event_float("free", 0.5, UP),
        f64::from((0.5f64 / 256.0) as f32)
    ));
    // A ranged Float's span is its own: 2..4, starting at 2.
    assert!(close(
        event_float("ranged", 4.0, UP),
        f64::from((2.0f64 + 4.0 * 2.0 / 256.0) as f32)
    ));
    // An Int carries a remainder like any other.
    let sequence = counts(
        {
            let mut row = argument_binding(1, source(22), &sensitivity_project(), "count");
            row.mode = SourceMode::Relative(TWOS);
            row.sensitivity = 0.5;
            row
        },
        5.0,
        &[UP, UP, UP, UP],
    );
    assert_eq!(sequence, [None, Some(6.0), None, Some(6.0)]);
    // A face edit goes out and nothing else: no OSC reaches the receiver.
}

#[test]
fn scene_select_moves_sensitivity_scenes_a_click_with_a_carried_remainder() {
    let mut project = sensitivity_project();
    let moves = |sensitivity: f64, wires: &[u8]| -> Vec<Option<i32>> {
        let mut project = project.clone();
        project.mappings = vec![relative_row(
            ControlRef::SceneNavigation(SceneNavigation::Select),
            0.0,
            1.0,
            TWOS,
            sensitivity,
        )];
        let mut resolver = MappingResolver::default();
        wires
            .iter()
            .map(|&wire| {
                let mut moved = view_actions(&mut resolver, &project, &[midi(&[0xb0, 22, wire])]);
                assert!(moved.len() <= 1);
                moved.pop().map(|action| match action {
                    crate::ViewAction::MoveScene(steps) => steps,
                    other => panic!("{other:?}"),
                })
            })
            .collect()
    };
    assert_eq!(moves(1.0, &[UP, DOWN]), [Some(1), Some(-1)]);
    assert_eq!(moves(4.0, &[UP, DOWN]), [Some(4), Some(-4)]);
    assert_eq!(moves(2.5, &[UP, UP, DOWN]), [Some(2), Some(3), Some(-2)]);
    assert_eq!(
        moves(0.25, &[UP, UP, UP, UP, DOWN, DOWN, DOWN, DOWN]),
        [None, None, None, Some(1), None, None, None, Some(-1)]
    );
    // Min and Max play no part in Scene Select, and neither does a reversed pair.
    project.mappings.clear();
}

/// An encoder on Scene Select, as a host drives it: each drain starts with the scene the host
/// has selected, and the host then applies the moves, clamped to the scenes. Returns the move each
/// drain asked for (`None` for no move) and the row selected at the end.
fn spin_scenes(
    sensitivity: f64,
    scenes: usize,
    start: usize,
    drains: &[&[u8]],
) -> (Vec<Option<i32>>, usize) {
    use crate::{Consumer, ViewAction};
    let mut project = sensitivity_project();
    project.scenes = (0..scenes)
        .map(|index| Scene {
            id: SceneId(10 + index as u64),
            name: String::new(),
            follow_actions: Default::default(),
        })
        .collect();
    for track in &mut project.tracks {
        track.slots.resize_with(scenes, Default::default);
    }
    project.mappings = vec![relative_row(
        ControlRef::SceneNavigation(SceneNavigation::Select),
        0.0,
        1.0,
        TWOS,
        sensitivity,
    )];
    let mut resolver = MappingResolver::default();
    let mut selected = start;
    let moves = drains
        .iter()
        .map(|wires| {
            resolver.selected_scene(Some(selected));
            let events: Vec<_> = wires.iter().map(|&w| midi(&[0xb0, 22, w])).collect();
            let actions = view_actions(&mut resolver, &project, &events);
            let mut total = None;
            for action in actions {
                let ViewAction::MoveScene(steps) = action else {
                    panic!("{action:?}");
                };
                total = Some(total.unwrap_or(0) + steps);
                selected = action.scene_after(Some(selected), scenes).unwrap();
            }
            total
        })
        .collect();
    (moves, selected)
}

#[test]
fn scene_select_past_the_last_scene_owes_nothing_so_the_reversal_moves_on_the_fourth_click() {
    // On the last of five scenes at 25 %: three clicks on would owe 0.75 and can never be paid,
    // so the reversal needs the usual four clicks, not seven.
    let wires: Vec<[u8; 1]> = [UP, UP, UP, DOWN, DOWN, DOWN, DOWN]
        .into_iter()
        .map(|wire| [wire])
        .collect();
    let drains: Vec<&[u8]> = wires.iter().map(|wire| wire.as_slice()).collect();
    assert_eq!(
        spin_scenes(0.25, 5, 4, &drains),
        (vec![None, None, None, None, None, None, Some(-1)], 3)
    );
}

#[test]
fn scene_select_past_the_first_scene_owes_nothing_either() {
    let wires: Vec<[u8; 1]> = [DOWN, DOWN, DOWN, UP, UP, UP, UP]
        .into_iter()
        .map(|wire| [wire])
        .collect();
    let drains: Vec<&[u8]> = wires.iter().map(|wire| wire.as_slice()).collect();
    assert_eq!(
        spin_scenes(0.25, 5, 0, &drains),
        (vec![None, None, None, None, None, None, Some(1)], 1)
    );
}

#[test]
fn scene_select_drops_what_a_cut_short_move_owed() {
    // At 175 % a click on from the last scene owes 0.75 after its unit, which the end absorbs;
    // only without that leftover does the next click back owe -1.75 and take two scenes.
    assert_eq!(
        spin_scenes(1.75, 5, 4, &[&[UP], &[DOWN], &[DOWN]]),
        (vec![Some(1), Some(-1), Some(-2)], 1)
    );
}

#[test]
fn scene_select_keeps_its_remainder_away_from_the_ends() {
    // Mid-list nothing clamps: three clicks on owe 0.75, three back pay it, and the fourth owes a
    // quarter again. A spin that reaches the last scene within one unit is still mid-list.
    let wires: Vec<[u8; 1]> = [UP, UP, UP, DOWN, DOWN, DOWN, DOWN, DOWN, DOWN, DOWN]
        .into_iter()
        .map(|wire| [wire])
        .collect();
    let drains: Vec<&[u8]> = wires.iter().map(|wire| wire.as_slice()).collect();
    let (moves, selected) = spin_scenes(0.25, 5, 2, &drains);
    assert_eq!(moves.iter().flatten().collect::<Vec<_>>(), [&-1]);
    assert_eq!(selected, 1);
}

#[test]
fn scene_select_follows_its_own_moves_within_one_drain() {
    // The host applies a drain's moves afterwards, so the resolver tracks the selection itself:
    // eight clicks at 25 % from the second-to-last scene move on one scene, then reach the end,
    // and the three clicks after that owe nothing, so four clicks back move.
    let (moves, selected) = spin_scenes(
        0.25,
        5,
        3,
        &[
            &[UP, UP, UP, UP, UP, UP, UP, UP, UP, UP, UP],
            &[DOWN, DOWN, DOWN, DOWN],
        ],
    );
    // 11 clicks: one scene after four, a unit that the end cuts short after eight, and a
    // remainder owed to the end after that, all dropped. The four back then move one scene.
    assert_eq!(moves, [Some(1), Some(-1)]);
    assert_eq!(selected, 3);
}

#[test]
fn switches_and_triggers_ignore_sensitivity() {
    let run = |sensitivity: f64, control: &ControlRef| -> String {
        let mut project = sensitivity_project();
        project.mappings = vec![relative_row(control.clone(), 0.0, 1.0, TWOS, sensitivity)];
        let mut resolver = MappingResolver::default();
        [UP, DOWN, UP, 5, 100]
            .iter()
            .map(|&wire| {
                format!(
                    "{:?}",
                    resolve(&mut resolver, &project, &[midi(&[0xb0, 22, wire])])
                )
            })
            .collect::<Vec<_>>()
            .join(" | ")
    };
    for control in [
        ControlRef::TrackSolo(TrackId(1)),
        ControlRef::TrackActivator(TrackId(1)),
        ControlRef::Transport(TransportControl::Play),
        ControlRef::Transport(TransportControl::FollowActions),
        ControlRef::Event {
            track: TrackId(1),
            event: ping(),
        },
        argument_control("flag"),
    ] {
        let standard = run(1.0, &control);
        assert_ne!(
            standard,
            ["[]"; 5].join(" | "),
            "{control:?} sends something"
        );
        for sensitivity in [0.01, 0.25, 4.0, 10.0] {
            assert_eq!(
                run(sensitivity, &control),
                standard,
                "{control:?} at {sensitivity}"
            );
        }
    }
}

#[test]
fn a_stored_sensitivity_does_nothing_while_the_mode_is_absolute() {
    let run = |sensitivity: f64| -> String {
        let mut project = sensitivity_project();
        let mut row = volume_row(0.0, 1.0, TWOS, sensitivity);
        row.mode = SourceMode::Absolute;
        project.mappings = vec![row];
        let mut resolver = MappingResolver::default();
        [0u8, 64, 127, 10]
            .iter()
            .map(|&wire| {
                format!(
                    "{:?}",
                    resolve(&mut resolver, &project, &[midi(&[0xb0, 22, wire])])
                )
            })
            .collect::<Vec<_>>()
            .join(" | ")
    };
    let standard = run(1.0);
    assert!(standard.contains("SetParameters"), "{standard}");
    assert_eq!(run(7.5), standard);
}

#[test]
fn each_mapping_carries_its_own_remainder() {
    let mut project = sensitivity_project();
    let scene = relative_row(
        ControlRef::SceneNavigation(SceneNavigation::Select),
        0.0,
        1.0,
        TWOS,
        0.5,
    );
    let mut count = count_row(0.0, 10.0, 0.25);
    count.id = MappingId(2);
    project.mappings = vec![scene, count];
    let mut resolver = MappingResolver::default();
    let snapshot = Engine::new(project.clone()).unwrap().snapshot();
    let mut report = Vec::new();
    for _ in 0..4 {
        let mut actions = Vec::new();
        resolver.receive_actions(&[midi(&[0xb0, 22, UP])], &project, &snapshot, &mut actions);
        report.push(actions.len());
    }
    // Scene Select (50 %) moves on every second click, the count (25 %) on the fourth: 0, 1, 0, 2.
    assert_eq!(report, [0, 1, 0, 2]);
}

#[test]
fn the_remainder_clears_when_the_definition_changes_and_on_project_replacement() {
    let mut project = sensitivity_project();
    project.mappings = vec![count_row(0.0, 10.0, 0.25)];
    let click = |resolver: &mut MappingResolver, project: &Project| {
        resolve(resolver, project, &[midi(&[0xb0, 22, UP])])
    };
    // Control: three clicks owe 0.75, so the fourth moves the count.
    let mut resolver = MappingResolver::default();
    for _ in 0..3 {
        assert!(click(&mut resolver, &project).is_empty());
    }
    assert!(!click(&mut resolver, &project).is_empty());

    // Another mapping appearing or reordering leaves this one's remainder alone.
    let mut resolver = MappingResolver::default();
    for _ in 0..3 {
        click(&mut resolver, &project);
    }
    let mut busy = project.clone();
    let mut other = volume_row(0.0, 1.0, TWOS, 1.0);
    other.id = MappingId(9);
    other.source = source(30);
    busy.mappings.insert(0, other);
    assert!(
        !click(&mut resolver, &busy).is_empty(),
        "unrelated edit kept the remainder"
    );

    // A change to the mapping's own definition (here the sensitivity) starts it again.
    let mut resolver = MappingResolver::default();
    for _ in 0..3 {
        click(&mut resolver, &project);
    }
    let mut edited = project.clone();
    edited.mappings[0].sensitivity = 0.5;
    assert!(
        click(&mut resolver, &edited).is_empty(),
        "owed 0.75, +0.5 would move at once"
    );
    assert!(!click(&mut resolver, &edited).is_empty());

    // So does a change of range, mode or target.
    for change in [
        (|row: &mut RemoteMapping| row.max = 9.0) as fn(&mut RemoteMapping),
        |row| row.mode = SourceMode::Relative(RelativeEncoding::SignedBit2Linear),
    ] {
        let mut resolver = MappingResolver::default();
        for _ in 0..3 {
            click(&mut resolver, &project);
        }
        let mut edited = project.clone();
        change(&mut edited.mappings[0]);
        // The mode change decodes the same wire value differently; either way nothing is owed
        // from before, so the first click after an edit cannot move a 25 % Int.
        let commands = resolve(&mut resolver, &edited, &[midi(&[0xb0, 22, UP])]);
        let moved = !commands.is_empty();
        let fresh = !resolve(
            &mut MappingResolver::default(),
            &edited,
            &[midi(&[0xb0, 22, UP])],
        )
        .is_empty();
        assert_eq!(moved, fresh, "an edited row starts from nothing owed");
    }

    // Project replacement: the host resets the resolver, even for a set with the same ids.
    let mut resolver = MappingResolver::default();
    for _ in 0..3 {
        click(&mut resolver, &project);
    }
    resolver.reset();
    assert!(
        click(&mut resolver, &project).is_empty(),
        "a reset forgets the owed 0.75"
    );
}

// Value Scaling takeover (design 4.3). A rig applies each drain's commands to a real engine, as
// the host does, so the next drain starts from the accepted state.

struct Rig {
    engine: Engine,
    resolver: MappingResolver,
}

impl Rig {
    fn new(project: Project) -> Self {
        Self {
            engine: Engine::new(project).unwrap(),
            resolver: MappingResolver::default(),
        }
    }

    fn get(&self, control: &ControlRef) -> f64 {
        describe_value(self.engine.project(), control)
            .unwrap()
            .value
            .unwrap()
    }

    fn volume(&self, track: u64) -> f64 {
        self.get(&ControlRef::Argument(volume(track)))
    }

    /// One drain of `bytes` from `port`, its commands applied. Returns how many went out.
    fn send(&mut self, port: &str, bytes: &[u8]) -> usize {
        self.drain(&[midi_on(port, bytes)], true)
    }

    fn drain(&mut self, events: &[Envelope], apply: bool) -> usize {
        let mut commands = Vec::new();
        self.resolver.receive(
            events,
            self.engine.project(),
            &self.engine.snapshot(),
            &mut commands,
        );
        let count = commands.len();
        if apply {
            for command in commands {
                self.engine.execute(command.value).unwrap();
            }
        }
        count
    }

    /// CC 22 from the first port at `wire`: the volume of track 1 afterwards, `None` when the
    /// message sent nothing.
    fn cc(&mut self, wire: u8) -> Option<f64> {
        (self.send("first", &[0xb0, 22, wire]) > 0).then(|| self.volume(1))
    }

    fn set_volume(&mut self, track: u64, value: f64) {
        let field = volume(track);
        self.engine
            .execute(Command::Edit(ProjectEdit::SetParameters(vec![(
                field.parameter,
                vec![(field.field, value)],
            )])))
            .unwrap();
    }
}

fn scaling(id: u64, source: Source, control: ControlRef, min: f64, max: f64) -> RemoteMapping {
    let mut mapping = RemoteMapping::new(MappingId(id), source, control, min, max);
    mapping.takeover = Takeover::ValueScaling;
    mapping
}

/// Track 1's volume starts at `start`, mapped to CC 22 over `min` to `max`.
fn scaling_rig(start: f64, min: f64, max: f64) -> Rig {
    let mut project = project();
    project.tracks[0].mixer_device.volume.value = start;
    project.mappings = vec![scaling(
        1,
        source(22),
        ControlRef::Argument(volume(1)),
        min,
        max,
    )];
    Rig::new(project)
}

fn near(left: f64, right: f64, within: f64) -> bool {
    (left - right).abs() <= within
}

#[track_caller]
fn assert_near(left: Option<f64>, right: f64, within: f64) {
    assert!(
        left.is_some_and(|left| near(left, right, within)),
        "{left:?} is not within {within} of {right}"
    );
}

/// The wire value nearest a hardware position.
fn wire(position: f64) -> u8 {
    (position * 127.0).round() as u8
}

/// The rule of the design's 4.3 in hardware travel, restated for the tests.
fn proportional(position: f64, from: f64, to: f64) -> f64 {
    if to > from {
        position + (to - from) * (1.0 - position) / (1.0 - from)
    } else {
        position - (from - to) * position / from
    }
}

fn hardware(wire: u8) -> f64 {
    f64::from(wire) / 127.0
}

// Hardware 0.2 with the value at 0.8: up to 0.6 gives 0.9, up to 1.0 gives 1.0; or down to 0.1
// gives 0.4, down to 0 gives 0. A CC sends multiples of 1/127, so the literals hold to about
// 0.02 and the exact values follow the rule at the wire values actually sent.
#[test]
fn value_scaling_worked_values_with_the_hardware_below_the_value() {
    let mut up = scaling_rig(0.8, 0.0, 1.0);
    assert_eq!(up.cc(wire(0.2)), None, "the first message only records");
    let moved = up.cc(wire(0.6)).unwrap();
    assert_near(
        Some(moved),
        proportional(0.8, hardware(wire(0.2)), hardware(wire(0.6))),
        1e-9,
    );
    assert_near(Some(moved), 0.9, 0.02);
    assert_eq!(up.cc(127), Some(1.0), "both reach the top together");
    let mut down = scaling_rig(0.8, 0.0, 1.0);
    down.cc(wire(0.2));
    let moved = down.cc(wire(0.1)).unwrap();
    assert_near(
        Some(moved),
        proportional(0.8, hardware(wire(0.2)), hardware(wire(0.1))),
        1e-9,
    );
    assert_near(Some(moved), 0.4, 0.02);
    assert_eq!(down.cc(0), Some(0.0), "both reach the bottom together");
}

// Hardware 0.8 with the value at 0.2: up to 0.9 gives 0.6; down to 0.4 gives 0.1.
#[test]
fn value_scaling_worked_values_with_the_hardware_above_the_value() {
    let mut up = scaling_rig(0.2, 0.0, 1.0);
    assert_eq!(up.cc(wire(0.8)), None);
    let moved = up.cc(wire(0.9)).unwrap();
    assert_near(
        Some(moved),
        proportional(0.2, hardware(wire(0.8)), hardware(wire(0.9))),
        1e-9,
    );
    assert_near(Some(moved), 0.6, 0.02);
    assert_eq!(up.cc(127), Some(1.0));
    let mut down = scaling_rig(0.2, 0.0, 1.0);
    down.cc(wire(0.8));
    let moved = down.cc(wire(0.4)).unwrap();
    assert_near(
        Some(moved),
        proportional(0.2, hardware(wire(0.8)), hardware(wire(0.4))),
        1e-9,
    );
    assert_near(Some(moved), 0.1, 0.02);
    assert_eq!(down.cc(0), Some(0.0));
}

#[test]
fn the_first_message_from_a_port_only_records_unless_the_value_already_matches() {
    let mut rig = scaling_rig(0.8, 0.0, 1.0);
    assert_eq!(rig.cc(5), None);
    assert_eq!(rig.volume(1), 0.8, "no jump on the first touch");
    // A control already where the hardware is catches at once, as None would.
    let mut rig = scaling_rig(64.0 / 127.0, 0.0, 1.0);
    assert_eq!(rig.cc(64), Some(64.0 / 127.0));
    assert_eq!(rig.cc(100), Some(100.0 / 127.0), "and then tracks 1:1");
}

#[test]
fn value_scaling_catches_mid_travel_then_tracks_one_to_one() {
    let mut rig = scaling_rig(0.5, 0.0, 1.0);
    assert_eq!(rig.cc(60), None);
    let before = rig.cc(61).unwrap();
    assert!(before > 0.5 && before < 61.0 / 127.0 + 0.5, "{before}");
    // A long step closes the last of the gap. The value was ahead of the hardware, so the
    // catching message keeps it there (within half a step) rather than step back.
    assert_near(rig.cc(120), 120.0 / 127.0, 0.5 / 127.0);
    assert!(rig.volume(1) >= 120.0 / 127.0);
    for wire in [50, 51, 0, 127, 33] {
        assert_eq!(rig.cc(wire), Some(f64::from(wire) / 127.0), "{wire}");
    }
}

#[test]
fn value_scaling_always_moves_the_hardwares_way_and_never_passes_it() {
    // Every starting pair on a coarse grid, then a climb to the top and a fall to the bottom
    // through every wire value. The value never goes against the hardware's direction, stays on
    // its own side of the hardware until the two meet, and ends on the end the hardware reached.
    for from in (0..=127u8).step_by(9) {
        for value in (0..=20).map(|n| f64::from(n) / 20.0) {
            for to in [127u8, 0].into_iter().filter(|to| *to != from) {
                let mut rig = scaling_rig(value, 0.0, 1.0);
                rig.cc(from);
                let side = (value - hardware(from)).signum();
                let wires: Vec<u8> = if to > from {
                    (from + 1..=to).collect()
                } else {
                    (to..from).rev().collect()
                };
                let (mut last, mut caught) = (value, value == hardware(from));
                for wire in wires {
                    let now = rig.cc(wire).unwrap_or(last);
                    let name = format!("{from}->{wire} from {value}");
                    if to > from {
                        assert!(now >= last - 1e-12, "against the hardware, up {name}");
                    } else {
                        assert!(now <= last + 1e-12, "against the hardware, down {name}");
                    }
                    caught |= near(now, hardware(wire), 1e-12);
                    if !caught {
                        assert!((now - hardware(wire)) * side >= -1e-9, "passed it {name}");
                    }
                    last = now;
                }
                assert_eq!(last, hardware(to), "{from}->{to} from {value}");
            }
        }
    }
}

#[test]
fn a_range_of_0_2_to_0_8_and_an_inverted_range_apply_min_max_and_invert_as_none_does() {
    // The value 0.5 sits mid-travel of 0.2 to 0.8.
    let mut rig = scaling_rig(0.5, 0.2, 0.8);
    rig.cc(0);
    // Position 0.5 of the travel, hardware 0 to 0.5: it moves to 0.75, which is 0.2 + 0.75 * 0.6.
    assert_near(rig.cc(wire(0.5)), 0.65, 0.01);
    assert_eq!(rig.cc(127), Some(0.8));
    assert_eq!(rig.cc(0), Some(0.2), "tracks 1:1 once caught, through Min");
    // Inverted: Min 0.8, Max 0.2. The value 0.65 sits a quarter of the way along the travel.
    let mut rig = scaling_rig(0.65, 0.8, 0.2);
    rig.cc(0);
    let moved = rig.cc(wire(0.5)).unwrap();
    assert!(
        moved < 0.65,
        "up the hardware runs down an inverted range: {moved}"
    );
    assert_eq!(
        rig.cc(127),
        Some(0.2),
        "the top of the travel is Max, the low value"
    );
    assert_eq!(rig.cc(0), Some(0.8));
}

#[test]
fn a_value_outside_min_to_max_counts_as_the_nearer_end() {
    let mut rig = scaling_rig(0.9, 0.2, 0.8);
    rig.cc(10);
    let moved = rig.cc(wire(0.5)).unwrap();
    assert!((0.2..=0.8).contains(&moved) && moved > 0.5, "{moved}");
    assert_eq!(rig.cc(127), Some(0.8));
}

#[test]
fn equal_ends_hold_one_value_so_the_mode_acts_as_none() {
    let mut rig = scaling_rig(0.4, 0.4, 0.4);
    assert_eq!(
        rig.cc(0),
        Some(0.4),
        "no first-touch hold: None would send it"
    );
    assert_eq!(rig.cc(127), Some(0.4));
}

#[test]
fn a_gui_drag_an_undo_and_a_rejected_command_reset_the_scaling_from_the_engine() {
    // A GUI drag part-way: the position follows the engine's new value, not the old working one.
    let mut rig = scaling_rig(0.2, 0.0, 1.0);
    rig.cc(wire(0.8));
    let mid = rig.cc(wire(0.9)).unwrap();
    assert_near(
        Some(mid),
        proportional(0.2, hardware(wire(0.8)), hardware(wire(0.9))),
        1e-9,
    );
    rig.set_volume(1, 0.3);
    let next = rig.cc(wire(0.95)).unwrap();
    let expected = proportional(0.3, hardware(wire(0.9)), hardware(wire(0.95)));
    assert_near(Some(next), expected, 1e-9);
    // A caught control released by an external change must converge again.
    let mut rig = scaling_rig(0.5, 0.0, 1.0);
    rig.cc(60);
    assert_near(rig.cc(120), 120.0 / 127.0, 0.5 / 127.0);
    rig.set_volume(1, 0.1);
    let after = rig.cc(125).unwrap();
    assert_near(
        Some(after),
        proportional(0.1, hardware(120), hardware(125)),
        1e-9,
    );
    assert!(
        after > 0.1 && after < hardware(125),
        "converging again: {after}"
    );
    // Undo: the engine value no longer equals the last one sent.
    let mut rig = scaling_rig(0.2, 0.0, 1.0);
    rig.cc(wire(0.8));
    rig.cc(wire(0.9));
    rig.engine.execute(Command::Undo).unwrap();
    assert_eq!(rig.volume(1), 0.2);
    let after = rig.cc(wire(0.95)).unwrap();
    assert!(after > 0.2 && after < 0.95, "{after}");
    // A rejected command: nothing applied, so the engine still holds the old value.
    let mut rig = scaling_rig(0.2, 0.0, 1.0);
    rig.cc(wire(0.8));
    assert_eq!(rig.drain(&[midi(&[0xb0, 22, wire(0.9)])], false), 1);
    assert_eq!(rig.volume(1), 0.2);
    // The rejected move left no working value behind: the next one starts from the engine's 0.2.
    let again = proportional(0.2, hardware(wire(0.9)), hardware(wire(0.95)));
    assert_near(rig.cc(wire(0.95)), again, 1e-9);
}

#[test]
fn project_replacement_forgets_the_scaling_and_the_hardware_history() {
    let mut rig = scaling_rig(0.2, 0.0, 1.0);
    rig.cc(wire(0.8));
    rig.cc(wire(0.9));
    rig.resolver.reset();
    assert_eq!(
        rig.cc(wire(0.95)),
        None,
        "the first message again only records"
    );
}

#[test]
fn an_int_target_with_a_span_under_127_keeps_converging_between_whole_units() {
    // The Int "count" runs 0 to 10; one CC step moves its position by well under one unit.
    let mut project = sensitivity_project();
    project.mappings = vec![scaling(1, source(22), argument_control("count"), 0.0, 10.0)];
    let mut rig = Rig::new(project);
    let count = |rig: &Rig| rig.get(&argument_control("count"));
    assert_eq!(count(&rig), 5.0);
    rig.send("first", &[0xb0, 22, 40]);
    // While the hardware is still below the value, the position climbs by less than a unit a
    // message, and the target must follow its unrounded position, not the rounded value.
    let mut position = 0.5;
    let mut seen = vec![count(&rig)];
    for wire in 41..=60u8 {
        position = proportional(position, hardware(wire - 1), hardware(wire));
        rig.send("first", &[0xb0, 22, wire]);
        assert_eq!(count(&rig), (position * 10.0).round(), "wire {wire}");
        seen.push(count(&rig));
    }
    assert!(seen.contains(&6.0), "{seen:?}");
    for wire in 61..=127u8 {
        rig.send("first", &[0xb0, 22, wire]);
        seen.push(count(&rig));
    }
    assert!(seen.windows(2).all(|pair| pair[1] >= pair[0]), "{seen:?}");
    assert!(
        seen.contains(&6.0),
        "the unrounded position keeps it moving: {seen:?}"
    );
    assert_eq!(
        *seen.last().unwrap(),
        10.0,
        "and it reaches the top with the hardware"
    );
    let mut down = vec![count(&rig)];
    for wire in (0..127u8).rev() {
        rig.send("first", &[0xb0, 22, wire]);
        down.push(count(&rig));
    }
    assert_eq!(*down.last().unwrap(), 0.0);
}

#[test]
fn fanout_to_two_controls_at_different_values_converges_separately() {
    let mut project = project();
    project.tracks[0].mixer_device.volume.value = 0.2;
    project.tracks[1].mixer_device.volume.value = 0.8;
    project.mappings = [1, 2]
        .map(|id| scaling(id, source(22), ControlRef::Argument(volume(id)), 0.0, 1.0))
        .into();
    let mut rig = Rig::new(project);
    rig.cc(wire(0.5));
    rig.cc(wire(0.75));
    let (low, high) = (rig.volume(1), rig.volume(2));
    assert!(low > 0.2 && low < 0.75, "{low}");
    assert!(high > 0.8 && high < 1.0, "{high}");
    assert!(low < high);
    rig.cc(127);
    assert_eq!((rig.volume(1), rig.volume(2)), (1.0, 1.0));
    rig.cc(0);
    assert_eq!((rig.volume(1), rig.volume(2)), (0.0, 0.0));
}

#[test]
fn two_ports_sharing_a_cc_number_keep_their_own_history() {
    let mut rig = scaling_rig(0.2, 0.0, 1.0);
    assert_eq!(rig.send("a", &[0xb0, 22, 10]), 0);
    // Port b's first message cannot borrow port a's history: it only records.
    assert_eq!(rig.send("b", &[0xb0, 22, 127]), 0);
    assert_eq!(rig.volume(1), 0.2);
    // Port a goes on from where it was.
    assert_eq!(rig.send("a", &[0xb0, 22, 60]), 1);
    assert!(rig.volume(1) > 0.2);
}

#[test]
fn a_choice_and_a_switch_act_as_pick_up() {
    for (control, wires) in [
        (
            ControlRef::TrackSolo(TrackId(1)),
            [100u8, 80, 60, 20, 0, 64, 64, 0].as_slice(),
        ),
        (
            ControlRef::Transport(TransportControl::Quantisation),
            [127u8, 100, 90, 3, 0, 64, 127].as_slice(),
        ),
        (
            ControlRef::Transport(TransportControl::FollowActions),
            [20u8, 10, 0, 127].as_slice(),
        ),
    ] {
        let play = |takeover: Takeover| {
            let mut project = project();
            let (min, max) = learn_range(&control, &describe_value(&project, &control).unwrap());
            let mut mapping = scaling(1, source(22), control.clone(), min, max);
            mapping.takeover = takeover;
            project.mappings = vec![mapping];
            let mut rig = Rig::new(project);
            wires
                .iter()
                .map(|&wire| {
                    let sent = rig.send("first", &[0xb0, 22, wire]);
                    (sent, rig.get(&control))
                })
                .collect::<Vec<_>>()
        };
        let pick_up = play(Takeover::PickUp);
        assert!(
            pick_up.iter().any(|(sent, _)| *sent == 0)
                && pick_up.iter().any(|(sent, _)| *sent == 1),
            "the sequence must both block and catch: {pick_up:?}"
        );
        assert_eq!(play(Takeover::ValueScaling), pick_up, "{control:?}");
    }
}

#[test]
fn a_stored_takeover_on_an_activator_has_no_effect() {
    let play = |takeover: Takeover| {
        let mut project = project();
        let mut mapping = scaling(
            1,
            source(22),
            ControlRef::TrackActivator(TrackId(1)),
            ACTIVATOR_WINDOW[0],
            ACTIVATOR_WINDOW[1],
        );
        mapping.takeover = takeover;
        project.mappings = vec![mapping];
        let mut rig = Rig::new(project);
        [10u8, 100, 20, 127, 0, 90]
            .iter()
            .map(|&wire| {
                let sent = rig.send("first", &[0xb0, 22, wire]);
                (sent, rig.get(&ControlRef::TrackActivator(TrackId(1))))
            })
            .collect::<Vec<_>>()
    };
    let none = play(Takeover::None);
    assert!(none.iter().any(|(sent, _)| *sent == 1), "{none:?}");
    assert_eq!(play(Takeover::PickUp), none, "Pick-Up no longer acts on it");
    assert_eq!(play(Takeover::ValueScaling), none);
}

#[test]
fn value_scaling_on_relative_notes_and_keys_is_stored_and_inert() {
    let note = Source::Midi(MidiSource {
        channel: 1,
        message: MidiMessage::Note(60),
    });
    let relative = |takeover: Takeover| {
        let mut mapping = scaling(1, source(22), ControlRef::Argument(volume(1)), 0.0, 1.0);
        mapping.takeover = takeover;
        mapping.mode = SourceMode::Relative(RelativeEncoding::TwosComplement);
        mapping
    };
    let pad = |takeover: Takeover| {
        let mut mapping = scaling(1, note.clone(), ControlRef::Argument(volume(1)), 0.0, 1.0);
        mapping.takeover = takeover;
        mapping
    };
    for (messages, make) in [
        (
            [[0xb0u8, 22, 1], [0xb0, 22, 1], [0xb0, 22, 127]],
            &relative as &dyn Fn(Takeover) -> RemoteMapping,
        ),
        ([[0x90, 60, 100], [0x80, 60, 0], [0x90, 60, 90]], &pad),
    ] {
        let play = |takeover: Takeover| {
            let mut project = project();
            project.tracks[0].mixer_device.volume.value = 0.5;
            project.mappings = vec![make(takeover)];
            let mut rig = Rig::new(project);
            messages
                .iter()
                .map(|bytes| {
                    let sent = rig.send("first", bytes);
                    (sent, rig.volume(1))
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(play(Takeover::ValueScaling), play(Takeover::None));
    }
}

#[test]
fn pitch_bend_scales_in_14_bit_travel_and_catches_within_half_a_14_bit_step() {
    let bend = Source::Midi(MidiSource {
        channel: 1,
        message: MidiMessage::PitchBend,
    });
    let message = |value: u16| [0xe0, (value & 0x7f) as u8, (value >> 7) as u8];
    let travel = |value: u16| f64::from(value) / 16383.0;
    let rig_at = |start: f64| {
        let mut project = project();
        project.tracks[0].mixer_device.volume.value = start;
        project.mappings = vec![scaling(
            1,
            bend.clone(),
            ControlRef::Argument(volume(1)),
            0.0,
            1.0,
        )];
        Rig::new(project)
    };
    let mut rig = rig_at(0.8);
    assert_eq!(rig.send("first", &message(3277)), 0);
    rig.send("first", &message(9830));
    assert_near(Some(rig.volume(1)), 0.9, 0.001);
    rig.send("first", &message(16383));
    assert_eq!(rig.volume(1), 1.0);
    // A value a 14-bit step from the hardware is still not caught; within half a step it is.
    let mut rig = rig_at(travel(8000));
    rig.send("first", &message(8002));
    assert_eq!(
        rig.volume(1),
        travel(8000),
        "two steps apart: not caught yet"
    );
    let mut rig = rig_at(travel(8000));
    assert_eq!(
        rig.send("first", &message(8000)),
        1,
        "equal: caught at once"
    );
    assert_eq!(rig.volume(1), travel(8000));
}

// A Float event argument is stored as an f32. The resolver must record the number the engine
// will store, or Pick-Up, Value Scaling and Gate's dedup disagree with the engine on the next
// drain.

/// `project_with_arguments()` with the Float argument "free" holding 0.5, mapped to CC 22.
fn float_argument_rig(takeover: Takeover) -> Rig {
    let mut project = project_with_arguments();
    project.tracks[0].devices[0].target_mut().unwrap().events[0].fields[1].value = Some(0.5);
    let mut mapping = argument_binding(1, source(22), &project, "free");
    mapping.takeover = takeover;
    project.mappings = vec![mapping];
    Rig::new(project)
}

/// Track 1's volume at 0.5, mapped to CC 22, the f64 parameter the argument should behave like.
fn volume_rig(takeover: Takeover) -> Rig {
    let mut project = project();
    project.tracks[0].mixer_device.volume.value = 0.5;
    let mut mapping = binding(1, source(22), ControlRef::Argument(volume(1)));
    mapping.takeover = takeover;
    project.mappings = vec![mapping];
    Rig::new(project)
}

/// Feeds each wire value as its own drain to a Float argument and to Volume, which keeps an f64,
/// and checks that the argument sends exactly when Volume does and follows it to within an f32.
fn assert_float_argument_behaves_like_volume(takeover: Takeover, wires: &[u8]) -> Vec<usize> {
    let mut argument = float_argument_rig(takeover);
    let mut reference = volume_rig(takeover);
    let mut sent = Vec::new();
    for &wire in wires {
        let count = argument.send("first", &[0xb0, 22, wire]);
        let expected = reference.send("first", &[0xb0, 22, wire]);
        sent.push(count);
        assert_eq!(count, expected, "{takeover:?} at {wire}: commands sent");
        assert_near(
            Some(argument_value(argument.engine.project(), "free")),
            reference.volume(1),
            1e-6,
        );
    }
    sent
}

#[test]
fn pick_up_on_a_float_argument_keeps_following_after_it_catches() {
    // Down through 0.5 and up through it again, one message per drain.
    let down: Vec<u8> = (55..=66).rev().collect();
    let sent = assert_float_argument_behaves_like_volume(Takeover::PickUp, &down);
    assert!(
        sent[4..].iter().all(|count| *count == 1),
        "every message after the catch moves the value: {sent:?}"
    );
    let up: Vec<u8> = (61..=72).collect();
    let sent = assert_float_argument_behaves_like_volume(Takeover::PickUp, &up);
    assert!(
        sent[4..].iter().all(|count| *count == 1),
        "every message after the catch moves the value: {sent:?}"
    );
}

#[test]
fn value_scaling_on_a_float_argument_matches_the_f64_parameter_over_ten_drains() {
    let up: Vec<u8> = (25..35).collect();
    assert_float_argument_behaves_like_volume(Takeover::ValueScaling, &up);
    let down: Vec<u8> = (80..90).rev().collect();
    assert_float_argument_behaves_like_volume(Takeover::ValueScaling, &down);
}

#[test]
fn a_gate_on_a_float_argument_sends_nothing_for_a_repeated_note_on_or_off() {
    let mut project = project_with_arguments();
    // 0.3 and 0.7 have no exact f32, so the stored value differs from the f64 sent.
    let mut mapping = argument_binding(1, pad_source(), &project, "free");
    mapping.mode = SourceMode::Gate;
    mapping.min = 0.3;
    mapping.max = 0.7;
    project.mappings = vec![mapping];
    let mut rig = Rig::new(project);
    let counts: Vec<usize> = [PAD_ON, PAD_ON, PAD_OFF, PAD_OFF]
        .iter()
        .map(|message| rig.send("first", message))
        .collect();
    assert_eq!(counts, [1, 0, 1, 0]);
}

#[test]
fn a_stored_gate_on_a_trigger_acts_as_latch() {
    let messages: [&[u8]; 4] = [&PAD_ON, &PAD_OFF, &PAD_ON, &PAD_OFF];
    for name in ["Transport Play", "Bang event", "Clip slot"] {
        let (_, control, min, max) = pad_controls()
            .into_iter()
            .find(|(candidate, ..)| *candidate == name)
            .unwrap();
        let latch = pad_mapping(1, pad_source(), control, min, max);
        let gate = RemoteMapping {
            mode: SourceMode::Gate,
            ..latch.clone()
        };
        let mut project = pad_project();
        project.mappings = vec![gate.clone()];
        project
            .validate()
            .expect("the model takes a Gate on a trigger");
        assert_eq!(
            gate_run(gate, &messages),
            pad_summary(&gate_rows(latch, &messages)),
            "{name}"
        );
    }
}

/// A Float argument whose range ends have no exact f32 can still be driven to both ends: the
/// stored f32 must stay inside the range the engine checks.
#[test]
fn a_float_argument_with_inexact_range_ends_reaches_both_ends() {
    for (low, high) in [(0.0, 0.1), (0.3, 0.7), (0.1, 0.9)] {
        let mut project = project_with_arguments();
        let target = project.tracks[0].devices[0].target_mut().unwrap();
        let field = &mut target.events[0].fields[1];
        field.range = Some([low, high]);
        field.default = OscArgument::Float(((low + high) / 2.0) as f32);
        field.value = Some(f64::from(((low + high) / 2.0) as f32));
        let mut mapping = argument_binding(1, source(22), &project, "free");
        mapping.min = low;
        mapping.max = high;
        project.mappings = vec![mapping];
        let mut rig = Rig::new(project);
        for (wire, end) in [(127, high), (0, low), (127, high)] {
            rig.send("first", &[0xb0, 22, wire]);
            let held = argument_value(rig.engine.project(), "free");
            assert!(
                (held - end).abs() < 1e-6 && (low..=high).contains(&held),
                "{low}..{high}: CC {wire} left {held}, wanted {end}"
            );
        }
    }
}
