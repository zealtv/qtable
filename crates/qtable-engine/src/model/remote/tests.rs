use super::*;

fn relative(path: &str) -> OscAddress {
    OscAddress::Relative(path.into())
}

/// Two scenes; track 1 holds a Target (device 5) with a Cutoff parameter, a Mode enum and a
/// Ping event; track 2 is a plain track; group 3 and output 9 test the kind limits.
pub(super) fn project() -> Project {
    let mut target = TargetDevice::default();
    target.parameters.push(ParameterDef {
        key: EntryKey("cutoff".into()),
        name: "Cutoff".into(),
        path: relative("/cutoff"),
        fields: vec![FieldDef {
            name: "Cutoff".into(),
            range: Some([20.0, 20_000.0]),
            value: Some(440.0),
            ..FieldDef::new(FieldKey("f1".into()), FieldType::Float, true)
        }],
        send_mode: SendMode::EveryFrame,
    });
    let mut mode = FieldDef::new(FieldKey("f1".into()), FieldType::Int, true);
    mode.control = Control::Enum;
    mode.options = ["Low", "High"]
        .iter()
        .enumerate()
        .map(|(n, label)| EnumOption {
            label: (*label).into(),
            value: OscArgument::Int(n as i32),
        })
        .collect();
    mode.default = OscArgument::Int(0);
    mode.value = Some(1.0);
    target.parameters.push(ParameterDef {
        key: EntryKey("mode".into()),
        name: "Mode".into(),
        path: relative("/mode"),
        fields: vec![mode],
        send_mode: SendMode::OnRelease,
    });
    target.events.push(EventDef {
        key: EntryKey("ping".into()),
        name: "Ping".into(),
        path: relative("/ping"),
        fields: Vec::new(),
    });
    let mut first = Track::new(TrackId(1), "Lead", 2);
    let mut device = NewDevice::Target.build(DeviceId(5));
    device.name = "Synth".into();
    device.kind = DeviceKind::Target(target);
    first.devices.push(device);
    Project {
        scenes: vec![
            Scene {
                id: SceneId(10),
                name: String::new(),
                follow_actions: SceneFollowActions::default(),
            },
            Scene {
                id: SceneId(11),
                name: "Drop".into(),
                follow_actions: SceneFollowActions::default(),
            },
        ],
        tracks: vec![
            first,
            Track::new(TrackId(2), "Pad", 2),
            Track::new_group(TrackId(3), "Group", 2),
        ],
        outputs: vec![Track::new_output(TrackId(9), "Out", "localhost", 9000)],
        ..Project::default()
    }
}

pub(super) fn field(parameter: &str, field: &str) -> ControlRef {
    ControlRef::Argument(FieldRef {
        parameter: ParameterRef {
            track: TrackId(1),
            device: DeviceRef::Chain(DeviceId(5)),
            key: ParameterKey(parameter.into()),
        },
        field: FieldKey(field.into()),
    })
}

pub(super) fn event() -> ControlRef {
    ControlRef::Event {
        track: TrackId(1),
        event: EventRef {
            device: DeviceId(5),
            key: EntryKey("ping".into()),
        },
    }
}

#[test]
fn an_argument_describes_its_domain_range_and_value() {
    let project = project();
    let cutoff = describe_control(&project, &field("cutoff", "f1")).unwrap();
    assert_eq!(cutoff.label, "Cutoff");
    assert_eq!(cutoff.path, "Lead / Synth");
    assert_eq!(cutoff.kind, ControlKind::Float);
    assert_eq!(cutoff.range, Some([20.0, 20_000.0]));
    assert_eq!(cutoff.value, Some(440.0));
    assert_eq!(cutoff.access, ControlAccess::ReadWrite);
    let mode = describe_control(&project, &field("mode", "f1")).unwrap();
    assert_eq!(mode.kind, ControlKind::Enum);
    assert_eq!(mode.choices, ["Low", "High"]);
    assert_eq!(mode.range, Some([0.0, 1.0]));
    assert_eq!(mode.value, Some(1.0));
}

