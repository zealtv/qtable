use qtable_engine::{
    edit::{self, Carried, EnvelopeEdit, LaneEdit, ProjectEdit, TargetEdit},
    model::*,
};

fn setup() -> (Project, ArgumentRef) {
    let mut p = Project::default();
    p.scenes.push(Scene {
        id: SceneId(1),
        name: "Scene".into(),
        follow_actions: Default::default(),
    });
    p.tracks.push(Track::new(TrackId(1), "Source", 1));
    p.tracks.push(Track::new(TrackId(2), "Destination", 1));
    let device = p.tracks[0].first_target_mut_or_insert(|| DeviceId(1));
    let t = device.target_mut().unwrap();
    let mut field = FieldDef::new(FieldKey("x".into()), FieldType::Float, true);
    field.range = Some([0.0, 100.0]);
    t.parameters.push(ParameterDef {
        key: EntryKey("position".into()),
        name: String::new(),
        path: OscAddress::Relative("/position".into()),
        fields: vec![field],
        send_mode: SendMode::Continuous,
    });
    let target = ArgumentRef {
        device: DeviceRef::Chain(DeviceId(1)),
        parameter: ParameterKey("position".into()),
        field: FieldKey("x".into()),
    };
    p.tracks[0].slots[0].clip =
        Some(Clip::new(ClipId(1), "Clip", BeatTime(960), LIVE_CLIP_PALETTE[0]).unwrap());
    envelope(
        &mut p,
        EnvelopeEdit::Add {
            target: target.clone(),
        },
    )
    .unwrap();
    envelope(
        &mut p,
        EnvelopeEdit::SetPoints {
            target: target.clone(),
            points: points(&[20.0, 80.0]),
        },
    )
    .unwrap();
    (p, target)
}
fn points(values: &[f64]) -> Vec<Breakpoint> {
    values
        .iter()
        .enumerate()
        .map(|(i, v)| Breakpoint {
            time: BeatTime(i as u64 * 480),
            value: *v,
            curve: 0.5,
        })
        .collect()
}
fn envelope(p: &mut Project, edit: EnvelopeEdit) -> Result<edit::EditReport, ModelError> {
    edit::apply(
        p,
        ProjectEdit::EditEnvelope {
            track: TrackId(1),
            slot: 0,
            edit,
        },
    )
}
fn env(p: &Project) -> &AutomationEnvelope {
    p.tracks[0].slots[0]
        .clip
        .as_ref()
        .unwrap()
        .lanes
        .iter()
        .find_map(Lane::envelope)
        .unwrap()
}
fn definition(p: &mut Project, edit: TargetEdit) -> edit::EditReport {
    edit::apply(
        p,
        ProjectEdit::EditTarget {
            device: DeviceId(1),
            edit,
        },
    )
    .unwrap()
}

#[test]
fn envelope_edits_are_transactional_and_shortening_keeps_points() {
    let (mut p, target) = setup();
    for edit in [
        EnvelopeEdit::Add {
            target: target.clone(),
        },
        EnvelopeEdit::SetPoints {
            target: target.clone(),
            points: points(&[101.0]),
        },
    ] {
        let before = p.clone();
        assert!(envelope(&mut p, edit).is_err());
        assert_eq!(p, before);
    }
    envelope(
        &mut p,
        EnvelopeEdit::SetPoints {
            target: target.clone(),
            points: points(&[0.0, 1.0, 2.0, 3.0]),
        },
    )
    .unwrap();
    let before = p.clone();
    edit::apply(
        &mut p,
        ProjectEdit::SetClipLength {
            track: TrackId(1),
            slot: 0,
            length: BeatTime(240),
        },
    )
    .unwrap();
    assert_eq!(env(&p).points, env(&before).points);
    envelope(&mut p, EnvelopeEdit::Remove { target }).unwrap();
    assert!(p.tracks[0].slots[0].clip.as_ref().unwrap().lanes.is_empty());
}

