//! Saved mapping rows, atomic edits, and cleanup follow stable control identities.
use qtable_engine::{
    edit::{self, MappingEdit, ProjectEdit, TargetEdit},
    file,
    model::*,
};

fn project() -> Project {
    let mut project = file::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/v9-markers.ron"
    ))
    .unwrap();
    project
        .tracks
        .push(Track::new(TrackId(2), "Spare", project.scenes.len()));
    project.validate().unwrap();
    project
}

fn cc() -> Source {
    Source::Midi(MidiSource {
        channel: 1,
        message: MidiMessage::Cc(21),
    })
}

fn argument(field: &str) -> ControlRef {
    ControlRef::Argument(FieldRef {
        parameter: ParameterRef {
            track: TrackId(1),
            device: DeviceRef::Chain(DeviceId(1)),
            key: ParameterKey("values".into()),
        },
        field: FieldKey(field.into()),
    })
}

fn volume(track: u64) -> ControlRef {
    ControlRef::Argument(FieldRef::volume(TrackId(track)))
}

fn event() -> ControlRef {
    ControlRef::Event {
        track: TrackId(1),
        event: EventRef {
            device: DeviceId(1),
            key: EntryKey("cue".into()),
        },
    }
}

fn add(project: &mut Project, source: Source, control: ControlRef) -> MappingId {
    edit::apply(
        project,
        ProjectEdit::EditMapping(MappingEdit::Add {
            source,
            control,
            min: 0.0,
            max: 1.0,
        }),
    )
    .unwrap();
    project.mappings.last().unwrap().id
}

fn edit_mapping(project: &mut Project, edit: MappingEdit) -> edit::EditReport {
    edit::apply(project, ProjectEdit::EditMapping(edit)).unwrap()
}

fn key(name: &str) -> Source {
    Source::Key(KeySource {
        key: KeyName(name.into()),
    })
}

#[test]
fn mapping_a_control_replaces_only_its_binding_of_the_same_source_kind() {
    let mut p = project();
    let first = add(&mut p, cc(), volume(1));
    let fanout = add(&mut p, cc(), volume(2));
    let keyboard = add(&mut p, key("a"), volume(1));
    edit_mapping(
        &mut p,
        MappingEdit::SetTakeover {
            mapping: first,
            takeover: Takeover::PickUp,
        },
    );
    edit_mapping(
        &mut p,
        MappingEdit::SetRange {
            mapping: first,
            min: 0.9,
            max: 0.1,
        },
    );
    edit_mapping(
        &mut p,
        MappingEdit::SetMode {
            mapping: first,
            mode: SourceMode::Relative(RelativeEncoding::TwosComplement),
        },
    );
    // Notes, CCs and pitch bend all share the single MIDI binding on a target.
    for message in [
        MidiMessage::Note(60),
        MidiMessage::PitchBend,
        MidiMessage::Cc(2),
    ] {
        let previous = p
            .mappings
            .iter()
            .find(|row| row.control == volume(1) && matches!(row.source, Source::Midi(_)))
            .unwrap()
            .id;
        let report = edit_mapping(
            &mut p,
            MappingEdit::Add {
                source: Source::Midi(MidiSource {
                    channel: 2,
                    message,
                }),
                control: volume(1),
                min: 0.0,
                max: 1.0,
            },
        );
        assert_eq!(report.removed_mappings, 1);
        assert!(!report.is_empty());
        assert_eq!(p.mappings.len(), 3);
        assert_eq!(p.mappings[0].id, fanout);
        assert_eq!(p.mappings[0].source, cc());
        assert_eq!(p.mappings[1].id, keyboard);
        let replacement = p.mappings.last().unwrap();
        assert!(replacement.id.0 > previous.0);
        assert_eq!((replacement.min, replacement.max), (0.0, 1.0));
        assert_eq!(replacement.mode, SourceMode::Absolute);
        assert_eq!(replacement.takeover, Takeover::None);
        p.validate().unwrap();
    }
    let midi = p.mappings.last().unwrap().clone();
    let replacement_key = add(&mut p, key("b"), volume(1));
    assert_eq!(p.mappings.len(), 3);
    assert_eq!(p.mappings[1], midi);
    assert_eq!(p.mappings[2].id, replacement_key);
    assert_eq!(p.mappings[2].source, key("b"));
    // The same key can still drive another target.
    add(&mut p, key("b"), volume(2));
    p.validate().unwrap();
}

