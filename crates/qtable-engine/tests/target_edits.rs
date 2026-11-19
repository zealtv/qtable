use qtable_engine::edit::{self, ProjectEdit, TargetEdit};
use qtable_engine::engine::{Command, Engine};
use qtable_engine::model::*;
use qtable_test_support::TestReceiver;
use rosc::OscType;
use std::time::{Duration, Instant};

fn relative(text: &str) -> OscAddress {
    OscAddress::Relative(text.into())
}
fn key(text: &str) -> EntryKey {
    EntryKey(text.into())
}
fn change(p: &mut Project, device: DeviceId, edit: TargetEdit) {
    edit::apply(p, ProjectEdit::EditTarget { device, edit }).unwrap();
}
fn target(p: &Project, id: DeviceId) -> &TargetDevice {
    p.device(id).unwrap().1.target().unwrap()
}
fn fixture() -> (Project, DeviceId) {
    let mut p = qtable_engine::file::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/v6-lanes.ron"
    ))
    .unwrap();
    let device = p.track(TrackId(1)).unwrap().devices[0].id;
    change(
        &mut p,
        device,
        TargetEdit::AddParameter {
            path: relative("/level"),
            name: "Level".into(),
        },
    );
    (p, device)
}
fn parameter_ref(p: &Project, device: DeviceId) -> ParameterRef {
    ParameterRef {
        track: p.device(device).unwrap().0.id,
        device: DeviceRef::Chain(device),
        key: ParameterKey("level".into()),
    }
}

#[test]
fn entries_have_frozen_keys_and_paths_are_normalized() {
    let (mut p, device) = fixture();
    change(&mut p, device, TargetEdit::SetPath("layer".into()));
    change(
        &mut p,
        device,
        TargetEdit::AddEvent {
            path: relative("cue"),
            name: String::new(),
        },
    );
    change(
        &mut p,
        device,
        TargetEdit::AddEvent {
            path: relative("cue"),
            name: "Second".into(),
        },
    );
    change(
        &mut p,
        device,
        TargetEdit::SetName {
            key: key("cue"),
            name: "Cue".into(),
        },
    );
    change(
        &mut p,
        device,
        TargetEdit::SetEntryPath {
            key: key("cue"),
            path: OscAddress::Absolute("show/cue".into()),
        },
    );
    change(
        &mut p,
        device,
        TargetEdit::SetName {
            key: key("level"),
            name: "Opacity".into(),
        },
    );
    change(
        &mut p,
        device,
        TargetEdit::SetEntryPath {
            key: key("level"),
            path: relative("opacity"),
        },
    );
    let t = target(&p, device);
    assert_eq!(t.path.as_str(), "/layer");
    let cue = t.events.iter().find(|e| e.key == key("cue")).unwrap();
    assert_eq!(cue.path, OscAddress::Absolute("/show/cue".into()));
    assert_eq!(cue.name, "Cue");
    assert!(t.events.iter().any(|e| e.key == key("cue-2")));
    let r = parameter_ref(&p, device);
    let view = p.parameter(&r).unwrap();
    assert_eq!(view.fields[0].spec.name, "Opacity");
    assert_eq!(view.fields[0].spec.min, 0.0);
    assert!(view.enabled);
    assert!(
        p.parameter_address(&r, TrackId(201))
            .unwrap()
            .ends_with("/layer/opacity")
    );
    let original_refs: Vec<_> = p
        .track(TrackId(1))
        .unwrap()
        .slots
        .iter()
        .filter_map(|s| s.clip.as_ref())
        .flat_map(|c| c.events.iter().map(|e| e.definition.clone()))
        .collect();
    let event_key = original_refs[0].key.clone();
    change(
        &mut p,
        device,
        TargetEdit::SetEntryPath {
            key: event_key.clone(),
            path: relative("changed"),
        },
    );
    for r in original_refs.iter().filter(|r| r.key == event_key) {
        assert_eq!(
            p.event_path(TrackId(1), r),
            Some(relative("/layer/changed"))
        );
    }
    let text = ron::to_string(&p).unwrap();
    assert_eq!(ron::from_str::<Project>(&text).unwrap(), p);
}