#[test]
fn definition_changes_clamp_and_threshold_at_the_old_midpoint() {
    let (mut p, _) = setup();
    let report = definition(
        &mut p,
        TargetEdit::SetFieldControl {
            entry: EntryKey("position".into()),
            field: FieldKey("x".into()),
            control: Control::Toggle,
        },
    );
    assert_eq!(
        env(&p)
            .points
            .iter()
            .map(|p| (p.value, p.curve))
            .collect::<Vec<_>>(),
        [(0.0, 0.0), (1.0, 0.0)]
    );
    assert_eq!(report.converted_points, 2);
    let (mut p, _) = setup();
    let report = definition(
        &mut p,
        TargetEdit::SetFieldRange {
            entry: EntryKey("position".into()),
            field: FieldKey("x".into()),
            range: Some([30.0, 60.0]),
        },
    );
    assert_eq!(
        env(&p).points.iter().map(|p| p.value).collect::<Vec<_>>(),
        [30.0, 60.0]
    );
    assert_eq!(report.converted_points, 2);
}

#[test]
fn cross_track_copy_adopts_parameter_and_mixer_without_duplicate_definitions() {
    let (mut p, _) = setup();
    let mixer = ArgumentRef::volume();
    envelope(
        &mut p,
        EnvelopeEdit::Add {
            target: mixer.clone(),
        },
    )
    .unwrap();
    envelope(
        &mut p,
        EnvelopeEdit::SetPoints {
            target: mixer,
            points: points(&[0.2, 0.8]),
        },
    )
    .unwrap();
    let clip = p.tracks[0].slots[0].clip.clone().unwrap();
    let carried = Carried::from_clip(&p.tracks[0], &clip);
    for _ in 0..2 {
        edit::apply(
            &mut p,
            ProjectEdit::PasteClip {
                track: TrackId(2),
                slot: 0,
                clip: Box::new(clip.clone()),
                carried: carried.clone(),
            },
        )
        .unwrap();
    }
    let dest = &p.tracks[1];
    assert_eq!(dest.targets().flat_map(|(_, t)| &t.parameters).count(), 1);
    let lanes = &dest.slots[0].clip.as_ref().unwrap().lanes;
    assert_eq!(lanes.len(), 2);
    assert_ne!(
        lanes[0].envelope().unwrap().target.device,
        env(&p).target.device
    );
    assert_eq!(lanes[0].envelope().unwrap().points, env(&p).points);
    assert_eq!(lanes[1].envelope().unwrap().target.device, DeviceRef::Mixer);
}

#[test]
fn removing_an_event_preserves_mixed_lane_order() {
    let (mut p, _) = setup();
    for path in ["/a", "/b"] {
        edit::apply(
            &mut p,
            ProjectEdit::EditLane {
                track: TrackId(1),
                slot: 0,
                edit: LaneEdit::AddPath(path.into()),
            },
        )
        .unwrap();
    }
    p.tracks[0].slots[0].clip.as_mut().unwrap().lanes.swap(0, 1);
    let before = p.tracks[0].slots[0].clip.as_ref().unwrap().lanes.clone();
    edit::apply(
        &mut p,
        ProjectEdit::EditLane {
            track: TrackId(1),
            slot: 0,
            edit: LaneEdit::Remove { lane: 0 },
        },
    )
    .unwrap();
    assert_eq!(
        p.tracks[0].slots[0].clip.as_ref().unwrap().lanes,
        before[1..]
    );
}

#[test]
fn definition_removal_counts_a_mixed_clip_once() {
    let (mut p, target) = setup();
    edit::apply(
        &mut p,
        ProjectEdit::EditLane {
            track: TrackId(1),
            slot: 0,
            edit: LaneEdit::AddPath("/hit".into()),
        },
    )
    .unwrap();
    assert_eq!(p.tracks[0].field_uses(&target), (1, 2));
    let report = edit::apply(
        &mut p,
        ProjectEdit::RemoveDevices {
            devices: vec![DeviceId(1)],
        },
    )
    .unwrap();
    assert_eq!((report.removed_lanes, report.removed_in_clips), (2, 1));
    assert!(p.tracks[0].slots[0].clip.as_ref().unwrap().lanes.is_empty());
}

#[test]
fn removing_argument_or_parameter_removes_envelopes() {
    for whole in [false, true] {
        let (mut p, _) = setup();
        definition(
            &mut p,
            TargetEdit::AddField {
                entry: EntryKey("position".into()),
                value_type: FieldType::Float,
            },
        );
        let edit = if whole {
            TargetEdit::RemoveEntries {
                keys: vec![EntryKey("position".into())],
            }
        } else {
            TargetEdit::RemoveField {
                entry: EntryKey("position".into()),
                field: FieldKey("x".into()),
            }
        };
        let report = definition(&mut p, edit);
        assert_eq!((report.removed_lanes, report.removed_in_clips), (1, 1));
    }
}

