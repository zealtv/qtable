use super::*;

fn relative(path: &str) -> OscAddress {
    OscAddress::Relative(path.into())
}

fn parameter(value_type: FieldType) -> ParameterDef {
    ParameterDef {
        key: EntryKey("level".into()),
        name: "Level".into(),
        path: relative("/level"),
        fields: vec![FieldDef {
            name: "Level".into(),
            value: Some(1.0),
            ..FieldDef::new(FieldKey("f1".into()), value_type, true)
        }],
        send_mode: SendMode::EveryFrame,
    }
}

fn event(key: &str, path: &str) -> EventDef {
    EventDef {
        key: EntryKey(key.into()),
        name: String::new(),
        path: relative(path),
        fields: Vec::new(),
    }
}

#[test]
fn ids_and_lookups_cover_every_track_kind() {
    let mut project = Project::default();
    assert_eq!(project.next_device_id(), DeviceId(1));
    project
        .tracks
        .push(Track::new_group(TrackId(1), "Group", 0));
    project.tracks.push(Track::new(TrackId(2), "Track", 0));
    project.tracks[1].group = Some(TrackId(1));
    project
        .outputs
        .push(Track::new_output(TrackId(3), "Output", "localhost", 9000));
    project.tracks[0]
        .devices
        .push(NewDevice::Target.build(DeviceId(4)));
    project.tracks[1]
        .devices
        .push(NewDevice::Target.build(DeviceId(5)));
    project.outputs[0]
        .devices
        .push(NewDevice::Target.build(DeviceId(6)));
    project
        .main
        .devices
        .push(NewDevice::Target.build(DeviceId(7)));
    // Lookup and id allocation still cover raw data before validation rejects placement.
    assert!(project.validate().is_err());
    for (id, owner) in [(4, 1), (5, 2), (6, 3), (7, 0)] {
        assert_eq!(project.device(DeviceId(id)).unwrap().0.id, TrackId(owner));
        assert!(
            project
                .track(TrackId(owner))
                .unwrap()
                .capabilities()
                .device_chain
        );
        project.device_mut(DeviceId(id)).unwrap().active = false;
        assert!(!project.device(DeviceId(id)).unwrap().1.active);
    }
    assert_eq!(project.next_device_id(), DeviceId(8));
    assert!(project.device(DeviceId(99)).is_none());
    assert!(project.device_mut(DeviceId(99)).is_none());
    project.main.devices[0].id = DeviceId(4);
    assert!(
        project
            .validate()
            .unwrap_err()
            .0
            .contains("duplicate device ID")
    );
}

#[test]
fn insertion_clamps_to_the_category_run() {
    let mut track = Track::new(TrackId(1), "", 0);
    for id in 1..=3 {
        track.devices.push(NewDevice::Target.build(DeviceId(id)));
    }
    assert_eq!(
        track.insert_index(DeviceCategory::Processor, Some(DeviceId(1))),
        3
    );
    assert_eq!(track.insert_index(DeviceCategory::Control, None), 0);
    assert_eq!(
        track.insert_index(DeviceCategory::Generator, Some(DeviceId(3))),
        0
    );
    assert_eq!(track.insert_index(DeviceCategory::Modulator, None), 0);
    assert_eq!(
        track.insert_index(DeviceCategory::Target, Some(DeviceId(2))),
        1
    );
    assert_eq!(track.insert_index(DeviceCategory::Target, None), 3);
    assert_eq!(
        track.insert_index(DeviceCategory::Target, Some(DeviceId(99))),
        3
    );
}

#[test]
fn fresh_keys_use_last_part_and_both_lists_and_reserve_device_on() {
    let mut target = TargetDevice::default();
    assert_eq!(
        target.fresh_key(&relative("/layer/Opacity")),
        EntryKey("opacity".into())
    );
    assert_eq!(target.fresh_key(&relative("/A.B")), EntryKey("a-b".into()));
    assert_eq!(target.fresh_key(&relative("/")), EntryKey("event".into()));
    assert_eq!(
        target.fresh_key(&relative("/device_on")),
        EntryKey("device_on-2".into())
    );
    target.events.push(event("opacity", "/other"));
    let mut p = parameter(FieldType::Float);
    p.key = EntryKey("opacity-2".into());
    target.parameters.push(p);
    assert_eq!(
        target.fresh_key(&OscAddress::Absolute("/Opacity".into())),
        EntryKey("opacity-3".into())
    );
}