#[test]
fn stored_duplicate_targets_of_each_source_kind_are_rejected() {
    for source in [cc(), key("a")] {
        for duplicate_source in [
            source.clone(),
            match source {
                Source::Midi(_) => Source::Midi(MidiSource {
                    channel: 16,
                    message: MidiMessage::Note(127),
                }),
                Source::Key(_) => key("b"),
            },
        ] {
            let mut p = project();
            add(&mut p, source.clone(), volume(1));
            let mut duplicate = p.mappings[0].clone();
            duplicate.id = MappingId(2);
            duplicate.source = duplicate_source;
            p.mappings.push(duplicate);
            assert!(
                p.validate()
                    .unwrap_err()
                    .0
                    .contains("control already has a mapping")
            );
            // Saving refuses invalid stored rows; loading must reject them as well.
            let path = std::env::temp_dir().join(format!(
                "qtable-duplicate-mapping-{}.ron",
                std::process::id()
            ));
            assert!(file::save(&path, &p).is_err());
            // Bypass the validated writer to test an invalid format 10 file.
            let body = ron::ser::to_string_pretty(&p, ron::ser::PrettyConfig::default()).unwrap();
            std::fs::write(&path, format!("(version: 10, project: {body})")).unwrap();
            let error = file::load(&path).unwrap_err();
            assert!(
                matches!(error, file::FileError::Model(ref error)
                if error.0.contains("control already has a mapping")),
                "{error}"
            );
            std::fs::remove_file(path).unwrap();
        }
    }
}

#[test]
fn every_source_mode_and_control_survives_a_format10_round_trip() {
    let mut p = project();
    let controls = [
        argument("slider"),
        event(),
        volume(201),
        volume(0),
        ControlRef::ClipSlot {
            track: TrackId(1),
            scene: SceneId(2),
        },
        ControlRef::Scene(SceneId(1)),
        ControlRef::TrackStop(TrackId(1)),
        ControlRef::TrackActivator(TrackId(10)),
        ControlRef::TrackSolo(TrackId(201)),
    ];
    for control in controls
        .into_iter()
        .chain(TransportControl::ALL.map(ControlRef::Transport))
    {
        add(&mut p, cc(), control);
    }
    round_trip(&p);
    for encoding in [
        RelativeEncoding::SignedBit,
        RelativeEncoding::SignedBit2,
        RelativeEncoding::BinOffset,
        RelativeEncoding::TwosComplement,
        RelativeEncoding::SignedBitLinear,
        RelativeEncoding::SignedBit2Linear,
        RelativeEncoding::BinOffsetLinear,
        RelativeEncoding::TwosComplementLinear,
    ] {
        let id = add(&mut p, cc(), argument("slider"));
        edit_mapping(
            &mut p,
            MappingEdit::SetMode {
                mapping: id,
                mode: SourceMode::Relative(encoding),
            },
        );
        round_trip(&p);
    }
    for source in [
        Source::Midi(MidiSource {
            channel: 16,
            message: MidiMessage::PitchBend,
        }),
        Source::Midi(MidiSource {
            channel: 1,
            message: MidiMessage::Note(0),
        }),
        Source::Key(KeySource {
            key: KeyName("a".into()),
        }),
    ] {
        add(&mut p, source, volume(1));
        round_trip(&p);
    }
    p.mappings[0].min = 0.8;
    p.mappings[0].max = 0.2;
    p.mappings[1].takeover = Takeover::PickUp;
    round_trip(&p);
}

fn round_trip(p: &Project) {
    round_trip_named(p, "format10-mappings");
}

