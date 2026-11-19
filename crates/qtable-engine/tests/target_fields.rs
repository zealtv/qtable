use qtable_engine::edit::{self, ProjectEdit, TargetEdit};
use qtable_engine::model::*;

fn entry() -> EntryKey {
    EntryKey("point".into())
}
fn field(s: &str) -> FieldKey {
    FieldKey(s.into())
}
fn fixture() -> (Project, DeviceId, ParameterRef) {
    let mut p = Project::default();
    edit::apply(&mut p, ProjectEdit::AddTrack { name: "T".into() }).unwrap();
    let track = p.tracks[0].id;
    edit::apply(
        &mut p,
        ProjectEdit::AddDevice {
            track,
            kind: NewDevice::Target,
            before: None,
        },
    )
    .unwrap();
    let device = p.tracks[0].devices[0].id;
    change(
        &mut p,
        device,
        TargetEdit::AddParameter {
            path: OscAddress::Relative("/point".into()),
            name: "Point".into(),
        },
    );
    (
        p,
        device,
        ParameterRef {
            track,
            device: DeviceRef::Chain(device),
            key: ParameterKey("point".into()),
        },
    )
}
fn change(p: &mut Project, device: DeviceId, edit: TargetEdit) {
    edit::apply(p, ProjectEdit::EditTarget { device, edit }).unwrap();
}
fn rejected(p: &mut Project, device: DeviceId, edit: TargetEdit) {
    let before = p.clone();
    assert!(edit::apply(p, ProjectEdit::EditTarget { device, edit }).is_err());
    assert_eq!(*p, before);
}

fn float_range_fixture(range: [f64; 2], parameter: bool) -> (Project, DeviceId) {
    let (mut p, device, _) = fixture();
    let target = p.tracks[0].devices[0].target_mut().unwrap();
    let f = &mut target.parameters[0].fields[0];
    f.range = Some(range);
    f.default = OscArgument::Float(range[0] as f32);
    f.value = Some(range[1]);
    if !parameter {
        let definition = target.parameters.remove(0);
        target.events.push(EventDef {
            key: definition.key,
            name: definition.name,
            path: definition.path,
            fields: definition.fields,
        });
    }
    p.validate().unwrap();
    (p, device)
}

#[test]
fn float_to_int_refuses_outside_and_single_value_ranges_atomically() {
    let min = f64::from(i32::MIN);
    let max = f64::from(i32::MAX);
    for parameter in [true, false] {
        for range in [
            [1e12, 2e12],
            [-2e12, -1e12],
            [max, max + 1.0],
            [min - 1.0, min],
        ] {
            let (mut p, device) = float_range_fixture(range, parameter);
            rejected(
                &mut p,
                device,
                TargetEdit::SetFieldType {
                    entry: entry(),
                    field: field("f1"),
                    value_type: FieldType::Int,
                },
            );
        }
    }
}

#[test]
fn float_to_int_clips_boundary_ranges_and_rounds_fractional_bounds_outward() {
    let min = f64::from(i32::MIN);
    let max = f64::from(i32::MAX);
    for parameter in [true, false] {
        for (range, expected, default, value) in [
            (
                [min - 1.0, min + 1.0],
                [min, min + 1.0],
                i32::MIN,
                min + 1.0,
            ),
            ([max - 1.0, max + 1.0], [max - 1.0, max], i32::MAX, max),
            ([-0.25, 0.25], [-1.0, 1.0], 0, 0.0),
        ] {
            let (mut p, device) = float_range_fixture(range, parameter);
            change(
                &mut p,
                device,
                TargetEdit::SetFieldType {
                    entry: entry(),
                    field: field("f1"),
                    value_type: FieldType::Int,
                },
            );
            let target = p.tracks[0].devices[0].target().unwrap();
            let f = if parameter {
                &target.parameters[0].fields[0]
            } else {
                &target.events[0].fields[0]
            };
            assert_eq!(f.range, Some(expected));
            assert_eq!(f.value_type, FieldType::Int);
            assert_eq!(f.default, OscArgument::Int(default));
            assert_eq!(f.value, Some(value));
            p.validate().unwrap();
        }
    }
}