#[test]
fn parameter_specs_share_type_rules_and_display_names() {
    for (kind, expected) in [
        (FieldType::Float, ParameterKind::Continuous),
        (FieldType::Int, ParameterKind::Quantised { step: 1.0 }),
        (FieldType::Bool, ParameterKind::Enum(&["Off", "On"])),
    ] {
        let mut p = parameter(kind);
        p.validate().unwrap();
        let f = &mut p.fields[0];
        let spec = f.spec();
        assert_eq!(spec.name, "Level");
        assert_eq!(spec.kind, expected);
        assert_eq!(spec.unit, Unit::None);
        assert_eq!((spec.min, spec.max, spec.default), (0.0, 1.0, 0.0));
        assert_eq!(spec.check(f.value.unwrap()).unwrap(), 1.0);
        f.name.clear();
        assert_eq!(f.display_name(0), "Arg 1");
        p.name.clear();
        assert_eq!(p.display_name(), "/level");
    }
    assert_eq!(
        parameter(FieldType::Bool).fields[0]
            .spec()
            .parse("On")
            .unwrap(),
        1.0
    );
    assert_eq!(event("bang", "/bang").display_name(), "/bang");
}

#[test]
fn target_validation_rejects_bad_keys_and_shared_duplicates() {
    for key in ["", "Upper", "white space", "a.b", "device_on", "é"] {
        let target = TargetDevice {
            events: vec![event(key, "/bang")],
            ..TargetDevice::default()
        };
        assert!(target.validate().is_err(), "{key}");
    }
    let mut target = TargetDevice {
        events: vec![event("level", "/bang")],
        parameters: vec![parameter(FieldType::Float)],
        ..TargetDevice::default()
    };
    assert!(target.validate().unwrap_err().0.contains("duplicate"));
    target.parameters.clear();
    target.events.push(event("level-2", "/bang"));
    target.validate().unwrap(); // Overloaded paths are allowed.
    target.events[1].path = relative("/bad path");
    assert!(target.validate().is_err());
    target.validate_with(validate_stored_address).unwrap();
    target.events[1].path = relative("unled");
    assert!(target.validate_with(validate_stored_address).is_err());
}

#[test]
fn parameter_validation_checks_all_numeric_fields() {
    for kind in [FieldType::Float, FieldType::Int, FieldType::Bool] {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0, 2.0] {
            let mut p = parameter(kind);
            p.fields[0].value = Some(bad);
            assert!(p.validate().is_err());
        }
    }
    for kind in [FieldType::Float, FieldType::Int] {
        for range in [
            [f64::NAN, 1.0],
            [0.0, f64::INFINITY],
            [1.0, 1.0],
            [2.0, 1.0],
        ] {
            let mut p = parameter(kind);
            p.fields[0].range = Some(range);
            assert!(p.validate().is_err());
        }
    }
    let mut p = parameter(FieldType::Int);
    p.fields[0].range = Some([-0.5, 1.0]);
    assert!(p.validate().is_err());
    p.fields[0].range = Some([-10.0, 10.0]);
    p.validate().unwrap();
    p.fields[0].value = Some(0.5);
    assert!(p.validate().is_err());
}

#[test]
fn ensure_event_reuses_signature_and_allocates_only_when_needed() {
    let mut track = Track::new(TrackId(1), "", 0);
    let a = track.ensure_event(relative("/bang"), &[FieldType::Int], || DeviceId(1));
    assert_eq!(
        track.ensure_event(relative("/bang"), &[FieldType::Int], || panic!("unused id")),
        a
    );
    let b = track.ensure_event(relative("/bang"), &[FieldType::Float], || {
        panic!("unused id")
    });
    assert_ne!(a.key, b.key);
    assert_eq!(track.find_event(&relative("/bang")), Some(a.clone()));
    track.devices.push(NewDevice::Target.build(DeviceId(2)));
    let target = track.devices[1].target_mut().unwrap();
    target.events.push(event("later", "/later"));
    let c = track.ensure_event(relative("/later"), &[], || panic!("unused id"));
    assert_eq!(c.device, DeviceId(2));
    assert_eq!(track.devices.len(), 2);
    assert_eq!(
        track
            .event_def(&b)
            .unwrap()
            .1
            .signature()
            .collect::<Vec<_>>(),
        vec![FieldType::Float]
    );
    assert!(
        track
            .event_def(&EventRef {
                device: DeviceId(99),
                key: a.key
            })
            .is_none()
    );
    assert!(track.find_event(&relative("/missing")).is_none());
}

