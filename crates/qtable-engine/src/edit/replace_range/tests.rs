use super::*;
use crate::edit::{LaneEdit, ProjectEdit, apply as edit};

fn target() -> ArgumentRef {
    ArgumentRef::volume()
}

fn point(time: u64, value: f64) -> Breakpoint {
    Breakpoint {
        time: BeatTime(time),
        value,
        curve: 0.5,
    }
}

fn hit(reference: &EventRef, time: u64, value: i32) -> Event {
    Event {
        position: BeatTime(time),
        definition: reference.clone(),
        arguments: vec![OscArgument::Int(value)],
    }
}

fn setup() -> (Project, EventRef, EventRef) {
    let mut p = Project::default();
    p.scenes.push(Scene {
        id: SceneId(1),
        name: "S".into(),
        follow_actions: Default::default(),
    });
    p.tracks.push(Track::new(TrackId(1), "T", 1));
    let a = p
        .ensure_event(
            TrackId(1),
            OscAddress::Relative("/a".into()),
            &[FieldType::Int],
        )
        .unwrap();
    let b = p
        .ensure_event(
            TrackId(1),
            OscAddress::Absolute("/b".into()),
            &[FieldType::Int],
        )
        .unwrap();
    let mut clip = Clip::new(ClipId(1), "C", BeatTime(1000), LIVE_CLIP_PALETTE[0]).unwrap();
    clip.events = vec![
        hit(&a, 100, 1),
        hit(&b, 150, 9),
        hit(&a, 200, 2),
        hit(&a, 300, 3),
        hit(&a, 400, 4),
    ];
    clip.lanes.push(Lane::Envelope(AutomationEnvelope {
        region: None,
        target: target(),
        points: vec![
            point(0, 0.0),
            point(100, 0.1),
            point(200, 0.2),
            point(300, 0.3),
            point(400, 0.4),
        ],
    }));
    p.tracks[0].slots[0].clip = Some(clip);
    p.validate().unwrap();
    (p, a, b)
}

fn clip(p: &Project) -> &Clip {
    p.tracks[0].slots[0].clip.as_ref().unwrap()
}
fn points(p: &Project) -> &[Breakpoint] {
    &clip(p)
        .lanes
        .iter()
        .find_map(Lane::envelope)
        .unwrap()
        .points
}
fn command(range: Range<u64>, lanes: Vec<(LaneKey, LaneContent)>) -> ProjectEdit {
    command_with_policy(range, lanes, PointReplacement::KeepEdges)
}
fn command_with_policy(
    range: Range<u64>,
    lanes: Vec<(LaneKey, LaneContent)>,
    point_policy: PointReplacement,
) -> ProjectEdit {
    ProjectEdit::EditLane {
        track: TrackId(1),
        slot: 0,
        edit: LaneEdit::ReplaceRange {
            range: BeatTime(range.start)..BeatTime(range.end),
            lanes,
            point_policy,
        },
    }
}
fn hits(reference: &EventRef, values: &[(u64, i32)]) -> (LaneKey, LaneContent) {
    (
        LaneKey::Events(reference.clone()),
        LaneContent::Hits(values.iter().map(|&(t, v)| hit(reference, t, v)).collect()),
    )
}
fn envelope(values: &[(u64, f64)]) -> (LaneKey, LaneContent) {
    (
        LaneKey::Envelope(target()),
        LaneContent::Points(values.iter().map(|&(t, v)| point(t, v)).collect()),
    )
}

#[test]
fn zero_start_clear_keeps_edge_points_but_exact_replacement_includes_start() {
    let (original, a, _) = setup();
    let mut ranged = original.clone();
    edit(&mut ranged, command(0..200, vec![envelope(&[])])).unwrap();
    assert_eq!(
        points(&ranged),
        [
            point(0, 0.0),
            point(200, 0.2),
            point(300, 0.3),
            point(400, 0.4)
        ]
    );

    let mut exact = original;
    edit(
        &mut exact,
        command_with_policy(
            0..200,
            vec![hits(&a, &[]), envelope(&[(0, 0.7), (0, 0.8)])],
            PointReplacement::Exact,
        ),
    )
    .unwrap();
    assert_eq!(
        points(&exact),
        [
            point(0, 0.7),
            point(0, 0.8),
            point(200, 0.2),
            point(300, 0.3),
            point(400, 0.4)
        ]
    );
    assert!(
        !clip(&exact)
            .events
            .iter()
            .any(|h| h.definition == a && h.position < BeatTime(200))
    );
}