#[test]
fn signature_edits_conform_every_clip_and_remove_the_requested_position() {
    let (mut p, device) = fixture();
    let track = p.track(TrackId(1)).unwrap();
    let reference = track
        .slots
        .iter()
        .filter_map(|s| s.clip.as_ref())
        .flat_map(|c| &c.events)
        .find(|e| !e.arguments.is_empty())
        .unwrap()
        .definition
        .clone();
    let before_other = p.track(TrackId(2)).unwrap().clone();
    let signature = track
        .event_def(&reference)
        .unwrap()
        .1
        .signature()
        .collect::<Vec<_>>();
    change(
        &mut p,
        device,
        TargetEdit::AddField {
            entry: reference.key.clone(),
            value_type: FieldType::String,
        },
    );
    change(
        &mut p,
        device,
        TargetEdit::SetFieldName {
            entry: reference.key.clone(),
            field: qtable_engine::model::FieldKey(format!("f{}", signature.len() + 1)),
            name: "label".into(),
        },
    );
    change(
        &mut p,
        device,
        TargetEdit::SetFieldType {
            entry: reference.key.clone(),
            field: qtable_engine::model::FieldKey("f1".into()),
            value_type: FieldType::String,
        },
    );
    let before: Vec<_> = p
        .track(TrackId(1))
        .unwrap()
        .slots
        .iter()
        .filter_map(|s| s.clip.as_ref())
        .flat_map(|c| &c.events)
        .filter(|e| e.definition == reference)
        .map(|e| e.arguments.clone())
        .collect();
    assert!(before.len() >= 2);
    assert!(
        before
            .iter()
            .all(|args| matches!(args[0], OscArgument::String(_))
                && args.last() == Some(&OscArgument::String(String::new())))
    );
    change(
        &mut p,
        device,
        TargetEdit::RemoveField {
            entry: reference.key.clone(),
            field: qtable_engine::model::FieldKey("f1".into()),
        },
    );
    let after: Vec<_> = p
        .track(TrackId(1))
        .unwrap()
        .slots
        .iter()
        .filter_map(|s| s.clip.as_ref())
        .flat_map(|c| &c.events)
        .filter(|e| e.definition == reference)
        .map(|e| e.arguments.clone())
        .collect();
    assert_eq!(
        after,
        before
            .into_iter()
            .map(|args| args[1..].to_vec())
            .collect::<Vec<_>>()
    );
    assert_eq!(*p.track(TrackId(2)).unwrap(), before_other);
    p.validate().unwrap();
}

#[test]
fn removing_entries_removes_declared_and_derived_lanes_and_hits_only_on_the_owner() {
    let (mut p, device) = fixture();
    let event_key = target(&p, device).events[0].key.clone();
    let other = p.track(TrackId(2)).unwrap().clone();
    let slots = p.track(TrackId(1)).unwrap().slots.len();
    change(
        &mut p,
        device,
        TargetEdit::RemoveEntries {
            keys: vec![event_key.clone(), key("level")],
        },
    );
    for c in p
        .track(TrackId(1))
        .unwrap()
        .slots
        .iter()
        .filter_map(|s| s.clip.as_ref())
    {
        assert!(
            !c.lane_refs()
                .iter()
                .any(|r| r.device == device && r.key == event_key)
        );
        assert!(
            !c.events
                .iter()
                .any(|e| e.definition.device == device && e.definition.key == event_key)
        );
    }
    assert_eq!(p.track(TrackId(1)).unwrap().slots.len(), slots);
    assert_eq!(*p.track(TrackId(2)).unwrap(), other);
    assert!(p.parameter(&parameter_ref(&p, device)).is_none());
}