/// A parameter with several arguments names each row after both ("Position x"), as an event's
/// argument reads; one with a single argument keeps its own name, and Mixer Volume its own.
#[test]
fn a_parameter_with_several_arguments_names_each_after_parameter_and_argument() {
    let mut project = project();
    let mut x = FieldDef::new(FieldKey("x".into()), FieldType::Float, true);
    x.name = "x".into();
    let y = FieldDef::new(FieldKey("y".into()), FieldType::Float, true);
    project.tracks[0].devices[0]
        .target_mut()
        .unwrap()
        .parameters
        .push(ParameterDef {
            key: EntryKey("position".into()),
            name: "Position".into(),
            path: relative("/position"),
            fields: vec![x, y],
            send_mode: SendMode::EveryFrame,
        });
    let label = |field: &str| {
        describe_control(&project, &self::field("position", field))
            .unwrap()
            .label
    };
    assert_eq!(label("x"), "Position x");
    // An unnamed argument reads by its position, as on the face.
    assert_eq!(label("y"), "Position Arg 2");
    let single = describe_control(&project, &field("cutoff", "f1")).unwrap();
    assert_eq!(single.label, "Cutoff");
}

#[test]
fn mixer_volume_describes_as_a_unit_float() {
    let project = project();
    let volume = ControlRef::Argument(FieldRef::volume(TrackId(2)));
    let described = describe_control(&project, &volume).unwrap();
    assert_eq!(described.label, "Volume");
    assert_eq!(described.path, "Pad / Mixer");
    assert_eq!(described.range, Some([0.0, 1.0]));
    assert_eq!(described.value, Some(1.0));
}

#[test]
fn launches_and_events_are_write_only() {
    let project = project();
    let bang = describe_control(&project, &event()).unwrap();
    assert_eq!(
        (bang.kind, bang.access, bang.value, bang.label.as_str()),
        (ControlKind::Bang, ControlAccess::WriteOnly, None, "Ping")
    );
    let slot = describe_control(
        &project,
        &ControlRef::ClipSlot {
            track: TrackId(2),
            scene: SceneId(11),
        },
    )
    .unwrap();
    assert_eq!(
        (slot.kind, slot.label.as_str()),
        (ControlKind::Trigger, "Slot 2")
    );
    let unnamed = describe_control(&project, &ControlRef::Scene(SceneId(10))).unwrap();
    assert_eq!(unnamed.label, "Scene 1");
    let named = describe_control(&project, &ControlRef::Scene(SceneId(11))).unwrap();
    assert_eq!(named.label, "Drop");
}

#[test]
fn switches_report_their_state_and_kind_limits() {
    let mut project = project();
    project.tracks[1].mixer.solo = true;
    project.tracks[1].mixer.active = false;
    let solo = describe_control(&project, &ControlRef::TrackSolo(TrackId(2))).unwrap();
    let activator = describe_control(&project, &ControlRef::TrackActivator(TrackId(2))).unwrap();
    assert_eq!((solo.kind, solo.value), (ControlKind::Toggle, Some(1.0)));
    assert_eq!(activator.value, Some(0.0));
    // A Group Stop names its descendant scope; Main has no Solo or Activator.
    assert!(describe_control(&project, &ControlRef::TrackStop(TrackId(3))).is_some());
    assert!(describe_control(&project, &ControlRef::TrackSolo(project.main.id)).is_none());
    assert!(describe_control(&project, &ControlRef::TrackStop(TrackId(9))).is_none());
    assert!(describe_control(&project, &ControlRef::TrackSolo(TrackId(9))).is_some());
}