#[test]
fn exact_replacement_validates_all_lanes_before_publishing() {
    let (mut p, a, _) = setup();
    let before = p.clone();
    for values in [
        vec![(0, 0.1), (0, 0.2), (0, 0.3)],
        vec![(0, 2.0)],
        vec![(200, 0.5)],
    ] {
        assert!(
            edit(
                &mut p,
                command_with_policy(
                    0..200,
                    vec![hits(&a, &[]), envelope(&values)],
                    PointReplacement::Exact
                )
            )
            .is_err()
        );
        assert_eq!(p, before);
    }
}

#[test]
fn delete_keeps_edge_points_and_other_lanes_without_synthesizing_points() {
    let (mut p, a, b) = setup();
    let other = clip(&p).events[1].clone();
    let old_points = points(&p).to_vec();
    edit(
        &mut p,
        command(100..300, vec![hits(&a, &[]), envelope(&[])]),
    )
    .unwrap();
    assert_eq!(
        clip(&p).events,
        vec![other, hit(&a, 300, 3), hit(&a, 400, 4)]
    );
    assert_eq!(
        points(&p),
        [
            old_points[0].clone(),
            old_points[1].clone(),
            old_points[3].clone(),
            old_points[4].clone()
        ]
    );
    assert_eq!(clip(&p).lane_refs(), vec![a.clone(), b]);
    edit(&mut p, command(250..450, vec![hits(&a, &[])])).unwrap();
    assert!(clip(&p).lane_refs().contains(&a)); // Last hit cleared, lane survives.
    edit(&mut p, command(50..450, vec![envelope(&[])])).unwrap();
    assert_eq!(points(&p), [point(0, 0.0)]); // Neither edge had a point.
}

#[test]
fn paste_and_duplicate_overwrite_and_preserve_equal_time_insertion_order() {
    let (mut p, a, b) = setup();
    edit(
        &mut p,
        command(
            100..300,
            vec![
                hits(&a, &[(150, 7), (150, 8)]),
                envelope(&[(150, 0.7), (250, 0.8)]),
            ],
        ),
    )
    .unwrap();
    assert_eq!(
        clip(&p).events,
        vec![
            hit(&b, 150, 9),
            hit(&a, 150, 7),
            hit(&a, 150, 8),
            hit(&a, 300, 3),
            hit(&a, 400, 4)
        ]
    );
    edit(
        &mut p,
        command(
            300..500,
            vec![
                hits(&a, &[(350, 7), (350, 8)]),
                envelope(&[(350, 0.7), (450, 0.8)]),
            ],
        ),
    )
    .unwrap();
    assert_eq!(
        clip(&p).events,
        vec![
            hit(&b, 150, 9),
            hit(&a, 150, 7),
            hit(&a, 150, 8),
            hit(&a, 350, 7),
            hit(&a, 350, 8)
        ]
    );
    assert_eq!(
        points(&p),
        [
            point(0, 0.0),
            point(100, 0.1),
            point(150, 0.7),
            point(250, 0.8),
            point(300, 0.3),
            point(350, 0.7),
            point(450, 0.8)
        ]
    );
}

#[test]
fn move_writes_the_union_of_source_and_destination_in_one_edit() {
    let (mut p, a, _) = setup();
    // Move 100..300 by 100: preserve the existing edge point at 100.
    edit(
        &mut p,
        command(
            100..400,
            vec![
                hits(&a, &[(200, 1), (300, 2)]),
                envelope(&[(200, 0.1), (300, 0.2)]),
            ],
        ),
    )
    .unwrap();
    assert_eq!(
        clip(&p)
            .events
            .iter()
            .filter(|h| h.definition == a)
            .cloned()
            .collect::<Vec<_>>(),
        vec![hit(&a, 200, 1), hit(&a, 300, 2), hit(&a, 400, 4)]
    );
    assert_eq!(
        points(&p),
        [
            point(0, 0.0),
            point(100, 0.1),
            point(200, 0.1),
            point(300, 0.2),
            point(400, 0.4)
        ]
    );
}