#[test]
fn parameter_type_range_value_mode_and_threshold_share_the_parameter_api() {
    let (mut p, device) = fixture();
    let r = parameter_ref(&p, device);
    change(
        &mut p,
        device,
        TargetEdit::SetFieldRange {
            field: qtable_engine::model::FieldKey("f1".into()),
            entry: key("level"),
            range: Some([-1.2, 3.2]),
        },
    );
    change(
        &mut p,
        device,
        TargetEdit::SetFieldDefault {
            entry: key("level"),
            field: FieldKey("f1".into()),
            default: OscArgument::Float(3.2),
        },
    );
    edit::apply(&mut p, ProjectEdit::set_value(r.clone(), 2.6)).unwrap();
    change(
        &mut p,
        device,
        TargetEdit::SetFieldType {
            field: qtable_engine::model::FieldKey("f1".into()),
            entry: key("level"),
            value_type: FieldType::Int,
        },
    );
    let def = &target(&p, device).parameters[0];
    assert_eq!(
        (
            def.fields[0].spec().min,
            def.fields[0].spec().max,
            def.fields[0].default_value(),
            def.fields[0].value.unwrap()
        ),
        (-2.0, 4.0, 3.0, 3.0)
    );
    edit::apply(&mut p, ProjectEdit::set_value(r.clone(), 1.6)).unwrap();
    assert_eq!(p.parameter(&r).unwrap().fields[0].value, 2.0);
    change(
        &mut p,
        device,
        TargetEdit::SetFieldType {
            field: qtable_engine::model::FieldKey("f1".into()),
            entry: key("level"),
            value_type: FieldType::Bool,
        },
    );
    let def = &target(&p, device).parameters[0];
    assert_eq!(
        (
            def.fields[0].spec().min,
            def.fields[0].spec().max,
            def.fields[0].default_value(),
            def.fields[0].value.unwrap()
        ),
        (0.0, 1.0, 1.0, 1.0)
    );
    change(
        &mut p,
        device,
        TargetEdit::SetFieldType {
            field: qtable_engine::model::FieldKey("f1".into()),
            entry: key("level"),
            value_type: FieldType::Float,
        },
    );
    change(
        &mut p,
        device,
        TargetEdit::SetFieldControl {
            entry: key("level"),
            field: FieldKey("f1".into()),
            control: Control::Slider,
        },
    );
    change(
        &mut p,
        device,
        TargetEdit::SetFieldDefault {
            entry: key("level"),
            field: FieldKey("f1".into()),
            default: OscArgument::Float(0.0),
        },
    );
    change(
        &mut p,
        device,
        TargetEdit::SetFieldRange {
            field: qtable_engine::model::FieldKey("f1".into()),
            entry: key("level"),
            range: Some([0.0, 0.4]),
        },
    );
    assert_eq!(
        (
            p.parameter(&r).unwrap().fields[0].spec.default,
            p.parameter(&r).unwrap().fields[0].value
        ),
        (0.0, 0.4)
    );
    change(
        &mut p,
        device,
        TargetEdit::SetParameterSendMode {
            key: key("level"),
            mode: SendMode::EveryFrame,
        },
    );
    change(
        &mut p,
        device,
        TargetEdit::SetFieldThreshold {
            field: qtable_engine::model::FieldKey("f1".into()),
            entry: key("level"),
            threshold: 0.2,
        },
    );
    assert_eq!(
        p.parameter(&r).unwrap().output.send_mode,
        SendMode::EveryFrame
    );
    assert_eq!(p.parameter(&r).unwrap().fields[0].threshold, 0.2);
}

#[test]
fn rejected_target_edits_leave_the_project_whole() {
    let (mut p, device) = fixture();
    for edit in [
        TargetEdit::SetPath("bad path".into()),
        TargetEdit::AddEvent {
            path: relative("bad path"),
            name: String::new(),
        },
        TargetEdit::SetEntryPath {
            key: key("level"),
            path: relative("bad path"),
        },
        TargetEdit::RemoveEntries { keys: vec![] },
        TargetEdit::RemoveEntries {
            keys: vec![key("level"), key("missing")],
        },
        TargetEdit::SetName {
            key: key("missing"),
            name: String::new(),
        },
        TargetEdit::AddField {
            entry: key("missing"),
            value_type: FieldType::Int,
        },
        TargetEdit::RemoveField {
            entry: key("kick"),
            field: qtable_engine::model::FieldKey("missing".into()),
        },
        TargetEdit::SetFieldType {
            entry: key("kick"),
            field: qtable_engine::model::FieldKey("missing".into()),
            value_type: FieldType::Bool,
        },
        TargetEdit::SetFieldName {
            entry: key("kick"),
            field: qtable_engine::model::FieldKey("missing".into()),
            name: String::new(),
        },
        TargetEdit::SetFieldRange {
            field: qtable_engine::model::FieldKey("f1".into()),
            entry: key("level"),
            range: Some([1.0, 1.0]),
        },
        TargetEdit::SetFieldRange {
            field: qtable_engine::model::FieldKey("f1".into()),
            entry: key("level"),
            range: Some([0.0, f64::MAX]),
        },
        TargetEdit::SetFieldThreshold {
            field: qtable_engine::model::FieldKey("f1".into()),
            entry: key("level"),
            threshold: -1.0,
        },
    ] {
        let before = p.clone();
        assert!(edit::apply(&mut p, ProjectEdit::EditTarget { device, edit }).is_err());
        assert_eq!(p, before);
    }
    let before = p.clone();
    assert!(
        edit::apply(
            &mut p,
            ProjectEdit::set_value(parameter_ref(&before, device), f64::NAN)
        )
        .is_err()
    );
    assert_eq!(p, before);
}