#[test]
fn transport_controls_describe_their_current_values() {
    let mut project = project();
    project.tempo = Tempo::new(133.5).unwrap();
    project.global_quantisation =
        GlobalQuantisation::Value(QuantisationValue::Note(NoteValue::Eighth));
    project.follow_actions_enabled = false;
    let describe = |control| describe_control(&project, &ControlRef::Transport(control)).unwrap();
    let tempo = describe(TransportControl::Tempo);
    assert_eq!(
        (tempo.kind, tempo.range, tempo.value),
        (ControlKind::Float, None, Some(133.5))
    );
    let quant = describe(TransportControl::Quantisation);
    assert_eq!(quant.choices.len(), 14);
    assert_eq!(quant.choices[0], "None");
    assert_eq!(quant.choices[4], "1 Bar");
    assert_eq!(quant.choices[9], "1/8");
    assert_eq!(quant.value, Some(9.0));
    assert_eq!(describe(TransportControl::FollowActions).value, Some(0.0));
    for control in [
        TransportControl::Play,
        TransportControl::Stop,
        TransportControl::StopAll,
        TransportControl::ReEnableAutomation,
    ] {
        assert_eq!(describe(control).access, ControlAccess::WriteOnly);
    }
}

/// The project with a Hit event on track 1 (device 5) holding one argument of each kind.
fn project_with_hit_event() -> Project {
    let mut project = project();
    let float = |key: &str, control: Control, range: Option<[f64; 2]>, value: f64| FieldDef {
        control,
        range,
        value: Some(value),
        ..FieldDef::new(FieldKey(key.into()), FieldType::Float, false)
    };
    let mut pick = FieldDef::new(FieldKey("pick".into()), FieldType::String, false);
    pick.control = Control::Enum;
    pick.options = ["One", "Two", "Three"]
        .iter()
        .map(|label| EnumOption {
            label: (*label).into(),
            value: OscArgument::String((*label).into()),
        })
        .collect();
    pick.default = OscArgument::String("One".into());
    pick.seed_current();
    pick.value = Some(2.0);
    let mut text = FieldDef::new(FieldKey("words".into()), FieldType::String, false);
    text.control = Control::Text;
    text.seed_current();
    let mut flag = FieldDef::new(FieldKey("flag".into()), FieldType::Bool, false);
    flag.control = Control::Toggle;
    flag.seed_current();
    flag.value = Some(1.0);
    let mut count = FieldDef::new(FieldKey("count".into()), FieldType::Int, false);
    count.control = Control::NumberBox;
    count.seed_current();
    count.value = Some(3.0);
    let mut named = float("ranged", Control::Slider, Some([0.0, 10.0]), 4.0);
    named.name = "Velocity".into();
    project.tracks[0].devices[0]
        .target_mut()
        .unwrap()
        .events
        .push(EventDef {
            key: EntryKey("hit".into()),
            name: "Hit".into(),
            path: relative("/hit"),
            fields: vec![
                named,
                float("free", Control::NumberBox, None, 0.5),
                count,
                flag,
                pick,
                text,
            ],
        });
    project
}

pub(super) fn event_argument(field: &str) -> ControlRef {
    ControlRef::EventArgument {
        track: TrackId(1),
        event: EventRef {
            device: DeviceId(5),
            key: EntryKey("hit".into()),
        },
        field: FieldKey(field.into()),
    }
}

#[test]
fn an_event_argument_describes_its_kind_range_name_and_value() {
    let project = project_with_hit_event();
    let described = |field: &str| describe_control(&project, &event_argument(field)).unwrap();
    // Row names read `<event> <argument>`: the argument's own name, else its position.
    let ranged = described("ranged");
    assert_eq!(
        (ranged.label.as_str(), ranged.path.as_str()),
        ("Hit Velocity", "Lead / Synth")
    );
    assert_eq!(
        (ranged.kind, ranged.range, ranged.value),
        (ControlKind::Float, Some([0.0, 10.0]), Some(4.0))
    );
    assert_eq!(ranged.access, ControlAccess::ReadWrite);
    // An argument that sets no range has none: the descriptor does not invent one.
    let free = described("free");
    assert_eq!(free.label, "Hit Arg 2");
    assert_eq!(
        (free.kind, free.range, free.value),
        (ControlKind::Float, None, Some(0.5))
    );
    let count = described("count");
    assert_eq!(count.label, "Hit Arg 3");
    assert_eq!(
        (count.kind, count.range, count.value),
        (ControlKind::Int, None, Some(3.0))
    );
    let flag = described("flag");
    assert_eq!(
        (flag.kind, flag.range, flag.value),
        (ControlKind::Toggle, Some([0.0, 1.0]), Some(1.0))
    );
    let pick = described("pick");
    assert_eq!(
        (pick.kind, pick.range, pick.value),
        (ControlKind::Enum, Some([0.0, 2.0]), Some(2.0))
    );
    assert_eq!(pick.choices, ["One", "Two", "Three"]);
    // Text describes, holds no number and stays unmappable (kind Text has no Min or Max).
    let words = described("words");
    assert_eq!(
        (words.kind, words.range, words.value),
        (ControlKind::Text, None, None)
    );
    // Bang and argument are different controls of one event.
    assert_ne!(event_argument("flag"), event());
}