#[test]
fn draw_creates_missing_envelope_atomically_and_start_edge_jumps_keep_order() {
    let (mut p, _, _) = setup();
    p.tracks[0].slots[0].clip.as_mut().unwrap().lanes.clear();
    edit(
        &mut p,
        command(100..300, vec![envelope(&[(100, 0.2), (200, 0.8)])]),
    )
    .unwrap();
    assert_eq!(points(&p), [point(100, 0.2), point(200, 0.8)]);
    edit(&mut p, command(100..300, vec![envelope(&[(100, 0.9)])])).unwrap();
    assert_eq!(points(&p), [point(100, 0.2), point(100, 0.9)]);
}

#[test]
fn invalid_content_rejects_every_lane_and_leaves_no_created_lane() {
    let (mut p, a, b) = setup();
    p.tracks[0].slots[0].clip.as_mut().unwrap().lanes.clear();
    let before = p.clone();
    let mut wrong_type = hit(&a, 150, 1);
    wrong_type.arguments = vec![OscArgument::Float(1.0)];
    let missing_event = EventRef {
        device: DeviceId(999),
        ..a.clone()
    };
    let missing_argument = ArgumentRef {
        field: FieldKey("missing".into()),
        ..target()
    };
    let bad_lanes = vec![
        hits(&a, &[(99, 1)]),
        hits(&a, &[(300, 1)]),
        hits(&a, &[(200, 1), (100, 2)]),
        (
            LaneKey::Events(a.clone()),
            LaneContent::Hits(vec![hit(&b, 150, 1)]),
        ),
        (
            LaneKey::Events(a.clone()),
            LaneContent::Hits(vec![wrong_type]),
        ),
        hits(&missing_event, &[]),
        (
            LaneKey::Envelope(missing_argument),
            LaneContent::Points(vec![]),
        ),
        (LaneKey::Events(a.clone()), LaneContent::Points(vec![])),
        (LaneKey::Envelope(target()), LaneContent::Hits(vec![])),
        envelope(&[(99, 0.5)]),
        envelope(&[(300, 0.5)]),
        envelope(&[(200, 0.5), (100, 0.5)]),
        envelope(&[(150, 2.0)]),
        envelope(&[(150, f64::NAN)]),
        envelope(&[(150, 0.1), (150, 0.2), (150, 0.3)]),
        (
            LaneKey::Envelope(target()),
            LaneContent::Points(vec![Breakpoint {
                curve: f32::INFINITY,
                ..point(150, 0.5)
            }]),
        ),
    ];
    for bad in bad_lanes {
        let mut lanes = vec![hits(&b, &[]), envelope(&[(160, 0.6)])];
        // Keep the malformed lane last and avoid masking its error with a duplicate.
        lanes.retain(|(key, _)| *key != bad.0);
        lanes.push(bad.clone());
        assert!(edit(&mut p, command(100..300, lanes)).is_err(), "{bad:?}");
        assert_eq!(p, before);
    }
    for range in [
        100..100,
        Range {
            start: 300,
            end: 100,
        },
    ] {
        assert!(edit(&mut p, command(range, vec![hits(&a, &[])])).is_err());
        assert_eq!(p, before);
    }
    assert!(
        edit(
            &mut p,
            command(100..300, vec![hits(&a, &[]), hits(&a, &[])])
        )
        .is_err()
    );
    assert_eq!(p, before);
}

#[test]
fn mixed_lane_write_bumps_revision_once_and_rejected_write_does_not() {
    use crate::engine::{Command, Engine};
    let (p, a, _) = setup();
    let mut engine = Engine::new(p).unwrap();
    let revision = engine.project_revision();
    engine
        .execute(Command::Edit(command(
            100..300,
            vec![hits(&a, &[]), envelope(&[(150, 0.5)])],
        )))
        .unwrap();
    assert_eq!(engine.project_revision(), revision + 1);
    let before = engine.project().clone();
    assert!(
        engine
            .execute(Command::Edit(command(
                100..300,
                vec![hits(&a, &[]), envelope(&[(150, 2.0)])]
            )))
            .is_err()
    );
    assert_eq!(engine.project_revision(), revision + 1);
    assert_eq!(engine.project(), &before);
}

