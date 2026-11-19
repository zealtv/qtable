use qtable_engine::edit::{ProjectEdit, TargetEdit};
use qtable_engine::engine::{Command, Engine};
use qtable_engine::model::*;
use qtable_test_support::TestReceiver;
use rosc::OscType;
use std::time::{Duration, Instant};

fn field(key: &str, kind: FieldType, control: Control, value: f64) -> FieldDef {
    let mut f = FieldDef::new(FieldKey(key.into()), kind, true);
    if matches!(kind, FieldType::Float | FieldType::Int) {
        f.range = Some([-100.0, 100.0]);
    }
    f.control = control;
    f.value = Some(value);
    f
}
fn project(receiver: &TestReceiver, fields: Vec<FieldDef>) -> (Project, ParameterRef) {
    let mut track = Track::new(TrackId(1), "Track", 0);
    track.route = Route::single(TrackId(2));
    track.devices.push(Device {
        id: DeviceId(1),
        name: "Target".into(),
        active: true,
        expanded: false,
        kind: DeviceKind::Target(TargetDevice {
            parameters: vec![ParameterDef {
                key: EntryKey("point".into()),
                name: "Point".into(),
                path: OscAddress::Relative("/point".into()),
                fields,
                send_mode: SendMode::Continuous,
            }],
            ..TargetDevice::default()
        }),
    });
    (
        Project {
            tracks: vec![track],
            outputs: vec![Track::new_output(
                TrackId(2),
                "Out",
                "127.0.0.1",
                receiver.addr().port(),
            )],
            ..Project::default()
        },
        ParameterRef {
            track: TrackId(1),
            device: DeviceRef::Chain(DeviceId(1)),
            key: ParameterKey("point".into()),
        },
    )
}
fn receive(receiver: &TestReceiver) -> rosc::OscMessage {
    receiver
        .wait_for_messages(1, Duration::from_secs(2))
        .unwrap()
        .remove(0)
        .1
}
fn silent(receiver: &TestReceiver) {
    assert!(
        receiver
            .wait_for_messages(1, Duration::from_millis(70))
            .is_err()
    );
}

#[test]
fn controls_convert_to_ordered_wire_values_and_unchanged_options_are_not_resent() {
    let receiver = TestReceiver::bind().unwrap();
    let mut fields = vec![
        field("float", FieldType::Float, Control::Slider, 0.25),
        field("int", FieldType::Int, Control::NumberBox, -2.0),
        field("bool", FieldType::Bool, Control::Toggle, 1.0),
        field("float-toggle", FieldType::Float, Control::Toggle, 1.0),
        field("int-toggle", FieldType::Int, Control::Toggle, 1.0),
        field("float-enum", FieldType::Float, Control::Enum, 1.0),
        field("int-enum", FieldType::Int, Control::Enum, 1.0),
        field("string-enum", FieldType::String, Control::Enum, 1.0),
    ];
    for (index, options) in [
        vec![OscArgument::Float(-4.0), OscArgument::Float(3.5)],
        vec![OscArgument::Int(-20), OscArgument::Int(42)],
        vec![
            OscArgument::String("off".into()),
            OscArgument::String("on".into()),
        ],
    ]
    .into_iter()
    .enumerate()
    {
        let f = &mut fields[index + 5];
        f.default = options[0].clone();
        f.options = options
            .into_iter()
            .map(|value| EnumOption {
                label: match &value {
                    OscArgument::String(s) => s.clone(),
                    _ => format!("{value:?}"),
                },
                value,
            })
            .collect();
    }
    let (p, target) = project(&receiver, fields);
    // Int rounding is defined at the conversion boundary too, after numeric control edits.
    let view = p.parameter(&target).unwrap();
    let mut int = p
        .field(&FieldRef {
            parameter: target.clone(),
            field: FieldKey("int".into()),
        })
        .unwrap();
    int.value = -1.6;
    assert_eq!(int.argument(), OscArgument::Int(-2));
    assert_eq!(view.fields.len(), 8);
    let mut engine = Engine::new(p).unwrap();
    engine.execute(Command::Play).unwrap();
    let expected = vec![
        OscType::Float(0.25),
        OscType::Int(-2),
        OscType::Bool(true),
        OscType::Float(1.0),
        OscType::Int(1),
        OscType::Float(3.5),
        OscType::Int(42),
        OscType::String("on".into()),
    ];
    let message = receive(&receiver);
    assert_eq!(message.addr, "/point");
    assert_eq!(message.args, expected);
    silent(&receiver);
    engine
        .execute(Command::Edit(ProjectEdit::SetParameter {
            target: target.clone(),
            values: vec![(FieldKey("bool".into()), 0.0)],
        }))
        .unwrap();
    let mut expected = expected;
    expected[2] = OscType::Bool(false);
    assert_eq!(receive(&receiver).args, expected);
    // Invalid option edits leave the wire value unchanged and send nothing.
    let error = engine
        .execute(Command::Edit(ProjectEdit::EditTarget {
            device: DeviceId(1),
            edit: TargetEdit::SetFieldOptions {
                entry: EntryKey("point".into()),
                field: FieldKey("int-enum".into()),
                options: vec![
                    EnumOption {
                        label: "Same A".into(),
                        value: OscArgument::Int(42),
                    },
                    EnumOption {
                        label: "Same B".into(),
                        value: OscArgument::Int(42),
                    },
                ],
            },
        }))
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Enum option values must be unique")
    );
    silent(&receiver);
    // An index is not a wire value: moving the selected option sends nothing.
    engine
        .execute(Command::Edit(ProjectEdit::EditTarget {
            device: DeviceId(1),
            edit: TargetEdit::SetFieldOptions {
                entry: EntryKey("point".into()),
                field: FieldKey("int-enum".into()),
                options: vec![
                    EnumOption {
                        label: "Selected".into(),
                        value: OscArgument::Int(42),
                    },
                    EnumOption {
                        label: "Other".into(),
                        value: OscArgument::Int(-20),
                    },
                ],
            },
        }))
        .unwrap();
    silent(&receiver);

    engine
        .execute(Command::Edit(ProjectEdit::SetParameter {
            target,
            values: vec![(FieldKey("int-enum".into()), 0.0)],
        }))
        .unwrap();
    silent(&receiver);
}