#[test]
fn fields_have_frozen_keys_ordered_views_and_atomic_value_edits() {
    let (mut p, device, target) = fixture();
    for value_type in [FieldType::Int, FieldType::Bool] {
        change(
            &mut p,
            device,
            TargetEdit::AddField {
                entry: entry(),
                value_type,
            },
        );
    }
    change(
        &mut p,
        device,
        TargetEdit::SetFieldName {
            entry: entry(),
            field: field("f1"),
            name: "X".into(),
        },
    );
    change(
        &mut p,
        device,
        TargetEdit::MoveField {
            entry: entry(),
            field: field("f3"),
            to: 0,
        },
    );
    edit::apply(
        &mut p,
        ProjectEdit::SetParameter {
            target: target.clone(),
            values: vec![(field("f1"), 0.75), (field("f2"), 0.8)],
        },
    )
    .unwrap();
    let values = p
        .parameter(&target)
        .unwrap()
        .fields
        .into_iter()
        .map(|f| (f.key.into_owned(), f.value))
        .collect::<Vec<_>>();
    assert_eq!(
        values,
        vec![(field("f3"), 0.0), (field("f1"), 0.75), (field("f2"), 1.0)]
    );
    let fields = p.track(target.track).unwrap().fields().collect::<Vec<_>>();
    assert_eq!(fields.len(), 4);
    assert_eq!(fields[0].field, field("f3"));
    assert_eq!(fields[3].field, field("value"));
    assert_eq!(p.field(&fields[1]).unwrap().spec.name, "X");
    assert_eq!(p.field(&fields[1]).unwrap().value, 0.75);
    assert!(
        p.field(&FieldRef {
            parameter: target.clone(),
            field: field("unknown")
        })
        .is_none()
    );
    for values in [
        vec![],
        vec![(field("f1"), 0.2), (field("f1"), 0.3)],
        vec![(field("f1"), 0.2), (field("missing"), 0.0)],
        vec![(field("f1"), 0.2), (field("f2"), f64::NAN)],
    ] {
        let before = p.clone();
        assert!(
            edit::apply(
                &mut p,
                ProjectEdit::SetParameter {
                    target: target.clone(),
                    values
                }
            )
            .is_err()
        );
        assert_eq!(p, before);
    }
    change(
        &mut p,
        device,
        TargetEdit::RemoveField {
            entry: entry(),
            field: field("f2"),
        },
    );
    change(
        &mut p,
        device,
        TargetEdit::RemoveField {
            entry: entry(),
            field: field("f3"),
        },
    );
    rejected(
        &mut p,
        device,
        TargetEdit::RemoveField {
            entry: entry(),
            field: field("f1"),
        },
    );
    rejected(
        &mut p,
        device,
        TargetEdit::MoveField {
            entry: entry(),
            field: field("f1"),
            to: 1,
        },
    );
}

#[test]
fn type_control_matrix_and_bounds_apply_to_events_and_parameters() {
    let types = [
        FieldType::Float,
        FieldType::Int,
        FieldType::Bool,
        FieldType::String,
    ];
    let controls = [
        Control::Slider,
        Control::Dial,
        Control::NumberBox,
        Control::Toggle,
        Control::Enum,
        Control::Text,
    ];
    for parameter in [false, true] {
        for kind in types {
            for control in controls {
                let mut f = FieldDef::new(field("f1"), kind, parameter);
                f.control = control;
                f.seed_current();
                if control == Control::Enum {
                    f.options = match kind {
                        FieldType::String => ["0", "1"]
                            .into_iter()
                            .map(|s| EnumOption {
                                label: s.into(),
                                value: OscArgument::String(s.into()),
                            })
                            .collect(),
                        FieldType::Float => vec![
                            EnumOption {
                                label: "Off".into(),
                                value: OscArgument::Float(0.0),
                            },
                            EnumOption {
                                label: "On".into(),
                                value: OscArgument::Float(1.0),
                            },
                        ],
                        FieldType::Int => vec![
                            EnumOption {
                                label: "Off".into(),
                                value: OscArgument::Int(0),
                            },
                            EnumOption {
                                label: "On".into(),
                                value: OscArgument::Int(1),
                            },
                        ],
                        FieldType::Bool => vec![
                            EnumOption {
                                label: "Off".into(),
                                value: OscArgument::Bool(false),
                            },
                            EnumOption {
                                label: "On".into(),
                                value: OscArgument::Bool(true),
                            },
                        ],
                    };
                    f.default = f.options[0].value.clone();
                } else {
                    f.options.clear();
                }
                assert_eq!(
                    f.validate(parameter).is_ok(),
                    kind.controls(parameter, f.range.is_some())
                        .contains(&control),
                    "{kind:?} {control:?} parameter={parameter}"
                );
            }
        }
    }
    let mut f = FieldDef::new(field("f1"), FieldType::String, true);
    f.options.truncate(1);
    assert!(f.validate(true).is_err());
    f.validate(false).unwrap();
    f.options[0].label = "different".into();
    assert!(f.validate(false).is_err());
    f = FieldDef::new(field("f1"), FieldType::Float, true);
    f.range = Some([0.0, f64::MAX]);
    assert!(f.validate(true).is_err());
    f.range = Some([0.0, 1.0]);
    f.default = OscArgument::Float(f32::INFINITY);
    assert!(f.validate(true).is_err());
    f.default = OscArgument::Int(0);
    assert!(f.validate(true).is_err());
}

