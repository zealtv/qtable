use super::*;
use crate::edit::{ProjectEdit, apply};

#[test]
fn resolver_returns_the_spec_and_state_for_the_same_key() {
    let mut project = Project::default();
    let target = ParameterRef::volume(project.main.id);
    project.parameter_mut(&target).unwrap().value = 0.375;
    let (spec, stored) = project.resolve_parameter(&target).unwrap();
    let view = project.parameter(&target).unwrap();
    assert_eq!(spec, &VOLUME);
    assert_eq!(&*view.fields[0].spec, spec);
    assert_eq!(view.fields[0].value, stored.value);
    assert!(std::ptr::eq(&*view.output, &stored.output));
    assert_eq!(spec.parse("0.375").unwrap(), stored.value);
    let mut unknown = target;
    unknown.key = ParameterKey("unknown".into());
    assert!(project.resolve_parameter(&unknown).is_none());
    assert!(
        project
            .resolve_parameter(&ParameterRef::volume(TrackId(999)))
            .is_none()
    );
}

#[test]
fn every_track_kind_exposes_and_edits_its_device_parameters() {
    let mut track = Track::new(TrackId(1), "Track", 0);
    track.group = Some(TrackId(2));
    track.route = Route::Group;
    let mut project = Project {
        tracks: vec![Track::new_group(TrackId(2), "Group", 0), track],
        outputs: vec![Track::new_output(TrackId(3), "Output", "127.0.0.1", 9000)],
        ..Project::default()
    };
    for id in [TrackId(1), TrackId(2), TrackId(3), project.main.id] {
        let target = ParameterRef::volume(id);
        assert_eq!(
            project.track(id).unwrap().parameters().collect::<Vec<_>>(),
            vec![target.clone()]
        );
        let view = project.parameter(&target).unwrap();
        assert_eq!(&*view.fields[0].spec, &VOLUME);
        assert_eq!(view.fields[0].value, VOLUME.default);
        assert!(!view.enabled);
        apply(&mut project, ProjectEdit::set_value(target.clone(), 0.25)).unwrap();
        let output = ParameterOutput {
            enabled: true,
            address: "gain".into(),
            ..ParameterOutput::default()
        };
        apply(
            &mut project,
            ProjectEdit::SetParameterOutput {
                target: target.clone(),
                output,
            },
        )
        .unwrap();
        let view = project.parameter(&target).unwrap();
        assert_eq!(view.fields[0].value, 0.25);
        assert!(view.enabled);
        assert_eq!(view.output.address, "/gain");
        assert_eq!(ron::to_string(&target.key).unwrap(), "\"volume\"");
        assert_eq!(
            ron::from_str::<ParameterRef>(&ron::to_string(&target).unwrap()).unwrap(),
            target
        );
    }
}

#[test]
fn missing_targets_do_not_edit_the_project() {
    let mut project = Project::default();
    let original = project.clone();
    let mut unknown_key = ParameterRef::volume(project.main.id);
    unknown_key.key = ParameterKey("unknown".into());
    for target in [unknown_key, ParameterRef::volume(TrackId(999))] {
        assert!(project.parameter(&target).is_none());
        assert!(project.parameter_recipients(&target).is_empty());
        assert!(
            project
                .parameter_address(&target, project.main.id)
                .is_none()
        );
        assert!(apply(&mut project, ProjectEdit::set_value(target.clone(), 0.5)).is_err());
        assert!(
            apply(
                &mut project,
                ProjectEdit::SetParameterOutput {
                    target,
                    output: ParameterOutput::default()
                }
            )
            .is_err()
        );
        assert_eq!(project, original);
    }
}

#[test]
fn a_stale_position_still_finds_the_parameter_by_key() {
    let mut track = Track::new(TrackId(1), "Track", 0);
    let device = track.first_target_mut_or_insert(|| DeviceId(1));
    let target = device.target_mut().unwrap();
    for key in ["a", "b"] {
        target.parameters.push(ParameterDef {
            key: EntryKey(key.into()),
            name: String::new(),
            path: OscAddress::Relative(format!("/{key}")),
            fields: vec![FieldDef::new(
                FieldKey("value".into()),
                FieldType::Float,
                true,
            )],
            send_mode: SendMode::Continuous,
        });
    }
    let project = Project {
        tracks: vec![track],
        ..Project::default()
    };
    let b = ParameterRef {
        track: TrackId(1),
        device: DeviceRef::Chain(DeviceId(1)),
        key: ParameterKey("b".into()),
    };
    assert_eq!(project.parameter_position(&b), Some(1));
    for position in [Some(1), Some(0), Some(9), None] {
        let view = project.parameter_near(&b, position).unwrap();
        assert_eq!(view.output.address, "/b");
    }
    assert_eq!(
        project.parameter_position(&ParameterRef::volume(TrackId(1))),
        None
    );
}

#[test]
fn field_existence_matches_resolution_for_mixer_and_multi_argument_targets() {
    let mut track = Track::new(TrackId(1), "Track", 0);
    let device = track.first_target_mut_or_insert(|| DeviceId(1));
    let target = device.target_mut().unwrap();
    target.parameters.push(ParameterDef {
        key: EntryKey("position".into()),
        name: String::new(),
        path: OscAddress::Relative("/position".into()),
        fields: ["x", "y"]
            .map(|key| FieldDef::new(FieldKey(key.into()), FieldType::Float, true))
            .to_vec(),
        send_mode: SendMode::Continuous,
    });
    let project = Project {
        tracks: vec![track, Track::new_group(TrackId(2), "Group", 0)],
        outputs: vec![Track::new_output(TrackId(3), "Output", "127.0.0.1", 9000)],
        ..Project::default()
    };
    for track in [
        TrackId(1),
        TrackId(2),
        TrackId(3),
        project.main.id,
        TrackId(999),
    ] {
        for device in [
            DeviceRef::Mixer,
            DeviceRef::Chain(DeviceId(1)),
            DeviceRef::Chain(DeviceId(999)),
        ] {
            for parameter in ["volume", "position", "missing"] {
                for field in ["value", "x", "y", "missing"] {
                    let field = FieldRef {
                        parameter: ParameterRef {
                            track,
                            device,
                            key: ParameterKey(parameter.into()),
                        },
                        field: FieldKey(field.into()),
                    };
                    assert_eq!(
                        project.contains_field(&field),
                        project.field(&field).is_some(),
                        "{field:?}"
                    );
                }
            }
        }
    }
}