#[test]
fn argument_steps_are_validated_and_start_edge_jumps_coalesce() {
    let (mut p, a, _) = setup();
    let device = p.tracks[0].devices[0].target_mut().unwrap();
    let mut field = FieldDef::new(FieldKey("f1".into()), FieldType::Int, true);
    field.range = Some([0.0, 127.0]);
    device.parameters.push(ParameterDef {
        key: EntryKey("steps".into()),
        name: String::new(),
        path: OscAddress::Relative("/steps".into()),
        fields: vec![field],
        send_mode: SendMode::Continuous,
    });
    let stepped = ArgumentRef {
        device: DeviceRef::Chain(a.device),
        parameter: ParameterKey("steps".into()),
        field: FieldKey("f1".into()),
    };
    let before = p.clone();
    assert!(
        edit(
            &mut p,
            command(
                100..300,
                vec![
                    hits(&a, &[]),
                    (
                        LaneKey::Envelope(stepped.clone()),
                        LaneContent::Points(vec![point(150, 1.5)])
                    ),
                ]
            )
        )
        .is_err()
    );
    assert_eq!(p, before);
    edit(
        &mut p,
        command(
            100..300,
            vec![(
                LaneKey::Envelope(stepped),
                LaneContent::Points(vec![point(150, 2.0)]),
            )],
        ),
    )
    .unwrap();
    edit(
        &mut p,
        command(100..300, vec![envelope(&[(100, 0.6), (100, 0.7)])]),
    )
    .unwrap();
    assert_eq!(
        points(&p),
        [
            point(0, 0.0),
            point(100, 0.1),
            point(100, 0.7),
            point(300, 0.3),
            point(400, 0.4)
        ]
    );
}

#[test]
fn start_edge_merge_keeps_first_arriving_and_last_outgoing_point() {
    let (mut p, _, _) = setup();
    let envelope = p.tracks[0].slots[0]
        .clip
        .as_mut()
        .unwrap()
        .lanes
        .iter_mut()
        .find_map(|l| match l {
            Lane::Envelope(e) => Some(e),
            _ => None,
        })
        .unwrap();
    envelope.points.insert(2, point(100, 0.4));
    let mut outgoing = point(100, 0.8);
    outgoing.curve = -0.75;
    edit(
        &mut p,
        command(
            100..300,
            vec![(
                LaneKey::Envelope(target()),
                LaneContent::Points(vec![point(100, 0.6), outgoing.clone()]),
            )],
        ),
    )
    .unwrap();
    assert_eq!(
        points(&p),
        [
            point(0, 0.0),
            point(100, 0.1),
            outgoing.clone(),
            point(300, 0.3),
            point(400, 0.4)
        ]
    );

    // The equal-value pair becomes one point, retaining the incoming segment's curve.
    outgoing.value = 0.1;
    edit(
        &mut p,
        command(
            100..300,
            vec![(
                LaneKey::Envelope(target()),
                LaneContent::Points(vec![outgoing.clone()]),
            )],
        ),
    )
    .unwrap();
    assert_eq!(
        points(&p),
        [point(0, 0.0), outgoing, point(300, 0.3), point(400, 0.4)]
    );
}

#[test]
fn coalescing_cannot_hide_malformed_incoming_points() {
    let (mut p, a, _) = setup();
    let before = p.clone();
    for invalid in [
        point(100, 2.0),
        point(100, f64::NAN),
        Breakpoint {
            curve: f32::INFINITY,
            ..point(100, 0.5)
        },
    ] {
        assert!(
            edit(
                &mut p,
                command(
                    100..300,
                    vec![
                        hits(&a, &[]),
                        (
                            LaneKey::Envelope(target()),
                            LaneContent::Points(vec![invalid, point(100, 0.7)])
                        )
                    ]
                )
            )
            .is_err()
        );
        assert_eq!(p, before);
    }
}

#[test]
fn exact_replacement_keeps_equal_value_siblings_and_their_curves() {
    let (mut p, _, _) = setup();
    let incoming = vec![
        point(0, 0.5),
        Breakpoint {
            curve: -0.5,
            ..point(0, 0.5)
        },
    ];
    edit(
        &mut p,
        command_with_policy(
            0..200,
            vec![(
                LaneKey::Envelope(target()),
                LaneContent::Points(incoming.clone()),
            )],
            PointReplacement::Exact,
        ),
    )
    .unwrap();
    assert_eq!(&points(&p)[..2], &incoming);
}