#[test]
fn definition_edits_cover_controls_defaults_options_ranges_and_thresholds() {
    let (mut p, device, target) = fixture();
    let key = field("f1");
    edit::apply(&mut p, ProjectEdit::set_value(target.clone(), 0.123456789)).unwrap();
    change(
        &mut p,
        device,
        TargetEdit::SetFieldControl {
            entry: entry(),
            field: key.clone(),
            control: Control::Dial,
        },
    );
    assert_eq!(p.parameter(&target).unwrap().fields[0].value, 0.123456789);
    change(
        &mut p,
        device,
        TargetEdit::SetFieldRange {
            entry: entry(),
            field: key.clone(),
            range: Some([-2.0, 3.0]),
        },
    );
    change(
        &mut p,
        device,
        TargetEdit::SetFieldDefault {
            entry: entry(),
            field: key.clone(),
            default: OscArgument::Float(2.0),
        },
    );
    change(
        &mut p,
        device,
        TargetEdit::SetFieldThreshold {
            entry: entry(),
            field: key.clone(),
            threshold: 0.5,
        },
    );
    assert_eq!(
        p.field(&FieldRef {
            parameter: target.clone(),
            field: key.clone()
        })
        .unwrap()
        .threshold,
        0.5
    );
    rejected(
        &mut p,
        device,
        TargetEdit::SetFieldThreshold {
            entry: entry(),
            field: key.clone(),
            threshold: -1.0,
        },
    );
    rejected(
        &mut p,
        device,
        TargetEdit::SetFieldRange {
            entry: entry(),
            field: key.clone(),
            range: None,
        },
    );
    rejected(
        &mut p,
        device,
        TargetEdit::SetFieldDefault {
            entry: entry(),
            field: key.clone(),
            default: OscArgument::Float(4.0),
        },
    );
    change(
        &mut p,
        device,
        TargetEdit::SetFieldControl {
            entry: entry(),
            field: key.clone(),
            control: Control::Enum,
        },
    );
    change(
        &mut p,
        device,
        TargetEdit::SetFieldOptions {
            entry: entry(),
            field: key.clone(),
            options: vec![
                EnumOption {
                    label: "Low".into(),
                    value: OscArgument::Float(-2.0),
                },
                EnumOption {
                    label: "High".into(),
                    value: OscArgument::Float(3.0),
                },
            ],
        },
    );
    rejected(
        &mut p,
        device,
        TargetEdit::SetFieldOptions {
            entry: entry(),
            field: key.clone(),
            options: vec![],
        },
    );
    change(
        &mut p,
        device,
        TargetEdit::SetFieldType {
            entry: entry(),
            field: key.clone(),
            value_type: FieldType::String,
        },
    );
    let t = p.device(device).unwrap().1.target().unwrap();
    let f = &t.parameters[0].fields[0];
    assert_eq!(f.key, key);
    assert_eq!(f.control, Control::Enum);
    assert_eq!(f.options[0].value, OscArgument::String("-2.00".into()));
    assert_eq!(f.options[0].label, "-2.00");
    rejected(
        &mut p,
        device,
        TargetEdit::SetFieldControl {
            entry: entry(),
            field: key.clone(),
            control: Control::Toggle,
        },
    );
    change(
        &mut p,
        device,
        TargetEdit::SetFieldType {
            entry: entry(),
            field: key.clone(),
            value_type: FieldType::Bool,
        },
    );
    let t = p.device(device).unwrap().1.target().unwrap();
    assert_eq!(t.parameters[0].fields[0].control, Control::Toggle);
}

#[test]
fn new_events_can_grow_to_sixteen_fields_but_no_further() {
    let (mut p, device, _) = fixture();
    let entry = EntryKey("cue".into());
    change(
        &mut p,
        device,
        TargetEdit::AddEvent {
            path: OscAddress::Relative("/cue".into()),
            name: String::new(),
        },
    );
    for _ in 0..16 {
        change(
            &mut p,
            device,
            TargetEdit::AddField {
                entry: entry.clone(),
                value_type: FieldType::Int,
            },
        );
    }
    rejected(
        &mut p,
        device,
        TargetEdit::AddField {
            entry: entry.clone(),
            value_type: FieldType::Int,
        },
    );
    change(
        &mut p,
        device,
        TargetEdit::RemoveField {
            entry: entry.clone(),
            field: field("f16"),
        },
    );
    change(
        &mut p,
        device,
        TargetEdit::AddField {
            entry,
            value_type: FieldType::Int,
        },
    );
}