#[test]
fn an_event_argument_stops_describing_when_its_event_argument_or_track_goes() {
    let mut project = project_with_hit_event();
    for control in [
        event_argument("gone"),
        ControlRef::EventArgument {
            track: TrackId(2),
            event: EventRef {
                device: DeviceId(5),
                key: EntryKey("hit".into()),
            },
            field: FieldKey("flag".into()),
        },
        ControlRef::EventArgument {
            track: TrackId(1),
            event: EventRef {
                device: DeviceId(5),
                key: EntryKey("gone".into()),
            },
            field: FieldKey("flag".into()),
        },
    ] {
        assert!(
            describe_control(&project, &control).is_none(),
            "{control:?}"
        );
        assert!(describe_value(&project, &control).is_none(), "{control:?}");
    }
    let flag = event_argument("flag");
    assert!(describe_value(&project, &flag).is_some());
    let events = &mut project.tracks[0].devices[0].target_mut().unwrap().events;
    events
        .last_mut()
        .unwrap()
        .fields
        .retain(|f| f.key.0 != "flag");
    assert!(describe_value(&project, &flag).is_none());
}

#[test]
fn an_event_argument_follows_the_stored_value_and_validates_as_a_mapping_target() {
    let mut project = project_with_hit_event();
    let row = RemoteMapping::new(
        MappingId(1),
        Source::Midi(MidiSource {
            channel: 1,
            message: MidiMessage::Cc(7),
        }),
        event_argument("ranged"),
        0.0,
        10.0,
    );
    project.mappings.push(row.clone());
    project.validate_mappings().unwrap();
    // A mapping to a missing argument fails validation; the event's Bang is another row.
    project.mappings[0].control = event_argument("gone");
    assert!(project.validate_mappings().is_err());
    project.mappings[0].control = event_argument("ranged");
    let mut bang = row;
    bang.id = MappingId(2);
    bang.control = ControlRef::Event {
        track: TrackId(1),
        event: EventRef {
            device: DeviceId(5),
            key: EntryKey("hit".into()),
        },
    };
    project.mappings.push(bang);
    project.validate_mappings().unwrap();
    let events = &mut project.tracks[0].devices[0].target_mut().unwrap().events;
    events.last_mut().unwrap().fields[0].value = Some(9.0);
    assert_eq!(
        describe_value(&project, &event_argument("ranged"))
            .unwrap()
            .value,
        Some(9.0)
    );
}

#[test]
fn deleted_ids_no_longer_describe() {
    let project = project();
    for control in [
        ControlRef::Scene(SceneId(99)),
        ControlRef::TrackStop(TrackId(99)),
        field("cutoff", "gone"),
        field("gone", "f1"),
        ControlRef::ClipSlot {
            track: TrackId(2),
            scene: SceneId(99),
        },
        ControlRef::Event {
            track: TrackId(2),
            event: EventRef {
                device: DeviceId(5),
                key: EntryKey("ping".into()),
            },
        },
    ] {
        assert!(
            describe_control(&project, &control).is_none(),
            "{control:?}"
        );
        assert!(describe_value(&project, &control).is_none(), "{control:?}");
    }
}

#[test]
fn a_clip_slot_follows_its_scene_when_a_scene_is_inserted() {
    let mut project = project();
    let control = ControlRef::ClipSlot {
        track: TrackId(2),
        scene: SceneId(11),
    };
    assert_eq!(
        describe_control(&project, &control).unwrap().label,
        "Slot 2"
    );
    project.scenes.insert(
        0,
        Scene {
            id: SceneId(12),
            name: String::new(),
            follow_actions: SceneFollowActions::default(),
        },
    );
    project.tracks[1].slots.insert(0, Slot::default());
    assert_eq!(
        describe_control(&project, &control).unwrap().label,
        "Slot 3"
    );
}

