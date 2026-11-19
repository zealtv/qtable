use qtable_engine::{
    edit::{self, EditReport, ProjectEdit},
    engine::{Command, Engine},
    model::*,
    scheduler::AutomationState,
};
use std::time::{Duration, Instant};

fn field(target: &ParameterRef, key: &str) -> FieldRef {
    FieldRef {
        parameter: target.clone(),
        field: FieldKey(key.into()),
    }
}

fn fixture() -> (Project, ParameterRef, ParameterRef) {
    let mut track = Track::new(TrackId(1), "Target", 0);
    track.devices.push(Device {
        id: DeviceId(1),
        name: "Target".into(),
        active: true,
        expanded: false,
        kind: DeviceKind::Target(TargetDevice {
            parameters: vec![ParameterDef {
                key: EntryKey("xy".into()),
                name: "XY".into(),
                path: OscAddress::Relative("/xy".into()),
                fields: vec![
                    FieldDef::new(FieldKey("x".into()), FieldType::Float, true),
                    FieldDef::new(FieldKey("y".into()), FieldType::Bool, true),
                ],
                send_mode: SendMode::Continuous,
            }],
            ..TargetDevice::default()
        }),
    });
    (
        Project {
            tracks: vec![track, Track::new(TrackId(2), "Mixer", 0)],
            ..Project::default()
        },
        ParameterRef {
            track: TrackId(1),
            device: DeviceRef::Chain(DeviceId(1)),
            key: ParameterKey("xy".into()),
        },
        ParameterRef::volume(TrackId(2)),
    )
}

#[test]
fn invalid_batch_rejects_every_value_without_changing_revision() {
    let (project, target, mixer) = fixture();
    let valid = (target.clone(), vec![(FieldKey("x".into()), 0.25)]);
    let mut engine = Engine::new(project.clone()).unwrap();
    let mut batches = vec![vec![], vec![valid.clone(), (mixer.clone(), vec![])]];
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.1, 1.1] {
        batches.push(vec![
            valid.clone(),
            (mixer.clone(), vec![(FieldKey("value".into()), bad)]),
        ]);
        batches.push(vec![
            (mixer.clone(), vec![(FieldKey("value".into()), 0.4)]),
            (
                target.clone(),
                vec![(FieldKey("x".into()), 0.5), (FieldKey("y".into()), bad)],
            ),
        ]);
    }
    for missing in [
        ParameterRef {
            track: TrackId(999),
            ..mixer.clone()
        },
        ParameterRef {
            device: DeviceRef::Chain(DeviceId(999)),
            ..target.clone()
        },
        ParameterRef {
            key: ParameterKey("missing".into()),
            ..target.clone()
        },
    ] {
        batches.push(vec![
            valid.clone(),
            (missing, vec![(FieldKey("x".into()), 0.5)]),
        ]);
    }
    batches.extend([
        vec![
            valid.clone(),
            (mixer.clone(), vec![(FieldKey("missing".into()), 0.5)]),
        ],
        vec![valid.clone(), valid.clone()],
        vec![(
            target,
            vec![(FieldKey("x".into()), 0.25), (FieldKey("x".into()), 0.5)],
        )],
    ]);
    // Start from a nonzero revision: rejection must leave the counter alone.
    engine
        .execute(Command::Edit(ProjectEdit::SetParameter {
            target: mixer,
            values: vec![(FieldKey("value".into()), 0.4)],
        }))
        .unwrap();
    let before = engine.project().clone();
    for batch in batches {
        // Exercise edit::apply itself, so an engine candidate clone cannot hide partial writes.
        let mut direct = before.clone();
        assert!(edit::apply(&mut direct, ProjectEdit::SetParameters(batch.clone())).is_err());
        assert_eq!(direct, before);
        assert!(
            engine
                .execute(Command::Edit(ProjectEdit::SetParameters(batch)))
                .is_err()
        );
        assert_eq!(engine.project(), &before);
        assert_eq!(engine.project_revision(), 1);
    }
}