#[test]
fn enum_option_edits_preserve_the_wire_value_or_fall_back_to_the_default() {
    for kind in [FieldType::Float, FieldType::Int, FieldType::String] {
        let option = |name: &str, value: i32| EnumOption {
            label: name.into(),
            value: match kind {
                FieldType::Float => OscArgument::Float(value as f32),
                FieldType::Int => OscArgument::Int(value),
                FieldType::String => OscArgument::String(name.into()),
                FieldType::Bool => unreachable!(),
            },
        };
        let wide = option("Wide", 10);
        let mid = option("Mid", 20);
        let narrow = option("Narrow", 30);
        let extra = option("Extra", 40);
        for (options, default, expected) in [
            // Removing, reordering or inserting before Mid keeps its wire value.
            (
                vec![mid.clone(), narrow.clone()],
                narrow.clone(),
                mid.clone(),
            ),
            (
                vec![narrow.clone(), wide.clone(), mid.clone()],
                narrow.clone(),
                mid.clone(),
            ),
            (
                vec![extra.clone(), wide.clone(), mid.clone(), narrow.clone()],
                narrow.clone(),
                mid.clone(),
            ),
            // Removing Mid selects the retained default, even away from index zero.
            (
                vec![wide.clone(), narrow.clone()],
                narrow.clone(),
                narrow.clone(),
            ),
            // Removing both Mid and the default resolves the default to the first option.
            (
                vec![extra.clone(), wide.clone()],
                narrow.clone(),
                extra.clone(),
            ),
            // Removing only the default still preserves Mid.
            (
                vec![extra.clone(), mid.clone()],
                narrow.clone(),
                mid.clone(),
            ),
        ] {
            let (mut p, device, target) = fixture();
            let t = p.tracks[0].devices[0].target_mut().unwrap();
            let f = &mut t.parameters[0].fields[0];
            f.value_type = kind;
            f.control = Control::Enum;
            f.range = if kind == FieldType::String {
                None
            } else {
                Some([0.0, 100.0])
            };
            f.options = vec![wide.clone(), mid.clone(), narrow.clone()];
            f.default = default.value.clone();
            f.value = Some(1.0);
            p.validate().unwrap();
            let expected_index = options.iter().position(|o| *o == expected).unwrap() as f64;
            let expected_default = if options.contains(&default) {
                default.value
            } else {
                options[0].value.clone()
            };
            change(
                &mut p,
                device,
                TargetEdit::SetFieldOptions {
                    entry: entry(),
                    field: field("f1"),
                    options,
                },
            );
            let view = p.parameter(&target).unwrap();
            assert_eq!(view.fields[0].value, expected_index);
            assert_eq!(view.fields[0].argument(), expected.value);
            let t = p.device(device).unwrap().1.target().unwrap();
            assert_eq!(t.parameters[0].fields[0].default, expected_default);
        }
    }
}