#[test]
fn every_kind_of_control_round_trips_through_its_canonical_address() {
    let mut controls = vec![
        field("cutoff", "f1"),
        ControlRef::Argument(FieldRef::volume(TrackId(2))),
        event(),
        event_argument("flag"),
        ControlRef::ClipSlot {
            track: TrackId(2),
            scene: SceneId(11),
        },
        ControlRef::Scene(SceneId(10)),
        ControlRef::TrackStop(TrackId(2)),
        ControlRef::TrackActivator(TrackId(2)),
        ControlRef::TrackSolo(TrackId(2)),
    ];
    controls.extend(TransportControl::ALL.map(ControlRef::Transport));
    let mut seen = std::collections::HashSet::new();
    for control in controls {
        let text = control_address(&control);
        assert!(seen.insert(text.clone()), "{text} repeats");
        assert_eq!(parse_control_address(&text), Some(control), "{text}");
    }
}

#[test]
fn label_free_facts_match_display_descriptors_for_each_control_kind() {
    let mut project = project_with_hit_event();
    let target = project.tracks[0].devices[0].target_mut().unwrap();
    for (key, value_type, control) in [
        ("integer", FieldType::Int, Control::NumberBox),
        ("boolean", FieldType::Bool, Control::Toggle),
        ("text", FieldType::String, Control::Text),
    ] {
        target.parameters[0].fields.push(FieldDef {
            control,
            value: Some(0.0),
            ..FieldDef::new(FieldKey(key.into()), value_type, true)
        });
    }
    let mut controls = vec![
        field("cutoff", "f1"),
        field("mode", "f1"),
        field("cutoff", "integer"),
        field("cutoff", "boolean"),
        field("cutoff", "text"),
        event(),
        event_argument("ranged"),
        event_argument("free"),
        event_argument("count"),
        event_argument("flag"),
        event_argument("pick"),
        event_argument("words"),
        ControlRef::Argument(FieldRef::volume(TrackId(2))),
        ControlRef::ClipSlot {
            track: TrackId(2),
            scene: SceneId(11),
        },
        ControlRef::Scene(SceneId(10)),
        ControlRef::TrackStop(TrackId(2)),
        ControlRef::TrackSolo(TrackId(2)),
        ControlRef::TrackActivator(TrackId(2)),
    ];
    controls.extend(TransportControl::ALL.map(ControlRef::Transport));
    for control in controls {
        let display = describe_control(&project, &control).unwrap();
        let facts = describe_value(&project, &control).unwrap();
        assert_eq!(facts, ControlValue::from(&display), "{control:?}");
        assert_eq!(
            facts,
            ControlValue {
                kind: display.kind,
                range: display.range,
                access: display.access,
                value: display.value,
            },
            "{control:?}"
        );
    }
}

#[test]
fn canonical_addresses_match_the_design_leaves() {
    assert_eq!(
        control_address(&field("cutoff", "f1")),
        "/qtable/tracks/by-id/1/devices/by-id/5/parameters/cutoff/arguments/f1/value"
    );
    assert_eq!(
        control_address(&ControlRef::Argument(FieldRef::volume(TrackId(2)))),
        "/qtable/tracks/by-id/2/devices/mixer/parameters/volume/arguments/value/value"
    );
    assert_eq!(
        control_address(&event()),
        "/qtable/tracks/by-id/1/devices/by-id/5/events/ping/fire"
    );
    assert_eq!(
        control_address(&event_argument("ranged")),
        "/qtable/tracks/by-id/1/devices/by-id/5/events/hit/arguments/ranged/value"
    );
    assert_eq!(
        control_address(&ControlRef::ClipSlot {
            track: TrackId(2),
            scene: SceneId(11),
        }),
        "/qtable/tracks/by-id/2/clip_slots/by-scene/11/fire"
    );
    assert_eq!(
        control_address(&ControlRef::Transport(TransportControl::Tempo)),
        "/qtable/transport/tempo/value"
    );
    for (control, leaf) in [
        (TransportControl::Play, "start"),
        (TransportControl::Stop, "stop"),
        (TransportControl::StopAll, "stop_all"),
        (TransportControl::Tempo, "tempo/value"),
        (TransportControl::Quantisation, "quantisation/value"),
        (TransportControl::FollowActions, "follow_actions/value"),
        (TransportControl::ReEnableAutomation, "re_enable_automation"),
    ] {
        let address = control_address(&ControlRef::Transport(control));
        assert_eq!(address, format!("/qtable/transport/{leaf}"));
        assert_eq!(
            parse_control_address(&address),
            Some(ControlRef::Transport(control))
        );
    }
}

