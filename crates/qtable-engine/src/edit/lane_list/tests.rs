use crate::{
    edit::{self, Carried, EditReport, EnvelopeEdit, LaneEdit, ProjectEdit},
    model::*,
};

fn clip(p: &Project, track: u64, slot: usize) -> &Clip {
    p.clip(TrackId(track), slot).unwrap()
}
fn run(p: &mut Project, track: u64, slot: usize, edit: LaneEdit) -> Result<EditReport, ModelError> {
    edit::apply(
        p,
        ProjectEdit::EditLane {
            track: TrackId(track),
            slot,
            edit,
        },
    )
}
fn setup() -> Project {
    let mut p = Project::default();
    for at in 0..2 {
        edit::apply(
            &mut p,
            ProjectEdit::AddScene {
                at,
                name: format!("S{at}"),
            },
        )
        .unwrap();
    }
    for id in 1..=2 {
        p.tracks.push(Track::new(TrackId(id), "Track", 2));
        for slot in 0..2 {
            edit::apply(
                &mut p,
                ProjectEdit::CreateClip {
                    track: TrackId(id),
                    slot,
                    length: BeatTime(960),
                },
            )
            .unwrap();
        }
    }
    let mut hits = Vec::new();
    for (time, path, value) in [(0, "/a", 1), (480, "/b", 2), (959, "/a", 3)] {
        hits.push(
            p.hit(
                TrackId(1),
                BeatTime(time),
                OscAddress::Relative(path.into()),
                vec![OscArgument::Int(value)],
            )
            .unwrap(),
        );
    }
    let device = &mut p.tracks[0].devices[0];
    let device_id = device.id;
    let t = device.target_mut().unwrap();
    t.parameters.push(ParameterDef {
        key: EntryKey("level".into()),
        name: "Level".into(),
        path: OscAddress::Relative("/level".into()),
        fields: vec![FieldDef::new(FieldKey("f1".into()), FieldType::Float, true)],
        send_mode: SendMode::Continuous,
    });
    let env = |device, parameter: &str, field: &str| {
        Lane::Envelope(AutomationEnvelope {
            region: None,
            target: ArgumentRef {
                device,
                parameter: ParameterKey(parameter.into()),
                field: FieldKey(field.into()),
            },
            points: [0, 480, 960]
                .into_iter()
                .enumerate()
                .map(|(i, time)| Breakpoint {
                    time: BeatTime(time),
                    value: i as f64 * 0.4,
                    curve: 0.25,
                })
                .collect(),
        })
    };
    let source = p.clip_mut(TrackId(1), 0).unwrap();
    source.lanes = vec![
        Lane::Events(EventLane {
            definition: hits[0].definition.clone(),
        }),
        env(DeviceRef::Mixer, "volume", "value"),
        Lane::Events(EventLane {
            definition: hits[1].definition.clone(),
        }),
        env(DeviceRef::Chain(device_id), "level", "f1"),
    ];
    source.events = hits;
    p.validate().unwrap();
    p
}
fn copy(p: &Project, indices: &[usize], at: usize) -> LaneEdit {
    let source = clip(p, 1, 0);
    let lanes: Vec<_> = indices.iter().map(|i| source.lanes[*i].clone()).collect();
    let hits = source
        .events
        .iter()
        .filter(|e| {
            lanes
                .iter()
                .any(|l| l.event().is_some_and(|l| l.definition == e.definition))
        })
        .cloned()
        .collect();
    LaneEdit::Paste {
        at,
        lanes,
        hits,
        carried: Carried::from_clip(p.track(TrackId(1)).unwrap(), source),
    }
}