/// Tests run in parallel, so each round trip needs a file name of its own.
fn round_trip_named(p: &Project, name: &str) {
    let path = std::env::temp_dir().join(format!("qtable-{name}-{}.ron", std::process::id()));
    file::save(&path, p).unwrap();
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(saved.contains("version: 12"));
    assert_eq!(&file::load(&path).unwrap(), p);
    file::save(&path, &file::load(&path).unwrap()).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), saved);
    // Optional real pre-bump binary, captured before compiling format 10.
    if let Some(reader) = std::env::var_os("QTABLE_FORMAT9_READER") {
        assert!(
            std::process::Command::new(reader)
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn file_loading_rejects_unknown_fields_and_invalid_mapping_data() {
    let mut p = project();
    add(&mut p, cc(), volume(1));
    let path =
        std::env::temp_dir().join(format!("qtable-mapping-guard-{}.ron", std::process::id()));
    file::save(&path, &p).unwrap();
    let source = std::fs::read_to_string(&path).unwrap();
    for bad in [
        source.replacen("version: 12", "future: true, version: 12", 1),
        source.replacen("mappings:", "future: true, mappings:", 1),
        source.replacen("takeover:", "future: true, takeover:", 1),
        source.replacen("channel: 1", "channel: 0", 1),
        source.replacen("min: 0.0", "min: NaN", 1),
        // Even an incorrectly tagged new set cannot lose mappings through the v9 reader.
        source.replacen("version: 12", "version: 9", 1),
    ] {
        assert_ne!(bad, source);
        std::fs::write(&path, bad).unwrap();
        assert!(file::load(&path).is_err());
    }
    let before = std::fs::read(&path).unwrap();
    p.mappings[0].max = f64::INFINITY;
    assert!(file::save(&path, &p).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn invalid_mapping_edits_are_atomic_and_do_not_silently_drop_rows() {
    let mut p = project();
    let id = add(&mut p, cc(), volume(1));
    for edit in [
        MappingEdit::Add {
            source: cc(),
            control: volume(999),
            min: 0.0,
            max: 1.0,
        },
        MappingEdit::Add {
            source: Source::Midi(MidiSource {
                channel: 0,
                message: MidiMessage::Cc(21),
            }),
            control: volume(1),
            min: 0.0,
            max: 1.0,
        },
        MappingEdit::Add {
            source: cc(),
            control: volume(1),
            min: f64::NAN,
            max: 1.0,
        },
        MappingEdit::SetRange {
            mapping: id,
            min: f64::NAN,
            max: 1.0,
        },
        MappingEdit::SetRange {
            mapping: id,
            min: 0.0,
            max: f64::INFINITY,
        },
        MappingEdit::Remove {
            mapping: MappingId(999),
        },
        MappingEdit::SetMode {
            mapping: MappingId(999),
            mode: SourceMode::Absolute,
        },
        MappingEdit::SetTakeover {
            mapping: MappingId(999),
            takeover: Takeover::None,
        },
    ] {
        let before = p.clone();
        assert!(edit::apply(&mut p, ProjectEdit::EditMapping(edit)).is_err());
        assert_eq!(p, before);
    }
    edit_mapping(
        &mut p,
        MappingEdit::SetRange {
            mapping: id,
            min: 0.5,
            max: 0.5,
        },
    );
    let mut duplicate = p.clone();
    duplicate.mappings.push(duplicate.mappings[0].clone());
    assert!(
        duplicate
            .validate()
            .unwrap_err()
            .0
            .contains("duplicate mapping ID")
    );
    for source in [
        Source::Midi(MidiSource {
            channel: 17,
            message: MidiMessage::PitchBend,
        }),
        Source::Midi(MidiSource {
            channel: 1,
            message: MidiMessage::Cc(128),
        }),
        Source::Midi(MidiSource {
            channel: 1,
            message: MidiMessage::Note(255),
        }),
        Source::Key(KeySource {
            key: KeyName(String::new()),
        }),
        Source::Key(KeySource {
            key: KeyName("x\0".into()),
        }),
    ] {
        assert!(source.validate().is_err());
    }
    let note = add(
        &mut p,
        Source::Midi(MidiSource {
            channel: 1,
            message: MidiMessage::Note(60),
        }),
        event(),
    );
    let before = p.clone();
    assert!(
        edit::apply(
            &mut p,
            ProjectEdit::EditMapping(MappingEdit::SetMode {
                mapping: note,
                mode: SourceMode::Relative(RelativeEncoding::SignedBit)
            })
        )
        .is_err()
    );
    assert_eq!(p, before);
    p.mappings[0].id = MappingId(u64::MAX);
    let before = p.clone();
    assert!(
        edit::apply(
            &mut p,
            ProjectEdit::EditMapping(MappingEdit::Add {
                source: cc(),
                control: volume(1),
                min: 0.0,
                max: 1.0
            })
        )
        .is_err()
    );
    assert_eq!(p, before);
}

#[test]
fn deleting_controls_removes_only_their_mappings_and_reports_each_row() {
    for (deletion, controls) in [
        (
            ProjectEdit::EditTarget {
                device: DeviceId(1),
                edit: TargetEdit::RemoveField {
                    entry: EntryKey("values".into()),
                    field: FieldKey("slider".into()),
                },
            },
            vec![argument("slider")],
        ),
        (
            ProjectEdit::EditTarget {
                device: DeviceId(1),
                edit: TargetEdit::RemoveEntries {
                    keys: vec![EntryKey("values".into()), EntryKey("cue".into())],
                },
            },
            vec![argument("slider"), event()],
        ),
        (
            ProjectEdit::RemoveDevices {
                devices: vec![DeviceId(1)],
            },
            vec![argument("slider"), event()],
        ),
        (
            ProjectEdit::RemoveScene { scene: SceneId(2) },
            vec![
                ControlRef::Scene(SceneId(2)),
                ControlRef::ClipSlot {
                    track: TrackId(1),
                    scene: SceneId(2),
                },
            ],
        ),
        (
            ProjectEdit::RemoveScenes {
                scenes: vec![SceneId(2)],
            },
            vec![
                ControlRef::Scene(SceneId(2)),
                ControlRef::ClipSlot {
                    track: TrackId(1),
                    scene: SceneId(2),
                },
            ],
        ),
        (
            ProjectEdit::RemoveTracks {
                tracks: vec![TrackId(1)],
            },
            vec![
                argument("slider"),
                event(),
                volume(1),
                ControlRef::TrackStop(TrackId(1)),
                ControlRef::TrackSolo(TrackId(10)),
                ControlRef::ClipSlot {
                    track: TrackId(1),
                    scene: SceneId(1),
                },
            ],
        ),
        (
            ProjectEdit::RemoveTracks {
                tracks: vec![TrackId(201)],
            },
            vec![volume(201), ControlRef::TrackActivator(TrackId(201))],
        ),
        (
            ProjectEdit::Ungroup { group: TrackId(10) },
            vec![volume(10), ControlRef::TrackSolo(TrackId(10))],
        ),
    ] {
        let mut p = project();
        let survivor = add(&mut p, cc(), volume(2));
        // MIDI and key bindings on each deleted control must both be removed.
        for control in &controls {
            add(&mut p, cc(), control.clone());
            add(&mut p, key("a"), control.clone());
        }
        let report = edit::apply(&mut p, deletion.clone()).unwrap();
        assert_eq!(report.removed_mappings, controls.len() * 2, "{deletion:?}");
        assert_eq!(p.mappings.len(), 1, "{deletion:?}");
        assert_eq!(p.mappings[0].id, survivor);
        p.validate().unwrap();
    }
}

#[test]
fn mappings_follow_scene_and_device_ids_and_survive_clip_and_range_edits() {
    let mut p = project();
    add(
        &mut p,
        cc(),
        ControlRef::ClipSlot {
            track: TrackId(1),
            scene: SceneId(1),
        },
    );
    add(&mut p, cc(), argument("slider"));
    add(&mut p, cc(), event());
    add(&mut p, cc(), volume(1));
    let original = p.mappings.clone();
    for edit in [
        ProjectEdit::AddScene {
            at: 0,
            name: "Inserted".into(),
        },
        ProjectEdit::MoveScene { from: 1, to: 2 },
        ProjectEdit::DeleteClip {
            track: TrackId(1),
            slot: 2,
        },
        ProjectEdit::CreateClip {
            track: TrackId(1),
            slot: 2,
            length: BeatTime(960),
        },
        ProjectEdit::MoveTrack { from: 0, to: 1 },
        ProjectEdit::AddDevice {
            track: TrackId(1),
            kind: NewDevice::Target,
            before: Some(DeviceId(1)),
        },
        ProjectEdit::MoveDevice {
            device: DeviceId(1),
            before: Some(DeviceId(2)),
        },
        ProjectEdit::EditTarget {
            device: DeviceId(1),
            edit: TargetEdit::SetFieldRange {
                entry: EntryKey("values".into()),
                field: FieldKey("slider".into()),
                range: Some([0.25, 0.75]),
            },
        },
    ] {
        let report = edit::apply(&mut p, edit.clone()).unwrap();
        assert_eq!(report.removed_mappings, 0, "{edit:?}");
        assert_eq!(p.mappings, original);
    }
    let before = p.clone();
    assert!(
        edit::apply(
            &mut p,
            ProjectEdit::RemoveTracks {
                tracks: vec![TrackId(1), TrackId(999)]
            }
        )
        .is_err()
    );
    assert_eq!(p, before);
}

#[test]
fn removing_multiple_scenes_prunes_only_their_scene_and_slot_mappings() {
    let mut p = project();
    edit::apply(
        &mut p,
        ProjectEdit::AddScene {
            at: 0,
            name: "Temporary".into(),
        },
    )
    .unwrap();
    let removed: Vec<_> = p.scenes.iter().take(2).map(|s| s.id).collect();
    let survivor_scene = p.scenes.last().unwrap().id;
    let survivor = add(&mut p, cc(), ControlRef::Scene(survivor_scene));
    for scene in &removed {
        for control in [
            ControlRef::Scene(*scene),
            ControlRef::ClipSlot {
                track: TrackId(1),
                scene: *scene,
            },
        ] {
            add(&mut p, cc(), control.clone());
            add(&mut p, key("a"), control);
        }
    }
    let report = edit::apply(&mut p, ProjectEdit::RemoveScenes { scenes: removed }).unwrap();
    assert_eq!(report.removed_mappings, 8);
    assert_eq!(
        p.mappings.iter().map(|m| m.id).collect::<Vec<_>>(),
        [survivor]
    );
    p.validate().unwrap();
}

#[test]
fn sensitivity_and_gate_edit_atomically_and_round_trip() {
    let mut p = project();
    let id = add(&mut p, cc(), volume(1));
    assert_eq!(p.mappings[0].sensitivity, 1.0);
    edit_mapping(
        &mut p,
        MappingEdit::SetMode {
            mapping: id,
            mode: SourceMode::Relative(RelativeEncoding::TwosComplement),
        },
    );
    edit_mapping(
        &mut p,
        MappingEdit::SetSensitivity {
            mapping: id,
            sensitivity: 2.5,
        },
    );
    assert_eq!(p.mappings[0].sensitivity, 2.5);
    round_trip_named(&p, "sensitivity-gate");
    // The range is 1% to 1000%; a refused edit leaves the project as it was.
    for sensitivity in [0.0, 0.009, 10.5, f64::NAN, f64::INFINITY, -1.0] {
        let before = p.clone();
        let result = edit::apply(
            &mut p,
            ProjectEdit::EditMapping(MappingEdit::SetSensitivity {
                mapping: id,
                sensitivity,
            }),
        );
        assert!(result.is_err(), "{sensitivity}");
        assert_eq!(p, before);
    }
    let before = p.clone();
    assert!(
        edit::apply(
            &mut p,
            ProjectEdit::EditMapping(MappingEdit::SetSensitivity {
                mapping: MappingId(999),
                sensitivity: 1.0,
            })
        )
        .is_err()
    );
    assert_eq!(p, before);
    // Sensitivity changes the mapping list only, which stays out of the undo history.
    assert!(
        !ProjectEdit::EditMapping(MappingEdit::SetSensitivity {
            mapping: id,
            sensitivity: 2.0
        })
        .makes_step()
    );
    // Gate needs a note or key: a CC row refuses it, a key row takes it.
    let before = p.clone();
    assert!(
        edit::apply(
            &mut p,
            ProjectEdit::EditMapping(MappingEdit::SetMode {
                mapping: id,
                mode: SourceMode::Gate,
            })
        )
        .is_err()
    );
    assert_eq!(p, before);
    let gate = add(&mut p, key("g"), volume(1));
    edit_mapping(
        &mut p,
        MappingEdit::SetMode {
            mapping: gate,
            mode: SourceMode::Gate,
        },
    );
    edit_mapping(
        &mut p,
        MappingEdit::SetTakeover {
            mapping: gate,
            takeover: Takeover::ValueScaling,
        },
    );
    round_trip_named(&p, "sensitivity-gate");
    // A trigger holds no value, so Gate there is stored and acts as Latch (the resolver's
    // call); the model takes it, and a file holding it loads.
    let scene = p.scenes[0].id;
    let slot = add(
        &mut p,
        key("h"),
        ControlRef::ClipSlot {
            track: TrackId(1),
            scene,
        },
    );
    edit_mapping(
        &mut p,
        MappingEdit::SetMode {
            mapping: slot,
            mode: SourceMode::Gate,
        },
    );
    round_trip_named(&p, "sensitivity-gate-trigger");
    // Gate still needs a note or key, whatever the control.
    let before = p.clone();
    let cc_slot = add(
        &mut p,
        cc(),
        ControlRef::ClipSlot {
            track: TrackId(1),
            scene,
        },
    );
    assert!(
        edit::apply(
            &mut p,
            ProjectEdit::EditMapping(MappingEdit::SetMode {
                mapping: cc_slot,
                mode: SourceMode::Gate,
            })
        )
        .is_err()
    );
    p.mappings.pop();
    assert_eq!(p.mappings, before.mappings);
}

fn event_argument(field: &str) -> ControlRef {
    ControlRef::EventArgument {
        track: TrackId(1),
        event: EventRef {
            device: DeviceId(1),
            key: EntryKey("cue".into()),
        },
        field: FieldKey(field.into()),
    }
}

#[test]
fn deleting_an_event_argument_event_device_or_track_removes_only_its_mappings() {
    for deletion in [
        ProjectEdit::EditTarget {
            device: DeviceId(1),
            edit: TargetEdit::RemoveField {
                entry: EntryKey("cue".into()),
                field: FieldKey("slider".into()),
            },
        },
        ProjectEdit::EditTarget {
            device: DeviceId(1),
            edit: TargetEdit::RemoveEntries {
                keys: vec![EntryKey("cue".into())],
            },
        },
        ProjectEdit::RemoveDevices {
            devices: vec![DeviceId(1)],
        },
        ProjectEdit::RemoveTracks {
            tracks: vec![TrackId(1)],
        },
    ] {
        let mut p = project();
        let doomed = add(&mut p, cc(), event_argument("slider"));
        let bang = add(&mut p, key("a"), event());
        let other = add(&mut p, key("b"), event_argument("dial"));
        let removed = edit_mapping_and(&mut p, deletion.clone());
        // Removing one argument leaves the event's Bang and its other argument mapped; removing
        // the event, its device or its track takes all three.
        let only_the_argument = matches!(
            deletion,
            ProjectEdit::EditTarget {
                edit: TargetEdit::RemoveField { .. },
                ..
            }
        );
        let survivors: Vec<_> = p.mappings.iter().map(|row| row.id).collect();
        let expected = if only_the_argument {
            vec![bang, other]
        } else {
            vec![]
        };
        assert_eq!(survivors, expected, "{deletion:?}");
        assert_eq!(removed, 3 - expected.len(), "{deletion:?}");
        assert!(!survivors.contains(&doomed));
        p.validate().unwrap();
    }
}

fn edit_mapping_and(project: &mut Project, edit: ProjectEdit) -> usize {
    edit::apply(project, edit).unwrap().removed_mappings
}

#[test]
fn a_gate_mapping_does_not_block_retyping_its_event_argument() {
    let mut p = project();
    let id = add(&mut p, key("g"), event_argument("slider"));
    edit_mapping(
        &mut p,
        MappingEdit::SetMode {
            mapping: id,
            mode: SourceMode::Gate,
        },
    );
    // A String argument holds no value; the row stays, inert, as a Latch row would.
    edit::apply(
        &mut p,
        ProjectEdit::EditTarget {
            device: DeviceId(1),
            edit: TargetEdit::SetFieldType {
                entry: EntryKey("cue".into()),
                field: FieldKey("slider".into()),
                value_type: FieldType::String,
            },
        },
    )
    .unwrap();
    assert_eq!(p.mappings.len(), 1);
    assert_eq!(p.mappings[0].mode, SourceMode::Gate);
    p.validate().unwrap();
    round_trip_named(&p, "gate-retyped-argument");
}