#[test]
fn addresses_that_are_not_canonical_leaves_do_not_parse() {
    for text in [
        "",
        "/qtable",
        "/qtable/tracks/at/1/stop",
        "/qtable/scenes/at/1/fire",
        "/qtable/tracks/by-id/x/stop",
        "/qtable/tracks/by-id/1/stop/extra",
        "/qtable/tracks/by-id/1/devices/by-id/5/events/Ping/fire",
        "/qtable/tracks/by-id/1/devices/by-id/5/events//fire",
        "/qtable/tracks/by-id/1/devices/by-id/5/events/hit/arguments/Velocity/value",
        "/qtable/tracks/by-id/1/devices/by-id/5/events/hit/arguments//value",
        "/qtable/tracks/by-id/1/devices/by-id/5/events/hit/arguments/free",
        "/qtable/tracks/by-id/1/devices/by-id/5/events/hit/arguments/free/value/extra",
        "/qtable/tracks/by-id/1/devices/mixer/events/hit/arguments/free/value",
        "/qtable/transport/unknown",
        "/qtable/transport/play",
        "/qtable/song/start",
        "/qtable/song/play",
        "/other/transport/start",
    ] {
        assert_eq!(parse_control_address(text), None, "{text:?}");
    }
}

#[test]
fn view_controls_describe_round_trip_and_keep_selection_out_of_the_project() {
    let project = project();
    let controls = [
        ControlRef::TrackSelect(TrackId(1)),
        ControlRef::TrackSelect(TrackId(3)),
    ]
    .into_iter()
    .chain(SceneNavigation::ALL.map(ControlRef::SceneNavigation));
    for control in controls {
        assert_eq!(
            parse_control_address(&control_address(&control)),
            Some(control.clone())
        );
        let encoded = ron::to_string(&control).unwrap();
        assert_eq!(ron::from_str::<ControlRef>(&encoded).unwrap(), control);
        let descriptor = describe_control(&project, &control).unwrap();
        assert!(!descriptor.label.is_empty());
        assert_eq!(descriptor.value, None, "selection belongs to the host");
        if control == ControlRef::SceneNavigation(SceneNavigation::Select) {
            assert_eq!(descriptor.kind, ControlKind::Int);
            assert_eq!(descriptor.range, None, "it always spans every scene");
        } else {
            assert_eq!(descriptor.kind, ControlKind::Trigger);
        }
    }
    assert!(describe_control(&project, &ControlRef::TrackSelect(TrackId(999))).is_none());
}

#[test]
fn an_event_argument_is_stored_by_ids_in_the_spelling_the_design_gives() {
    let control = ControlRef::EventArgument {
        track: TrackId(3),
        event: EventRef {
            device: DeviceId(1),
            key: EntryKey("kick".into()),
        },
        field: FieldKey("velocity".into()),
    };
    let text = ron::to_string(&control).unwrap();
    assert_eq!(
        text,
        r#"EventArgument(track:(3),event:(device:(1),key:("kick")),field:("velocity"))"#
    );
    assert_eq!(ron::from_str::<ControlRef>(&text).unwrap(), control);
    assert!(ron::from_str::<ControlRef>(&text.replace("track:", "future:true,track:")).is_err());
    assert_eq!(
        parse_control_address(&control_address(&control)),
        Some(control)
    );
}