#[test]
fn field_limits_keys_and_format_seven_round_trip() {
    let (mut p, device, _) = fixture();
    for _ in 1..16 {
        change(
            &mut p,
            device,
            TargetEdit::AddField {
                entry: entry(),
                value_type: FieldType::Float,
            },
        );
    }
    rejected(
        &mut p,
        device,
        TargetEdit::AddField {
            entry: entry(),
            value_type: FieldType::Float,
        },
    );
    let t = p.device(device).unwrap().1.target().unwrap();
    assert_eq!(t.parameters[0].fresh_field_key(""), field("f17"));
    assert_eq!(
        t.parameters[0].fresh_field_key("Name Here"),
        field("name-here")
    );
    let mut fields = t.parameters[0].fields.clone();
    fields[0].name = "Renamed".into();
    assert_eq!(fields[0].display_name(2), "Renamed");
    assert_eq!(fields[1].display_name(1), "Arg 2");
    fields[1].key = fields[0].key.clone();
    assert!(FieldDef::validate_fields(&fields, true).is_err());
    for key in ["", "Upper", "a.b", "é"] {
        assert!(field(key).validate().is_err());
    }
    assert!(FieldDef::validate_fields(&[], true).is_err());
    FieldDef::validate_fields(&[], false).unwrap();
    let path = std::env::temp_dir().join(format!(
        "qtable-target-fields-{}.qtable",
        std::process::id()
    ));
    qtable_engine::file::save(&path, &p).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    let loaded = qtable_engine::file::load(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(p, loaded);
    assert!(text.contains("fields:"));
}

#[test]
fn event_field_moves_and_retypes_follow_keys_without_rewriting_hit_defaults() {
    let mut p = qtable_engine::file::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/v6-lanes.ron"
    ))
    .unwrap();
    let track = TrackId(1);
    let reference = p
        .track(track)
        .unwrap()
        .slots
        .iter()
        .filter_map(|s| s.clip.as_ref())
        .flat_map(|c| &c.events)
        .find(|e| !e.arguments.is_empty())
        .unwrap()
        .definition
        .clone();
    let (device, entry) = (reference.device, reference.key.clone());
    change(
        &mut p,
        device,
        TargetEdit::AddField {
            entry: entry.clone(),
            value_type: FieldType::Int,
        },
    );
    let before: Vec<_> = p
        .track(track)
        .unwrap()
        .slots
        .iter()
        .filter_map(|s| s.clip.as_ref())
        .flat_map(|c| &c.events)
        .filter(|e| e.definition == reference)
        .map(|e| e.arguments.clone())
        .collect();
    change(
        &mut p,
        device,
        TargetEdit::MoveField {
            entry: entry.clone(),
            field: field("f1"),
            to: 1,
        },
    );
    let after: Vec<_> = p
        .track(track)
        .unwrap()
        .slots
        .iter()
        .filter_map(|s| s.clip.as_ref())
        .flat_map(|c| &c.events)
        .filter(|e| e.definition == reference)
        .map(|e| e.arguments.clone())
        .collect();
    for (before, after) in before.iter().zip(&after) {
        assert_eq!(after[0], before[1]);
        assert_eq!(after[1], before[0]);
    }
    change(
        &mut p,
        device,
        TargetEdit::SetFieldType {
            entry: entry.clone(),
            field: field("f1"),
            value_type: FieldType::String,
        },
    );
    change(
        &mut p,
        device,
        TargetEdit::SetFieldDefault {
            entry: entry.clone(),
            field: field("f1"),
            default: OscArgument::String("future hits".into()),
        },
    );
    for (before, hit) in before.iter().zip(
        p.track(track)
            .unwrap()
            .slots
            .iter()
            .filter_map(|s| s.clip.as_ref())
            .flat_map(|c| &c.events)
            .filter(|e| e.definition == reference),
    ) {
        assert_eq!(
            hit.arguments[1],
            before[0].clone().convert(FieldType::String)
        );
    }
    assert_eq!(
        p.track(track)
            .unwrap()
            .event_def(&reference)
            .unwrap()
            .1
            .fields[1]
            .default,
        OscArgument::String("future hits".into())
    );
    rejected(
        &mut p,
        device,
        TargetEdit::SetFieldThreshold {
            entry: entry.clone(),
            field: field("f1"),
            threshold: 0.1,
        },
    );
    change(
        &mut p,
        device,
        TargetEdit::RemoveField {
            entry,
            field: field("f1"),
        },
    );
    p.validate().unwrap();
}

#[test]
fn entering_string_enum_seeds_named_options() {
    let (mut p, device, target) = fixture();
    change(
        &mut p,
        device,
        TargetEdit::SetFieldDefault {
            entry: entry(),
            field: field("f1"),
            default: OscArgument::Float(0.25),
        },
    );
    edit::apply(&mut p, ProjectEdit::set_value(target.clone(), 0.75)).unwrap();
    change(
        &mut p,
        device,
        TargetEdit::SetFieldType {
            entry: entry(),
            field: field("f1"),
            value_type: FieldType::String,
        },
    );
    let view = p.parameter(&target).unwrap();
    assert_eq!(view.fields[0].control, Control::Enum);
    assert_eq!(
        view.fields[0].options[0].value,
        OscArgument::String("One".into())
    );
    assert_eq!(
        view.fields[0].options[1].value,
        OscArgument::String("Two".into())
    );
    assert_eq!(view.fields[0].value, 0.0);
    change(
        &mut p,
        device,
        TargetEdit::SetFieldOptions {
            entry: entry(),
            field: field("f1"),
            options: vec![
                EnumOption {
                    label: "off".into(),
                    value: OscArgument::String("off".into()),
                },
                EnumOption {
                    label: "on".into(),
                    value: OscArgument::String("on".into()),
                },
            ],
        },
    );
    change(
        &mut p,
        device,
        TargetEdit::SetFieldType {
            entry: entry(),
            field: field("f1"),
            value_type: FieldType::Bool,
        },
    );
    let view = p.parameter(&target).unwrap();
    assert_eq!(view.fields[0].control, Control::Toggle);
    assert_eq!(view.fields[0].value, 0.0);
    assert_eq!(view.fields[0].spec.default, 0.0);
}