#[test]
fn batch_spans_tracks_and_fields_snaps_values_and_preserves_saved_snapshot() {
    let (before, target, mixer) = fixture();
    let batch = vec![
        (target.clone(), vec![(FieldKey("y".into()), 0.8)]),
        (mixer, vec![(FieldKey("value".into()), 0.4)]),
        // Repeating a parameter with disjoint fields is unambiguous.
        (target, vec![(FieldKey("x".into()), 0.75)]),
    ];
    let mut expected = before.clone();
    let definition = expected.tracks[0].devices[0].target_mut().unwrap();
    definition.parameters[0].fields[0].value = Some(0.75);
    definition.parameters[0].fields[1].value = Some(1.0);
    expected.tracks[1].mixer_device.volume.value = 0.4;
    let mut direct = before.clone();
    assert_eq!(
        edit::apply(&mut direct, ProjectEdit::SetParameters(batch.clone())).unwrap(),
        EditReport::default()
    );
    assert_eq!(direct, expected);
    let mut engine = Engine::new(before.clone()).unwrap();
    for revision in 1..=2 {
        assert_eq!(
            engine
                .execute(Command::Edit(ProjectEdit::SetParameters(batch.clone())))
                .unwrap(),
            EditReport::default()
        );
        assert_eq!(engine.project(), &expected);
        assert_eq!(engine.project_revision(), revision);
    }
    engine
        .execute(Command::LoadProject(before.clone()))
        .unwrap();
    assert_eq!(engine.project(), &before);
    assert_eq!(engine.project_revision(), 3);
}

#[test]
fn unchanged_batch_values_override_only_the_named_automation_fields() {
    let (mut project, target, mixer) = fixture();
    project.scenes.push(Scene {
        id: SceneId(1),
        name: "Scene".into(),
        follow_actions: Default::default(),
    });
    project.global_quantisation = GlobalQuantisation::None;
    let named = [field(&target, "x"), field(&mixer, "value")];
    let untouched = field(&ParameterRef::volume(TrackId(1)), "value");
    for track in &mut project.tracks {
        let mut clip = Clip::new(
            ClipId(track.id.0),
            "automation",
            BeatTime(3840),
            LIVE_CLIP_PALETTE[0],
        )
        .unwrap();
        for key in named
            .iter()
            .chain(std::iter::once(&untouched))
            .filter(|key| key.parameter.track == track.id)
        {
            clip.lanes.push(Lane::Envelope(AutomationEnvelope {
                region: None,
                target: ArgumentRef {
                    device: key.parameter.device,
                    parameter: key.parameter.key.clone(),
                    field: key.field.clone(),
                },
                points: vec![Breakpoint {
                    time: BeatTime::ZERO,
                    value: 0.7,
                    curve: 0.0,
                }],
            }));
        }
        track.slots.push(Slot {
            clip: Some(clip),
            ..Slot::default()
        });
    }
    let batch = named
        .iter()
        .map(|key| {
            (
                key.parameter.clone(),
                vec![(key.field.clone(), project.field(key).unwrap().value)],
            )
        })
        .collect();
    let mut engine = Engine::new(project).unwrap();
    engine
        .execute(Command::LaunchScene { scene: SceneId(1) })
        .unwrap();
    let wait = |engine: &Engine, wanted| {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let snapshot = engine.snapshot();
            if named.iter().all(|key| {
                snapshot
                    .automation
                    .iter()
                    .any(|v| &v.field == key && v.state == wanted)
            }) && snapshot
                .automation
                .iter()
                .any(|v| v.field == untouched && v.state == AutomationState::Following)
            {
                break;
            }
            assert!(Instant::now() < deadline, "missing state {wanted:?}");
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    wait(&engine, AutomationState::Following);
    engine
        .execute(Command::Edit(ProjectEdit::SetParameters(batch)))
        .unwrap();
    wait(&engine, AutomationState::Overridden);
}