fn udp_project(receiver: &TestReceiver) -> (Project, DeviceId, ParameterRef) {
    let mut p = Project {
        tracks: vec![Track::new(TrackId(1), "Source", 0)],
        outputs: vec![Track::new_output(
            TrackId(2),
            "Out",
            "127.0.0.1",
            receiver.addr().port(),
        )],
        ..Project::default()
    };
    p.tracks[0].route = Route::single(TrackId(2));
    edit::apply(
        &mut p,
        ProjectEdit::AddDevice {
            track: TrackId(1),
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
            path: relative("/level"),
            name: String::new(),
        },
    );
    let r = parameter_ref(&p, device);
    (p, device, r)
}
fn execute(engine: &mut Engine, device: DeviceId, edit: TargetEdit) {
    engine
        .execute(Command::Edit(ProjectEdit::EditTarget { device, edit }))
        .unwrap();
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
            .wait_for_messages(1, Duration::from_millis(90))
            .is_err()
    );
}

#[test]
fn target_parameters_send_typed_values_start_once_and_gate_the_device() {
    let receiver = TestReceiver::bind().unwrap();
    let (p, device, r) = udp_project(&receiver);
    let mut engine = Engine::new(p).unwrap();
    silent(&receiver);
    engine
        .execute(Command::Edit(ProjectEdit::set_value(r.clone(), 0.75)))
        .unwrap();
    assert_eq!(receive(&receiver).args, vec![OscType::Float(0.75)]);
    execute(
        &mut engine,
        device,
        TargetEdit::SetParameterSendMode {
            key: key("level"),
            mode: SendMode::OnRelease,
        },
    );
    silent(&receiver);
    engine.execute(Command::Play).unwrap();
    assert_eq!(receive(&receiver).args, vec![OscType::Float(0.75)]);
    silent(&receiver);
    execute(
        &mut engine,
        device,
        TargetEdit::SetFieldType {
            field: qtable_engine::model::FieldKey("f1".into()),
            entry: key("level"),
            value_type: FieldType::Int,
        },
    );
    assert_eq!(receive(&receiver).args, vec![OscType::Int(1)]);
    // Equal numeric value, changed wire type must still send.
    execute(
        &mut engine,
        device,
        TargetEdit::SetFieldType {
            field: qtable_engine::model::FieldKey("f1".into()),
            entry: key("level"),
            value_type: FieldType::Float,
        },
    );
    assert_eq!(receive(&receiver).args, vec![OscType::Float(1.0)]);
    execute(
        &mut engine,
        device,
        TargetEdit::SetFieldType {
            field: qtable_engine::model::FieldKey("f1".into()),
            entry: key("level"),
            value_type: FieldType::Bool,
        },
    );
    assert_eq!(receive(&receiver).args, vec![OscType::Bool(true)]);
    engine
        .execute(Command::Edit(ProjectEdit::set_value(r.clone(), 0.0)))
        .unwrap();
    assert_eq!(receive(&receiver).args, vec![OscType::Bool(false)]);
    engine
        .execute(Command::Edit(ProjectEdit::SetDevicesActive {
            devices: vec![device],
            active: false,
        }))
        .unwrap();
    engine
        .execute(Command::Edit(ProjectEdit::set_value(r, 1.0)))
        .unwrap();
    silent(&receiver);
    engine
        .execute(Command::Edit(ProjectEdit::SetDevicesActive {
            devices: vec![device],
            active: true,
        }))
        .unwrap();
    assert_eq!(receive(&receiver).args, vec![OscType::Bool(true)]);
    execute(
        &mut engine,
        device,
        TargetEdit::SetEntryPath {
            key: key("level"),
            path: OscAddress::Absolute("/global".into()),
        },
    );
    assert_eq!(receive(&receiver).addr, "/global");
}