#[test]
fn enum_values_must_be_unique_in_models_and_edits() {
    for parameter in [false, true] {
        for kind in [FieldType::Float, FieldType::Int, FieldType::String] {
            let (mut p, device, _) = fixture();
            let t = p.tracks[0].devices[0].target_mut().unwrap();
            let mut f = FieldDef::new(field("f1"), kind, parameter);
            f.control = Control::Enum;
            f.seed_current();
            if !parameter && kind != FieldType::String {
                f.range = Some([0.0, 1.0]);
            }
            let value = kind.default_argument();
            f.default = value.clone();
            f.options = vec![
                EnumOption {
                    label: "".into(),
                    value: value.clone(),
                },
                EnumOption {
                    label: "".into(),
                    value,
                },
            ];
            assert_eq!(
                f.validate(parameter).unwrap_err().0,
                "Enum option values must be unique"
            );
            let options = f.options.clone();
            f.options[1] = EnumOption {
                label: "1".into(),
                value: match kind {
                    FieldType::Float => OscArgument::Float(1.0),
                    FieldType::Int => OscArgument::Int(1),
                    FieldType::String => OscArgument::String("1".into()),
                    FieldType::Bool => unreachable!(),
                },
            };
            if parameter {
                t.parameters[0].fields[0] = f;
            } else {
                t.parameters.clear();
                t.events.push(EventDef {
                    key: entry(),
                    name: String::new(),
                    path: OscAddress::Relative("/point".into()),
                    fields: vec![f],
                });
            }
            p.validate().unwrap();
            let before = p.clone();
            let error = edit::apply(
                &mut p,
                ProjectEdit::EditTarget {
                    device,
                    edit: TargetEdit::SetFieldOptions {
                        entry: entry(),
                        field: field("f1"),
                        options,
                    },
                },
            )
            .unwrap_err();
            assert_eq!(error.0, "Enum option values must be unique");
            assert_eq!(p, before);
        }
    }
}

#[test]
fn retyping_non_enums_to_string_uses_fresh_defaults() {
    for (kind, default, expected) in [
        (FieldType::Float, OscArgument::Float(0.0), "Two"),
        (FieldType::Int, OscArgument::Int(1), "Two"),
    ] {
        let (mut p, device, target) = fixture();
        change(
            &mut p,
            device,
            TargetEdit::SetFieldType {
                entry: entry(),
                field: field("f1"),
                value_type: kind,
            },
        );
        change(
            &mut p,
            device,
            TargetEdit::SetFieldDefault {
                entry: entry(),
                field: field("f1"),
                default,
            },
        );
        edit::apply(&mut p, ProjectEdit::set_value(target.clone(), 1.0)).unwrap();
        if kind == FieldType::Float {
            edit::apply(&mut p, ProjectEdit::set_value(target.clone(), 0.0)).unwrap();
        }
        change(
            &mut p,
            device,
            TargetEdit::SetFieldType {
                entry: entry(),
                field: field("f1"),
                value_type: FieldType::String,
            },
        );
        let view = p.parameter(&target).unwrap();
        assert_eq!(view.fields[0].options.len(), 2);
        assert_eq!(view.fields[0].options[1].label, expected);
        assert_eq!(
            view.fields[0].options[1].value,
            OscArgument::String(expected.into())
        );
        assert_eq!(view.fields[0].value, 0.0);
        p.validate().unwrap();
    }
}