#[test]
fn single_and_range_moves_share_track_final_index_semantics_and_keep_content() {
    for (from, to, order) in [
        (0..1, 3, vec![1, 2, 3, 0]),
        (3..4, 0, vec![3, 0, 1, 2]),
        (0..2, 2, vec![2, 3, 0, 1]),
        (1..3, 0, vec![1, 2, 0, 3]),
        (2..4, 0, vec![2, 3, 0, 1]),
        (0..4, 0, vec![0, 1, 2, 3]),
        (1..3, 1, vec![0, 1, 2, 3]),
    ] {
        let mut p = setup();
        let before = clip(&p, 1, 0).clone();
        assert!(
            run(&mut p, 1, 0, LaneEdit::Move { from, to })
                .unwrap()
                .is_empty()
        );
        assert_eq!(clip(&p, 1, 0).events, before.events);
        assert_eq!(
            clip(&p, 1, 0).lanes,
            order
                .iter()
                .map(|i| before.lanes[*i].clone())
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn event_only_and_envelope_only_ranges_move_in_the_same_list() {
    for envelope in [false, true] {
        let mut p = setup();
        p.clip_mut(TrackId(1), 0)
            .unwrap()
            .lanes
            .sort_by_key(|l| l.envelope().is_some());
        let before = clip(&p, 1, 0).lanes.clone();
        let (from, to) = if envelope { (2..4, 0) } else { (0..2, 2) };
        run(&mut p, 1, 0, LaneEdit::Move { from, to }).unwrap();
        assert_eq!(
            clip(&p, 1, 0).lanes,
            [before[2..].to_vec(), before[..2].to_vec()].concat()
        );
    }
}

#[test]
fn remove_a_mixed_set_counts_content_once_and_keeps_definitions() {
    let mut p = setup();
    let before = p.clone();
    let report = run(
        &mut p,
        1,
        0,
        LaneEdit::RemoveMany {
            lanes: vec![3, 0, 0],
        },
    )
    .unwrap();
    assert_eq!(
        (
            report.removed_lanes,
            report.removed_hits,
            report.removed_points,
            report.removed_in_clips
        ),
        (2, 2, 3, 1)
    );
    assert_eq!(
        clip(&p, 1, 0).lanes,
        before.clip(TrackId(1), 0).unwrap().lanes[1..3]
    );
    assert_eq!(p.tracks[0].devices, before.tracks[0].devices);
    assert_eq!(clip(&p, 1, 0).events.len(), 1);
    assert!(
        run(&mut p, 1, 0, LaneEdit::RemoveMany { lanes: vec![] })
            .unwrap()
            .is_empty()
    );
    run(&mut p, 1, 0, LaneEdit::RemoveMany { lanes: vec![0, 1] }).unwrap();
    assert!(clip(&p, 1, 0).lanes.is_empty());
    assert!(clip(&p, 1, 0).events.is_empty());
}

#[test]
fn legacy_edits_route_to_the_same_move_and_remove_rules() {
    let mut p = setup();
    let mut mixed = p.clone();
    let target = clip(&p, 1, 0).lanes[1].envelope().unwrap().target.clone();
    let a = edit::apply(
        &mut p,
        ProjectEdit::EditEnvelope {
            track: TrackId(1),
            slot: 0,
            edit: EnvelopeEdit::Move {
                target: target.clone(),
                to: 3,
            },
        },
    )
    .unwrap();
    let b = run(&mut mixed, 1, 0, LaneEdit::Move { from: 1..2, to: 3 }).unwrap();
    assert_eq!((&p, a), (&mixed, b));
    let a = edit::apply(
        &mut p,
        ProjectEdit::EditEnvelope {
            track: TrackId(1),
            slot: 0,
            edit: EnvelopeEdit::Remove { target },
        },
    )
    .unwrap();
    let b = run(&mut mixed, 1, 0, LaneEdit::RemoveMany { lanes: vec![3] }).unwrap();
    assert_eq!((&p, a), (&mixed, b));
    // Event ordinal 1 is mixed index 1 after those edits, originally index 2.
    let a = run(&mut p, 1, 0, LaneEdit::Remove { lane: 1 }).unwrap();
    let b = run(&mut mixed, 1, 0, LaneEdit::RemoveMany { lanes: vec![1] }).unwrap();
    assert_eq!((&p, a), (&mixed, b));
}

#[test]
fn same_clip_paste_discards_identical_hits_and_preserves_the_whole_clip() {
    let mut p = setup();
    let before = p.clone();
    let edit = copy(&p, &[0, 1, 2, 3], 0);
    let report = run(&mut p, 1, 0, edit).unwrap();
    assert_eq!(p, before);
    assert_eq!(report.removed_hits, 3);
    assert_eq!(
        (
            report.removed_lanes,
            report.removed_points,
            report.adopted_lanes
        ),
        (4, 6, 0)
    );
    assert_eq!(report.removed_in_clips, 1);
}

#[test]
fn another_clip_on_same_track_receives_order_hits_and_points_without_adoption() {
    let mut p = setup();
    let edit = copy(&p, &[3, 0, 1], 0);
    let report = run(&mut p, 1, 1, edit).unwrap();
    let source = clip(&p, 1, 0);
    assert_eq!(
        clip(&p, 1, 1).lanes,
        vec![
            source.lanes[3].clone(),
            source.lanes[0].clone(),
            source.lanes[1].clone()
        ]
    );
    assert_eq!(
        clip(&p, 1, 1).events,
        vec![source.events[0].clone(), source.events[2].clone()]
    );
    assert!(report.is_empty());
}

#[test]
fn cross_track_paste_adopts_both_definitions_and_keeps_mixer_local() {
    let mut p = setup();
    let edit = copy(&p, &[0, 1, 2, 3], 0);
    let report = run(&mut p, 2, 0, edit).unwrap();
    assert_eq!(report.adopted_lanes, 3);
    let dest = clip(&p, 2, 0);
    assert_eq!(dest.lanes.len(), 4);
    assert_eq!(dest.events.len(), 3);
    let track = p.track(TrackId(2)).unwrap();
    assert_eq!(track.devices.len(), 1);
    for i in [0, 2] {
        let definition = &dest.lanes[i].event().unwrap().definition;
        assert_ne!(definition.device, p.tracks[0].devices[0].id);
        assert!(track.event_def(definition).is_some());
    }
    assert_eq!(dest.lanes[1], clip(&p, 1, 0).lanes[1]);
    let env = dest.lanes[3].envelope().unwrap();
    assert!(track.argument_spec(&env.target).is_some());
    assert_eq!(
        env.points,
        clip(&p, 1, 0).lanes[3].envelope().unwrap().points
    );
    let devices = track.devices.clone();
    let edit = copy(&p, &[0, 1, 2, 3], 0);
    let report = run(&mut p, 2, 0, edit).unwrap();
    assert_eq!(p.tracks[1].devices, devices);
    assert_eq!(
        (
            report.adopted_lanes,
            report.removed_lanes,
            report.removed_points
        ),
        (3, 4, 6)
    );
    assert_eq!(clip(&p, 2, 0).events.len(), 3);
    assert_eq!(report.removed_hits, 3);
}

#[test]
fn paste_keeps_hits_and_points_outside_markers() {
    let mut p = setup();
    p.clip_mut(TrackId(1), 1).unwrap().end = BeatTime(480);
    p.clip_mut(TrackId(1), 1).unwrap().loop_end = BeatTime(480);
    let edit = copy(&p, &[0, 1, 2, 3], 0);
    let report = run(&mut p, 1, 1, edit).unwrap();
    assert_eq!(
        (
            report.removed_hits,
            report.removed_points,
            report.removed_in_clips
        ),
        (0, 0, 0)
    );
    assert_eq!(clip(&p, 1, 1).events.len(), 3);
    for e in clip(&p, 1, 1).lanes.iter().filter_map(Lane::envelope) {
        assert_eq!(e.points.len(), 3);
        assert!(e.points.last().unwrap().time > BeatTime(480));
    }
}

#[test]
fn invalid_indices_and_missing_definitions_reject_the_whole_transaction() {
    let p = setup();
    for edit in [
        LaneEdit::Move { from: 0..0, to: 0 },
        LaneEdit::Move {
            from: std::ops::Range { start: 3, end: 2 },
            to: 0,
        },
        LaneEdit::Move { from: 1..5, to: 0 },
        LaneEdit::Move { from: 1..3, to: 3 },
        LaneEdit::Move {
            from: 0..1,
            to: usize::MAX,
        },
        LaneEdit::RemoveMany { lanes: vec![0, 99] },
        LaneEdit::Paste {
            at: 0,
            lanes: vec![],
            hits: vec![],
            carried: vec![],
        },
        copy(&p, &[0], 5),
    ] {
        let mut candidate = p.clone();
        assert!(run(&mut candidate, 1, 0, edit).is_err());
        assert_eq!(candidate, p);
    }
    let mut edit = copy(&p, &[0, 3], 0);
    if let LaneEdit::Paste { carried, .. } = &mut edit {
        carried.clear();
    }
    let mut candidate = p.clone();
    assert!(run(&mut candidate, 2, 0, edit).is_err());
    assert_eq!(candidate, p);
}

#[test]
fn undeclared_event_lanes_are_materialised_after_declared_mixed_lanes() {
    let mut p = setup();
    let before = clip(&p, 1, 0).lanes.clone();
    p.clip_mut(TrackId(1), 0).unwrap().lanes = vec![before[1].clone(), before[3].clone()];
    run(&mut p, 1, 0, LaneEdit::Move { from: 2..4, to: 0 }).unwrap();
    assert_eq!(
        clip(&p, 1, 0).lanes,
        vec![
            before[0].clone(),
            before[2].clone(),
            before[1].clone(),
            before[3].clone()
        ]
    );
}

#[test]
fn format_nine_saves_and_reloads_reordered_removed_and_pasted_lanes() {
    let mut p = setup();
    // Use explicit colours so the loader's fill-unset-colours normalisation is a no-op.
    for t in &mut p.tracks {
        t.colour = LIVE_CLIP_PALETTE[0];
    }
    let edit = copy(&p, &[0, 1, 2, 3], 0);
    run(&mut p, 2, 0, edit).unwrap();
    run(&mut p, 2, 0, LaneEdit::Move { from: 1..3, to: 0 }).unwrap();
    run(&mut p, 2, 0, LaneEdit::RemoveMany { lanes: vec![2] }).unwrap();
    let path =
        std::env::temp_dir().join(format!("qtable-lane-traits-{}.qtable", std::process::id()));
    crate::file::save(&path, &p).unwrap();
    let source = std::fs::read_to_string(&path).unwrap();
    assert!(source.contains("version: 12"));
    let loaded = crate::file::load(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(loaded, p);
}

#[test]
fn paste_inserts_only_new_lanes_at_the_requested_index_and_keeps_equal_time_hit_order() {
    let mut p = setup();
    // Seed the destination with /b, then insert a new envelope and /a before it.
    let edit = copy(&p, &[2], 0);
    run(&mut p, 1, 1, edit).unwrap();
    let edit = copy(&p, &[1, 0], 0);
    run(&mut p, 1, 1, edit).unwrap();
    let source = clip(&p, 1, 0);
    assert_eq!(
        clip(&p, 1, 1).lanes,
        vec![
            source.lanes[1].clone(),
            source.lanes[0].clone(),
            source.lanes[2].clone()
        ]
    );
    // Existing equal-time hits precede copied hits, even when lanes insert first.
    let mut edit = copy(&p, &[0], 0);
    if let LaneEdit::Paste { hits, .. } = &mut edit {
        hits[0].position = BeatTime(480);
        hits[0].arguments = vec![OscArgument::Int(99)];
    }
    run(&mut p, 1, 1, edit).unwrap();
    let at_time: Vec<_> = clip(&p, 1, 1)
        .events
        .iter()
        .filter(|e| e.position == BeatTime(480))
        .map(|e| e.arguments.clone())
        .collect();
    assert_eq!(
        at_time,
        vec![vec![OscArgument::Int(2)], vec![OscArgument::Int(99)]]
    );
}

#[test]
fn invalid_paste_rolls_back_definitions_adopted_before_validation() {
    let p = setup();
    let mut edit = copy(&p, &[0, 3], 0);
    if let LaneEdit::Paste { lanes, .. } = &mut edit
        && let Lane::Envelope(e) = &mut lanes[1]
    {
        e.points[0].curve = f32::NAN;
    }
    let mut candidate = p.clone();
    assert!(run(&mut candidate, 2, 0, edit).is_err());
    assert_eq!(candidate, p);
    let mut edit = copy(&p, &[1], 0);
    if let LaneEdit::Paste { hits, .. } = &mut edit {
        hits.push(clip(&p, 1, 0).events[0].clone());
    }
    assert!(run(&mut candidate, 2, 0, edit).is_err());
    assert_eq!(candidate, p);
}

#[test]
fn adopted_lanes_that_converge_use_the_same_resolution_as_clip_paste() {
    let mut p = setup();
    let mut copied = clip(&p, 1, 0).clone();
    let mut carried = Carried::from_clip(&p.tracks[0], &copied);
    // Two distinct source events and parameters happen to have identical destination
    // definitions (e.g. copies from another set); first envelope wins after adoption.
    if let Carried::Event { definition, .. } = &mut carried[1] {
        definition.path = OscAddress::Relative("/a".into());
    }
    copied.events[1].position = copied.events[0].position;
    copied.events[1].arguments = copied.events[0].arguments.clone();
    let mut second = copied.lanes[3].envelope().unwrap().clone();
    second.target.parameter = ParameterKey("other".into());
    second.points[0].value = 0.1;
    let parameter = carried
        .iter()
        .find_map(|c| match c {
            Carried::Parameter { definition, .. } => Some(definition.clone()),
            _ => None,
        })
        .unwrap();
    carried.push(Carried::Parameter {
        from: second.target.clone(),
        definition: parameter,
    });
    copied.lanes.push(Lane::Envelope(second));
    let report = run(
        &mut p,
        2,
        0,
        LaneEdit::Paste {
            at: 0,
            lanes: copied.lanes.clone(),
            hits: copied.events.clone(),
            carried: carried.clone(),
        },
    )
    .unwrap();
    assert_eq!(
        (
            report.removed_lanes,
            report.removed_points,
            report.adopted_lanes
        ),
        (2, 3, 4)
    );
    assert_eq!(clip(&p, 2, 0).lanes.len(), 3);
    assert_eq!(clip(&p, 2, 0).events.len(), 2);
    assert_eq!((report.removed_hits, report.removed_in_clips), (1, 1));
    let mut whole = setup();
    let clip_report = edit::apply(
        &mut whole,
        ProjectEdit::PasteClip {
            track: TrackId(2),
            slot: 0,
            clip: Box::new(copied),
            carried,
        },
    )
    .unwrap();
    assert_eq!(clip_report, report);
    assert_eq!(clip(&whole, 2, 0).lanes, clip(&p, 2, 0).lanes);
    assert_eq!(clip(&whole, 2, 0).events, clip(&p, 2, 0).events);
}

#[test]
fn coalesced_destination_lane_discards_only_identical_copied_hits() {
    let mut p = setup();
    // Pre-existing identical hits remain; only copied duplicates are discarded.
    let repeated = clip(&p, 1, 0).events[0].clone();
    p.clip_mut(TrackId(1), 0)
        .unwrap()
        .events
        .insert(1, repeated);
    let before = clip(&p, 1, 0).events.clone();
    let mut edit = copy(&p, &[0], 0);
    if let LaneEdit::Paste { hits, .. } = &mut edit {
        let mut changed_value = hits[0].clone();
        changed_value.arguments = vec![OscArgument::Int(99)];
        let mut changed_time = hits[0].clone();
        changed_time.position = BeatTime(240);
        hits.extend([changed_value.clone(), changed_value, changed_time]);
    }
    let report = run(&mut p, 1, 0, edit).unwrap();
    assert_eq!(
        (
            report.removed_lanes,
            report.removed_hits,
            report.removed_in_clips
        ),
        (1, 4, 1)
    );
    let mut expected = before;
    let mut changed_value = expected[0].clone();
    changed_value.arguments = vec![OscArgument::Int(99)];
    let mut changed_time = expected[0].clone();
    changed_time.position = BeatTime(240);
    expected.extend([changed_value, changed_time]);
    expected.sort_by_key(|e| e.position);
    assert_eq!(clip(&p, 1, 0).events, expected);
}

#[test]
fn hits_in_distinct_lanes_and_deliberate_repeats_in_a_new_lane_remain() {
    let mut p = setup();
    let mut edit = copy(&p, &[0, 2], 0);
    if let LaneEdit::Paste { hits, .. } = &mut edit {
        // Same position and values on different event definitions are distinct.
        hits[1].position = hits[0].position;
        hits[1].arguments = hits[0].arguments.clone();
        // A single new lane with deliberate repeated hits has not coalesced.
        hits.push(hits[0].clone());
    }
    let report = run(&mut p, 1, 1, edit).unwrap();
    assert!(report.is_empty());
    assert_eq!(clip(&p, 1, 1).events.len(), 4);
}