#[test]
fn device_paths_compose_after_track_paths_and_gate_event_pieces() {
    let mut p = Project::default();
    p.tracks
        .push(Track::new_group(TrackId(1), "Group", 0).with_path("/group"));
    p.tracks
        .push(Track::new(TrackId(2), "Track", 0).with_path("/track"));
    p.tracks[1].group = Some(TrackId(1));
    p.outputs
        .push(Track::new_output(TrackId(3), "Output", "localhost", 9000).with_path("/receiver"));
    p.tracks[0].route = Route::single(TrackId(3));
    let r = p.tracks[1].ensure_event(relative("/hit"), &[], || DeviceId(1));
    let absolute = p.tracks[1].ensure_event(OscAddress::Absolute("/global".into()), &[], || {
        panic!("unused id")
    });
    let target = p.tracks[1].devices[0].target_mut().unwrap();
    target.path = Path::new("/layer").unwrap();
    p.validate().unwrap();
    assert_eq!(
        p.inherited_path(TrackId(2), TrackId(3), PathLevel::Device(r.device))
            .as_deref(),
        Some("/receiver/group/track/layer")
    );
    assert_eq!(p.event_path(TrackId(2), &r), Some(relative("/layer/hit")));
    assert_eq!(
        p.resolve_event_address(
            TrackId(2),
            TrackId(3),
            &p.event_path(TrackId(2), &r).unwrap()
        )
        .as_deref(),
        Some("/receiver/group/track/layer/hit")
    );
    assert_eq!(
        p.event_path(TrackId(2), &absolute),
        Some(OscAddress::Absolute("/global".into()))
    );
    assert!(p.event_path(TrackId(1), &r).is_none());
    assert!(
        p.inherited_path(TrackId(1), TrackId(3), PathLevel::Device(r.device))
            .is_none()
    );
    p.device_mut(r.device).unwrap().active = false;
    assert!(p.event_path(TrackId(2), &r).is_none());
    assert!(p.event_path(TrackId(2), &absolute).is_none());
    // A field's prefix remains available for editing an inactive device.
    assert!(
        p.inherited_path(TrackId(2), TrackId(3), PathLevel::Device(r.device))
            .is_some()
    );
    p.outputs[0]
        .devices
        .push(NewDevice::Target.build(DeviceId(2)));
    let target = p.outputs[0].devices[0].target_mut().unwrap();
    target.path = Path::new("/self").unwrap();
    assert_eq!(
        p.inherited_path(TrackId(3), TrackId(3), PathLevel::Device(DeviceId(2)))
            .as_deref(),
        Some("/receiver/self")
    );
}

#[test]
fn serde_keeps_empty_chains_omitted_and_device_defaults() {
    let track = Track::new(TrackId(1), "", 0);
    let text = ron::to_string(&track).unwrap();
    assert!(!text.contains("devices"));
    assert_eq!(ron::from_str::<Track>(&text).unwrap(), track);
    let mut device = NewDevice::Target.build(DeviceId(1));
    let text = ron::to_string(&device).unwrap();
    assert!(!text.contains("active"));
    assert_eq!(ron::from_str::<Device>(&text).unwrap(), device);
    device.active = false;
    let text = ron::to_string(&device).unwrap();
    assert!(text.contains("active:false"));
    assert_eq!(ron::from_str::<Device>(&text).unwrap(), device);
}