#[test]
fn new_enums_use_spelled_labels_and_one_based_typed_values() {
    for parameter in [true, false] {
        for kind in [FieldType::Float, FieldType::Int, FieldType::String] {
            let (mut p, device) = float_range_fixture([0.0, 127.0], parameter);
            change(
                &mut p,
                device,
                TargetEdit::SetFieldType {
                    entry: entry(),
                    field: field("f1"),
                    value_type: kind,
                },
            );
            change(
                &mut p,
                device,
                TargetEdit::SetFieldControl {
                    entry: entry(),
                    field: field("f1"),
                    control: Control::Enum,
                },
            );
            let t = p.device(device).unwrap().1.target().unwrap();
            let f = if parameter {
                &t.parameters[0].fields[0]
            } else {
                &t.events[0].fields[0]
            };
            let expected = match kind {
                FieldType::Float => [OscArgument::Float(1.0), OscArgument::Float(2.0)],
                FieldType::Int => [OscArgument::Int(1), OscArgument::Int(2)],
                FieldType::String => [
                    OscArgument::String("One".into()),
                    OscArgument::String("Two".into()),
                ],
                FieldType::Bool => unreachable!(),
            };
            assert_eq!(
                f.options
                    .iter()
                    .map(|o| o.label.as_str())
                    .collect::<Vec<_>>(),
                ["One", "Two"]
            );
            assert_eq!(
                f.options
                    .iter()
                    .map(|o| o.value.clone())
                    .collect::<Vec<_>>(),
                expected
            );
            assert_eq!(f.default, expected[0]);
            assert_eq!(f.current_argument(), expected[0]);
            p.validate().unwrap();
        }
    }
    let f = FieldDef::new(field("f1"), FieldType::String, true);
    assert_eq!(f.options[0].label, "One");
    assert_eq!(f.options[1].label, "Two");
    f.validate(true).unwrap();
}

#[test]
fn generated_enum_options_spell_through_twenty_then_use_digits() {
    let labels = [
        "One",
        "Two",
        "Three",
        "Four",
        "Five",
        "Six",
        "Seven",
        "Eight",
        "Nine",
        "Ten",
        "Eleven",
        "Twelve",
        "Thirteen",
        "Fourteen",
        "Fifteen",
        "Sixteen",
        "Seventeen",
        "Eighteen",
        "Nineteen",
        "Twenty",
        "21",
        "22",
    ];
    for kind in [FieldType::Float, FieldType::Int, FieldType::String] {
        let mut options = Vec::new();
        for (index, label) in labels.iter().enumerate() {
            let option = EnumOption::next_unused(kind, &options).unwrap();
            assert_eq!(option.label, *label);
            assert_eq!(
                option.value,
                match kind {
                    FieldType::Float => OscArgument::Float((index + 1) as f32),
                    FieldType::Int => OscArgument::Int((index + 1) as i32),
                    FieldType::String => OscArgument::String((*label).into()),
                    FieldType::Bool => unreachable!(),
                }
            );
            options.push(option);
        }
        let removed = options.remove(9);
        options.reverse();
        assert_eq!(EnumOption::next_unused(kind, &options).unwrap(), removed);
        options.push(removed);
        assert_eq!(EnumOption::next_unused(kind, &options).unwrap().label, "23");
    }
    assert_eq!(EnumOption::next_unused(FieldType::Bool, &[]), None);
}

#[test]
fn slider_and_dial_on_an_unranged_event_number_switch_on_a_range_and_clamp() {
    for (kind, range, default, current, clamped) in [
        (
            FieldType::Float,
            [0.0, 1.0],
            OscArgument::Float(2.5),
            OscArgument::Float(-3.0),
            (OscArgument::Float(1.0), OscArgument::Float(0.0)),
        ),
        (
            FieldType::Int,
            [0.0, 127.0],
            OscArgument::Int(500),
            OscArgument::Int(60),
            (OscArgument::Int(127), OscArgument::Int(60)),
        ),
    ] {
        for control in [Control::Slider, Control::Dial] {
            let (mut p, device, _) = fixture();
            let entry = EntryKey("cue".into());
            change(
                &mut p,
                device,
                TargetEdit::AddEvent {
                    path: OscAddress::Relative("/cue".into()),
                    name: String::new(),
                },
            );
            change(
                &mut p,
                device,
                TargetEdit::AddField {
                    entry: entry.clone(),
                    value_type: kind,
                },
            );
            let t = p.tracks[0].devices[0].target().unwrap();
            let f = &t.events[0].fields[0];
            assert_eq!((f.control, f.range), (Control::NumberBox, None));
            assert_eq!(
                kind.controls(false, false),
                [Control::Slider, Control::Dial, Control::NumberBox]
            );
            change(
                &mut p,
                device,
                TargetEdit::SetFieldDefault {
                    entry: entry.clone(),
                    field: field("f1"),
                    default: default.clone(),
                },
            );
            change(
                &mut p,
                device,
                TargetEdit::SetFieldValue {
                    entry: entry.clone(),
                    field: field("f1"),
                    value: current.clone(),
                },
            );
            change(
                &mut p,
                device,
                TargetEdit::SetFieldControl {
                    entry: entry.clone(),
                    field: field("f1"),
                    control,
                },
            );
            let t = p.tracks[0].devices[0].target().unwrap();
            let f = &t.events[0].fields[0];
            assert_eq!((f.control, f.range), (control, Some(range)));
            assert_eq!((f.default.clone(), f.current_argument()), clamped);
            // Turning the range off falls back to a Number box.
            change(
                &mut p,
                device,
                TargetEdit::SetFieldRange {
                    entry,
                    field: field("f1"),
                    range: None,
                },
            );
            let t = p.tracks[0].devices[0].target().unwrap();
            let f = &t.events[0].fields[0];
            assert_eq!((f.control, f.range), (Control::NumberBox, None));
            p.validate().unwrap();
        }
    }
}