#[test]
fn fifty_field_edits_send_one_complete_point_from_the_worker() {
    let receiver = TestReceiver::bind().unwrap();
    let (mut p, target) = project(
        &receiver,
        vec![
            field("x", FieldType::Float, Control::Slider, 0.0),
            field("y", FieldType::Float, Control::Slider, 0.7),
            field("radius", FieldType::Float, Control::Slider, 12.0),
        ],
    );
    p.outputs[0].output.as_mut().unwrap().stage.frame_rate = 1.0;
    let mut engine = Engine::new(p).unwrap();
    // Enqueue-time edits may qualify for the worker's initial grid tick. Establish a
    // completed frame so this burst tests coalescing within one known frame interval.
    engine
        .execute(Command::Edit(ProjectEdit::SetParameter {
            target: target.clone(),
            values: vec![(FieldKey("x".into()), -1.0)],
        }))
        .unwrap();
    assert_eq!(
        receive(&receiver).args,
        vec![
            OscType::Float(-1.0),
            OscType::Float(0.7),
            OscType::Float(12.0)
        ]
    );
    let edited = Instant::now();
    for step in 1..=50 {
        engine
            .execute(Command::Edit(ProjectEdit::SetParameter {
                target: target.clone(),
                values: vec![(FieldKey("x".into()), f64::from(step) / 100.0)],
            }))
            .unwrap();
    }
    assert!(edited.elapsed() < Duration::from_millis(500));
    let message = receive(&receiver);
    assert_eq!(message.addr, "/point");
    assert_eq!(
        message.args,
        vec![
            OscType::Float(0.5),
            OscType::Float(0.7),
            OscType::Float(12.0)
        ]
    );
    silent(&receiver);
}

#[test]
fn clip_hits_keep_their_typed_values_after_event_fields_move_and_retype() {
    let receiver = TestReceiver::bind().unwrap();
    let (mut p, _) = project(
        &receiver,
        vec![field("x", FieldType::Float, Control::Slider, 0.0)],
    );
    let track = &mut p.tracks[0];
    track.devices.clear();
    track.slots.push(Slot::default());
    let mut clip = Clip::new(ClipId(1), "Hit", BeatTime(3840), LIVE_CLIP_PALETTE[0]).unwrap();
    clip.looping = false;
    let hit = track.hit(
        BeatTime::ZERO,
        OscAddress::Relative("/hit".into()),
        vec![
            OscArgument::Int(7),
            OscArgument::String("kept".into()),
            OscArgument::Bool(true),
        ],
        || DeviceId(1),
    );
    let key = hit.definition.key.clone();
    clip.events.push(hit);
    track.slots[0].clip = Some(clip);
    p.scenes.push(Scene {
        id: SceneId(1),
        name: "Scene".into(),
        follow_actions: Default::default(),
    });
    let mut engine = Engine::new(p).unwrap();
    for edit in [
        TargetEdit::MoveField {
            entry: key.clone(),
            field: FieldKey("f3".into()),
            to: 0,
        },
        TargetEdit::SetFieldType {
            entry: key.clone(),
            field: FieldKey("f1".into()),
            value_type: FieldType::Float,
        },
        TargetEdit::SetFieldDefault {
            entry: key,
            field: FieldKey("f1".into()),
            default: OscArgument::Float(99.0),
        },
    ] {
        engine
            .execute(Command::Edit(ProjectEdit::EditTarget {
                device: DeviceId(1),
                edit,
            }))
            .unwrap();
    }
    engine
        .execute(Command::LaunchClip {
            track: TrackId(1),
            slot: 0,
        })
        .unwrap();
    let message = receive(&receiver);
    assert_eq!(message.addr, "/hit");
    assert_eq!(
        message.args,
        vec![
            OscType::Bool(true),
            OscType::Float(7.0),
            OscType::String("kept".into())
        ]
    );
    silent(&receiver);
}