#[test]
fn target_data_round_trips_and_project_validation_uses_the_address_policy() {
    let mut project = Project::default();
    project.tracks.push(Track::new(TrackId(1), "Track", 0));
    let reference = project.tracks[0].ensure_event(
        relative("/hit"),
        &[
            FieldType::Int,
            FieldType::Float,
            FieldType::String,
            FieldType::Bool,
        ],
        || DeviceId(1),
    );
    let target = project.tracks[0].devices[0].target_mut().unwrap();
    target.path = Path::new("/target").unwrap();
    target.parameters = vec![parameter(FieldType::Float)];
    project.validate().unwrap();
    let text = ron::to_string(&project).unwrap();
    assert_eq!(ron::from_str::<Project>(&text).unwrap(), project);
    let target = project.tracks[0].devices[0].target_mut().unwrap();
    target.events[0].path = relative("/old path");
    target.parameters[0].path = relative("/old parameter");
    target.path = Path::unchecked("/old target".into());
    assert!(project.validate().is_err());
    project.validate_stored().unwrap();
    assert_eq!(
        project.event_path(TrackId(1), &reference),
        Some(relative("/old target/old path"))
    );
    let target = project.tracks[0].devices[0].target_mut().unwrap();
    target.parameters[0].fields[0].value = Some(f64::NAN);
    assert!(project.validate_stored().is_err());
}

#[test]
fn chain_parameter_refs_do_not_resolve_as_mixer_volume() {
    let project = Project::default();
    let reference = ParameterRef {
        track: project.main.id,
        device: DeviceRef::Chain(DeviceId(1)),
        key: ParameterKey("volume".into()),
    };
    assert!(project.parameter(&reference).is_none());
    let mut project = project;
    assert!(project.parameter_mut(&reference).is_none());
}

#[test]
fn conforming_converts_by_the_table_appends_defaults_and_drops_extras() {
    use OscArgument::*;
    let cases = [
        (Int(-3), FieldType::Float, Float(-3.0)),
        (Int(7), FieldType::String, String("7".into())),
        (Int(0), FieldType::Bool, Bool(false)),
        (Float(2.5), FieldType::Int, Int(3)),
        (Float(1e20), FieldType::Int, Int(i32::MAX)),
        (Float(0.25), FieldType::String, String("0.25".into())),
        (Float(0.5), FieldType::Bool, Bool(true)),
        (String(" 42 ".into()), FieldType::Int, Int(42)),
        (String("x".into()), FieldType::Float, Float(0.0)),
        (String("On".into()), FieldType::Bool, Bool(true)),
        (String("yes".into()), FieldType::Bool, Bool(false)),
        (Bool(true), FieldType::Int, Int(1)),
        (Bool(false), FieldType::Float, Float(0.0)),
        (Bool(true), FieldType::String, String("true".into())),
    ];
    for (from, kind, expected) in cases {
        assert_eq!(from.clone().convert(kind), expected, "{from:?} to {kind:?}");
    }
    let mut track = Track::new(TrackId(1), "t", 0);
    let mut hit = track.hit(
        BeatTime::ZERO,
        OscAddress::Relative("/x".into()),
        vec![Int(1), Int(2)],
        || DeviceId(1),
    );
    assert!(hit.conform(&[FieldType::Float, FieldType::Int, FieldType::String]));
    assert_eq!(hit.arguments, vec![Float(1.0), Int(2), String("".into())]);
    assert!(hit.conform(&[FieldType::Float]));
    assert_eq!(hit.arguments, vec![Float(1.0)]);
    assert!(!hit.conform(&[FieldType::Float]));
    // Appending a default loses nothing.
    assert!(!hit.conform(&[FieldType::Float, FieldType::Bool]));
    assert_eq!(hit.arguments, vec![Float(1.0), Bool(false)]);
    assert_eq!(signature_text(&[FieldType::Int, FieldType::Float]), "i f");
    assert_eq!(signature_text(&[]), "no arguments");
}