#[test]
fn a_parameter_keeps_its_range_when_the_control_changes_to_slider_or_dial() {
    let (mut p, device, target) = fixture();
    for control in [Control::Dial, Control::Slider] {
        change(
            &mut p,
            device,
            TargetEdit::SetFieldControl {
                entry: entry(),
                field: field("f1"),
                control,
            },
        );
        let f = &p.parameter(&target).unwrap().fields[0];
        assert_eq!((f.control, f.spec.min, f.spec.max), (control, 0.0, 1.0));
    }
}

#[test]
fn parameter_edits_preserve_reports_snapshots_and_revision_on_rejection() {
    use qtable_engine::{
        edit::EditReport,
        engine::{Command, Engine},
    };
    let (mut project, device, target) = fixture();
    change(
        &mut project,
        device,
        TargetEdit::AddField {
            entry: entry(),
            value_type: FieldType::Bool,
        },
    );
    let mut engine = Engine::new(project).unwrap();
    let before = engine.project().clone();
    for values in [
        vec![],
        vec![(field("f1"), 0.2), (field("f1"), 0.3)],
        vec![(field("f1"), 0.2), (field("missing"), 0.0)],
        vec![(field("f1"), 0.2), (field("f2"), f64::NAN)],
        vec![(field("f1"), 0.2), (field("f2"), f64::INFINITY)],
    ] {
        assert!(
            engine
                .execute(Command::Edit(ProjectEdit::SetParameter {
                    target: target.clone(),
                    values,
                }))
                .is_err()
        );
        assert_eq!(engine.project(), &before);
        assert_eq!(engine.project_revision(), 0);
    }
    for target in [
        ParameterRef {
            key: ParameterKey("missing".into()),
            ..target.clone()
        },
        ParameterRef {
            track: TrackId(999),
            ..target.clone()
        },
        ParameterRef {
            device: DeviceRef::Chain(DeviceId(999)),
            ..target.clone()
        },
    ] {
        assert!(
            engine
                .execute(Command::Edit(ProjectEdit::SetParameter {
                    target,
                    values: vec![(field("f1"), 0.2)],
                }))
                .is_err()
        );
        assert_eq!(engine.project(), &before);
        assert_eq!(engine.project_revision(), 0);
    }
    // Both accepted changes and unchanged resets count once for unsaved tracking.
    for revision in 1..=2 {
        let report = engine
            .execute(Command::Edit(ProjectEdit::SetParameter {
                target: target.clone(),
                values: vec![(field("f1"), 0.75), (field("f2"), 0.8)],
            }))
            .unwrap();
        assert_eq!(report, EditReport::default());
        assert_eq!(engine.project_revision(), revision);
        let mut expected = before.clone();
        let definition = expected.tracks[0].devices[0].target_mut().unwrap();
        definition.parameters[0].fields[0].value = Some(0.75);
        definition.parameters[0].fields[1].value = Some(1.0);
        assert_eq!(engine.project(), &expected);
        // A client's saved snapshot must remain suitable for restoring the old values.
        assert_eq!(before.parameter(&target).unwrap().fields[0].value, 0.0);
    }
    let volume = ParameterRef::volume(target.track);
    let before_volume = engine.project().clone();
    assert!(
        engine
            .execute(Command::Edit(ProjectEdit::SetParameter {
                target: volume.clone(),
                values: vec![(field("value"), f64::NAN)],
            }))
            .is_err()
    );
    assert_eq!(engine.project(), &before_volume);
    assert_eq!(engine.project_revision(), 2);
    assert_eq!(
        engine
            .execute(Command::Edit(ProjectEdit::SetParameter {
                target: volume,
                values: vec![(field("value"), 0.4)],
            }))
            .unwrap(),
        EditReport::default()
    );
    assert_eq!(engine.project_revision(), 3);
    assert_eq!(engine.project().tracks[0].mixer_device.volume.value, 0.4);
    engine
        .execute(Command::LoadProject(before.clone()))
        .unwrap();
    assert_eq!(engine.project(), &before);
    assert_eq!(engine.project_revision(), 4);
}