#[test]
fn target_every_frame_uses_the_output_frame_rate_and_stops() {
    let receiver = TestReceiver::bind().unwrap();
    let (mut p, device, _) = udp_project(&receiver);
    p.outputs[0].output.as_mut().unwrap().stage.frame_rate = 50.0;
    change(
        &mut p,
        device,
        TargetEdit::SetParameterSendMode {
            key: key("level"),
            mode: SendMode::EveryFrame,
        },
    );
    let mut engine = Engine::new(p).unwrap();
    engine.execute(Command::Play).unwrap();
    let start = receive(&receiver);
    assert_eq!(start.args, vec![OscType::Float(0.0)]);
    let end = Instant::now() + Duration::from_millis(500);
    let mut count = 0;
    while Instant::now() < end {
        if let Ok(messages) =
            receiver.wait_for_messages(1, end.saturating_duration_since(Instant::now()))
        {
            assert_eq!(messages[0].1.args, vec![OscType::Float(0.0)]);
            count += 1;
        }
    }
    assert!((23..=27).contains(&count), "{count} frames");
    engine.execute(Command::Stop).unwrap();
    let _ = receiver.wait_for_messages(1, Duration::from_millis(30));
    silent(&receiver);
}

#[test]
fn target_parameters_resolve_on_clip_tracks_and_keep_device_ownership() {
    let (mut p, _) = fixture();
    for id in [TrackId(1), TrackId(2)] {
        let device = p.next_device_id();
        edit::apply(
            &mut p,
            ProjectEdit::AddDevice {
                track: id,
                kind: NewDevice::Target,
                before: None,
            },
        )
        .unwrap();
        change(
            &mut p,
            device,
            TargetEdit::AddParameter {
                path: relative("level"),
                name: String::new(),
            },
        );
        change(
            &mut p,
            device,
            TargetEdit::SetFieldType {
                entry: key("level"),
                field: FieldKey("f1".into()),
                value_type: FieldType::Int,
            },
        );
        let r = parameter_ref(&p, device);
        assert!(
            p.track(id)
                .unwrap()
                .parameters()
                .any(|candidate| candidate == r)
        );
        assert_eq!(
            p.track(id).unwrap().parameters().last().unwrap(),
            ParameterRef::volume(id)
        );
        assert_eq!(p.parameter(&r).unwrap().fields[0].spec.name, "/level");
        edit::apply(&mut p, ProjectEdit::set_value(r.clone(), 0.7)).unwrap();
        assert_eq!(p.parameter(&r).unwrap().fields[0].value, 1.0);
        let wrong_track = ParameterRef {
            track: TrackId(999),
            ..r.clone()
        };
        assert!(p.parameter(&wrong_track).is_none());
        assert!(p.parameter_recipients(&wrong_track).is_empty());
        // Target settings have their own edits; Mixer output edits cannot mutate a Target.
        let before = p.clone();
        assert!(
            edit::apply(
                &mut p,
                ProjectEdit::SetParameterOutput {
                    target: r,
                    output: ParameterOutput::default()
                }
            )
            .is_err()
        );
        assert_eq!(p, before);
    }
}

#[test]
fn integer_sends_preserve_values_beyond_float_precision() {
    let receiver = TestReceiver::bind().unwrap();
    let (mut p, device, r) = udp_project(&receiver);
    change(
        &mut p,
        device,
        TargetEdit::SetFieldType {
            field: qtable_engine::model::FieldKey("f1".into()),
            entry: key("level"),
            value_type: FieldType::Int,
        },
    );
    change(
        &mut p,
        device,
        TargetEdit::SetFieldRange {
            field: qtable_engine::model::FieldKey("f1".into()),
            entry: key("level"),
            range: Some([f64::from(i32::MIN), f64::from(i32::MAX)]),
        },
    );
    let mut engine = Engine::new(p).unwrap();
    for value in [16_777_217, 16_777_218, i32::MAX, i32::MIN] {
        engine
            .execute(Command::Edit(ProjectEdit::set_value(
                r.clone(),
                f64::from(value),
            )))
            .unwrap();
        assert_eq!(receive(&receiver).args, vec![OscType::Int(value)]);
    }
}