#[test]
fn enum_options_follow_wire_values_and_removed_options_use_default() {
    let (mut p, target) = setup();
    definition(
        &mut p,
        TargetEdit::SetFieldControl {
            entry: EntryKey("position".into()),
            field: FieldKey("x".into()),
            control: Control::Enum,
        },
    );
    envelope(
        &mut p,
        EnvelopeEdit::SetPoints {
            target,
            points: points(&[0.0, 1.0]),
        },
    )
    .unwrap();
    let options = [2.0, 3.0, 1.0]
        .into_iter()
        .map(|v| EnumOption {
            label: v.to_string(),
            value: OscArgument::Float(v),
        })
        .collect();
    definition(
        &mut p,
        TargetEdit::SetFieldOptions {
            entry: EntryKey("position".into()),
            field: FieldKey("x".into()),
            options,
        },
    );
    assert_eq!(
        env(&p).points.iter().map(|p| p.value).collect::<Vec<_>>(),
        [2.0, 0.0]
    );
    let options = [2.0, 3.0]
        .into_iter()
        .map(|v| EnumOption {
            label: v.to_string(),
            value: OscArgument::Float(v),
        })
        .collect();
    definition(
        &mut p,
        TargetEdit::SetFieldOptions {
            entry: EntryKey("position".into()),
            field: FieldKey("x".into()),
            options,
        },
    );
    assert_eq!(
        env(&p).points.iter().map(|p| p.value).collect::<Vec<_>>(),
        [0.0, 0.0]
    );
}

#[test]
fn cross_track_matching_conforms_and_move_renews_target() {
    let (mut p, target) = setup();
    let device = p.tracks[1].first_target_mut_or_insert(|| DeviceId(2));
    let t = device.target_mut().unwrap();
    let mut field = FieldDef::new(target.field.clone(), FieldType::Int, true);
    field.range = Some([0.0, 40.0]);
    t.parameters.push(ParameterDef {
        key: EntryKey("destination".into()),
        name: String::new(),
        path: OscAddress::Relative("/position".into()),
        fields: vec![field],
        send_mode: SendMode::Continuous,
    });
    let report = edit::apply(
        &mut p,
        ProjectEdit::MoveClip {
            from: (TrackId(1), 0),
            to: (TrackId(2), 0),
        },
    )
    .unwrap();
    assert_eq!(report.converted_points, 2);
    assert!(p.tracks[0].slots[0].clip.is_none());
    let e = p.tracks[1].slots[0].clip.as_ref().unwrap().lanes[0]
        .envelope()
        .unwrap();
    assert_eq!(e.target.parameter.0, "destination");
    assert_eq!(
        e.points
            .iter()
            .map(|p| (p.value, p.curve))
            .collect::<Vec<_>>(),
        [(20.0, 0.0), (40.0, 0.0)]
    );
}

#[test]
fn moving_an_envelope_uses_mixed_lane_indices_and_rejects_invalid_indices() {
    let (mut p, target) = setup();
    edit::apply(
        &mut p,
        ProjectEdit::EditLane {
            track: TrackId(1),
            slot: 0,
            edit: LaneEdit::AddPath("/hit".into()),
        },
    )
    .unwrap();
    envelope(
        &mut p,
        EnvelopeEdit::Move {
            target: target.clone(),
            to: 1,
        },
    )
    .unwrap();
    assert!(
        p.tracks[0].slots[0].clip.as_ref().unwrap().lanes[1]
            .envelope()
            .is_some()
    );
    let before = p.clone();
    assert!(envelope(&mut p, EnvelopeEdit::Move { target, to: 2 }).is_err());
    assert_eq!(p, before);
}

#[test]
fn malformed_carried_parameter_is_rejected_atomically() {
    let (mut p, _) = setup();
    let clip = p.tracks[0].slots[0].clip.clone().unwrap();
    let mut carried = Carried::from_clip(&p.tracks[0], &clip);
    let Carried::Parameter { definition, .. } = &mut carried[0] else {
        unreachable!()
    };
    definition.fields.clear();
    let before = p.clone();
    assert!(
        edit::apply(
            &mut p,
            ProjectEdit::PasteClip {
                track: TrackId(2),
                slot: 0,
                clip: Box::new(clip),
                carried
            }
        )
        .is_err()
    );
    assert_eq!(p, before);
}