#[test]
fn device_and_kind_answer_target_and_new_devices_agree_with_the_catalogue() {
    let mut device = NewDevice::Target.build(DeviceId(3));
    assert_eq!(device, Device::new(DeviceId(3), NewDevice::Target.kind()));
    assert_eq!(device.name, "Target");
    assert!(device.active && device.expanded);
    assert_eq!(device.target(), device.kind.target());
    device.target_mut().unwrap().path = Path::new("/fx").unwrap();
    assert_eq!(device.kind.target().unwrap().path.as_str(), "/fx");
    device
        .kind
        .target_mut()
        .unwrap()
        .events
        .push(event("a", "/a"));
    assert_eq!(device.target().unwrap().events.len(), 1);
}

#[test]
fn targets_lists_the_chains_target_devices_in_order() {
    let mut track = Track::new(TrackId(1), "T", 0);
    assert!(track.targets().next().is_none());
    for id in [4, 2, 9] {
        track.devices.push(NewDevice::Target.build(DeviceId(id)));
    }
    let ids: Vec<_> = track.targets().map(|(device, _)| device.id.0).collect();
    assert_eq!(ids, [4, 2, 9]);
    assert!(
        track
            .targets()
            .all(|(device, target)| device.target() == Some(target))
    );
}

#[test]
fn argument_resolves_device_parameter_and_field_and_nothing_else() {
    let mut track = Track::new(TrackId(1), "T", 0);
    let mut device = NewDevice::Target.build(DeviceId(5));
    device
        .target_mut()
        .unwrap()
        .parameters
        .push(parameter(FieldType::Float));
    track.devices.push(device);
    let reference = |device, parameter: &str, field: &str| ArgumentRef {
        device,
        parameter: ParameterKey(parameter.into()),
        field: FieldKey(field.into()),
    };
    let found = reference(DeviceRef::Chain(DeviceId(5)), "level", "f1");
    let (device, parameter, field) = track.argument(&found).unwrap();
    assert_eq!(device.id, DeviceId(5));
    assert_eq!(parameter.key.0, "level");
    assert_eq!(field.key.0, "f1");
    for missing in [
        reference(DeviceRef::Chain(DeviceId(6)), "level", "f1"),
        reference(DeviceRef::Chain(DeviceId(5)), "other", "f1"),
        reference(DeviceRef::Chain(DeviceId(5)), "level", "f2"),
        ArgumentRef::volume(),
    ] {
        assert!(track.argument(&missing).is_none(), "{missing:?}");
    }
    // Without the field check, a parameter whose field is gone still resolves.
    let stale = reference(DeviceRef::Chain(DeviceId(5)), "level", "f2");
    assert_eq!(track.argument_parameter(&stale).unwrap().1.key.0, "level");
    assert!(track.argument_parameter(&ArgumentRef::volume()).is_none());
}

#[test]
fn target_parameter_lookup_compares_keys_across_types() {
    let mut target = TargetDevice::default();
    target.parameters.push(parameter(FieldType::Float));
    let level = ParameterKey("level".into());
    assert_eq!(target.parameter(&level).unwrap().name, "Level");
    target.parameter_mut(&level).unwrap().name = "Gain".into();
    assert_eq!(target.parameter(&level).unwrap().name, "Gain");
    assert!(target.parameter(&ParameterKey("gone".into())).is_none());
}

#[test]
fn mixer_volume_has_one_spelling_in_each_reference_type() {
    let argument = ArgumentRef::volume();
    assert!(argument.is_volume());
    assert_eq!(argument.device, DeviceRef::Mixer);
    assert_eq!(argument.on(TrackId(7)), FieldRef::volume(TrackId(7)));
    let field = FieldRef::volume(TrackId(7));
    assert!(field.is_volume() && field.parameter.is_volume());
    assert_eq!(field.parameter, ParameterRef::volume(TrackId(7)));
    let mut other = field.clone();
    other.field = FieldKey("f1".into());
    assert!(!other.is_volume());
    let mut chain = ArgumentRef::volume();
    chain.device = DeviceRef::Chain(DeviceId(1));
    assert!(!chain.is_volume());
    // The Mixer resolves it, whatever the track's chain holds.
    let mut project = Project::default();
    project.tracks.push(Track::new(TrackId(7), "T", 0));
    assert!(project.contains_field(&field));
    assert!(project.field(&field).is_some());
    assert!(
        project
            .track(TrackId(7))
            .unwrap()
            .argument_axis(&argument)
            .is_some()
    );
}
