//! Edits that remove or convert more than they name say so, through the command layer, so every
//! client can report it.
use qtable_engine::edit::{self, Carried, EditReport, LaneEdit, ProjectEdit, TargetEdit};
use qtable_engine::engine::{Command, Engine};
use qtable_engine::file;
use qtable_engine::model::*;

/// Track 1 holds Target `DeviceId(1)`: `kick (i)`, `kick-2` (absolute), `kick-3 (f)`, `snare`
/// and `empty`. Clip A (slot 0) has the lanes empty, kick, kick-3, kick-2 and snare, and hits on
/// all but empty; clip B (slot 1) has one `kick` hit, `Int(2)`.
fn fixture() -> Project {
    file::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/v6-lanes.ron"
    ))
    .unwrap()
}

const DEVICE: DeviceId = DeviceId(1);
const TRACK: TrackId = TrackId(1);

fn key(text: &str) -> EntryKey {
    EntryKey(text.into())
}
fn event(text: &str) -> EventRef {
    EventRef {
        device: DEVICE,
        key: key(text),
    }
}
fn target(edit: TargetEdit) -> ProjectEdit {
    ProjectEdit::EditTarget {
        device: DEVICE,
        edit,
    }
}
fn set_path(slot: usize, lane: usize, path: &str) -> ProjectEdit {
    ProjectEdit::EditLane {
        track: TRACK,
        slot,
        edit: LaneEdit::SetPath {
            lane,
            path: path.into(),
        },
    }
}
fn hits(p: &Project, slot: usize, reference: &EventRef) -> Vec<Vec<OscArgument>> {
    p[TRACK].slots[slot]
        .clip
        .as_ref()
        .unwrap()
        .events
        .iter()
        .filter(|e| &e.definition == reference)
        .map(|e| e.arguments.clone())
        .collect()
}
fn converted(hits: usize, signature: &[FieldType]) -> EditReport {
    EditReport {
        converted_hits: hits,
        signature: Some(signature.to_vec()),
        ..EditReport::default()
    }
}

#[test]
fn a_lane_repointed_onto_another_signature_reports_the_hits_it_converted() {
    // Repro: a re-point converts every hit on the lane, and before this nothing
    // told the user. Clip A's `kick-3` lane holds `Float(0.5)`; `/tom` takes one Int.
    let mut engine = Engine::new(fixture()).unwrap();
    let mut run = |edit| engine.execute(Command::Edit(edit)).unwrap();
    let added = run(target(TargetEdit::AddEvent {
        path: OscAddress::Relative("/tom".into()),
        name: String::new(),
    }));
    assert!(added.is_empty(), "{added:?}");
    run(target(TargetEdit::AddField {
        entry: key("tom"),
        value_type: FieldType::Int,
    }));
    let report = run(set_path(0, 2, "/tom"));
    assert_eq!(report, converted(1, &[FieldType::Int]));
    assert_eq!(
        hits(engine.project(), 0, &event("tom")),
        [vec![OscArgument::Int(1)]]
    );
    let tempo = engine.execute(Command::SetTempo(120.0)).unwrap();
    assert!(tempo.is_empty());
}

#[test]
fn a_rename_or_a_repoint_to_the_same_signature_reports_nothing() {
    let mut p = fixture();
    // `kick-3` has no other user and `/hat` is free: rename in place.
    let report = edit::apply(&mut p, set_path(0, 2, "/hat")).unwrap();
    assert!(report.is_empty(), "{report:?}");
    // Clip B shares `kick`: re-point to a new event with the same signature.
    let report = edit::apply(&mut p, set_path(1, 0, "/new")).unwrap();
    assert!(report.is_empty(), "{report:?}");
    assert_eq!(hits(&p, 1, &event("new")), [vec![OscArgument::Int(2)]]);
}

#[test]
fn removing_entries_reports_the_lanes_and_hits_that_went() {
    let mut p = fixture();
    let report = edit::apply(
        &mut p,
        target(TargetEdit::RemoveEntries {
            keys: vec![key("kick"), key("kick-3"), key("empty")],
        }),
    )
    .unwrap();
    // Clip A loses three lanes (one of them empty) and two hits; clip B its lane and hit.
    let expected = EditReport {
        removed_lanes: 4,
        removed_hits: 3,
        removed_in_clips: 2,
        ..EditReport::default()
    };
    assert_eq!(report, expected);
}

#[test]
fn removing_a_device_reports_every_lane_and_hit_it_played() {
    let mut p = fixture();
    let report = edit::apply(
        &mut p,
        ProjectEdit::RemoveDevices {
            devices: vec![DEVICE],
        },
    )
    .unwrap();
    let expected = EditReport {
        removed_lanes: 6,
        removed_hits: 5,
        removed_in_clips: 2,
        ..EditReport::default()
    };
    assert_eq!(report, expected);
}

#[test]
fn signature_edits_report_retyped_and_dropped_values_but_not_appended_defaults() {
    let mut p = fixture();
    let mut run = |edit| edit::apply(&mut p, target(edit)).unwrap();
    let appended = run(TargetEdit::AddField {
        entry: key("kick"),
        value_type: FieldType::Float,
    });
    assert!(appended.is_empty(), "{appended:?}");
    let retyped = run(TargetEdit::SetFieldType {
        entry: key("kick"),
        field: qtable_engine::model::FieldKey("f1".into()),
        value_type: FieldType::Float,
    });
    assert_eq!(retyped, converted(2, &[FieldType::Float, FieldType::Float]));
    let dropped = run(TargetEdit::RemoveField {
        entry: key("kick"),
        field: qtable_engine::model::FieldKey("f2".into()),
    });
    assert_eq!(dropped, converted(2, &[FieldType::Float]));
    // `empty` has a lane but no hits.
    run(TargetEdit::AddField {
        entry: key("empty"),
        value_type: FieldType::Int,
    });
    let unplayed = run(TargetEdit::SetFieldType {
        entry: key("empty"),
        field: qtable_engine::model::FieldKey("f1".into()),
        value_type: FieldType::String,
    });
    assert!(unplayed.is_empty(), "{unplayed:?}");
}

#[test]
fn a_paste_reports_the_hits_it_conformed_to_the_events_current_signature() {
    let mut p = fixture();
    let clip = p[TRACK].slots[1].clip.clone().unwrap();
    let carried = Carried::from_clip(&p[TRACK], &clip);
    let mut run = |edit| edit::apply(&mut p, edit).unwrap();
    run(target(TargetEdit::SetFieldType {
        entry: key("kick"),
        field: qtable_engine::model::FieldKey("f1".into()),
        value_type: FieldType::Float,
    }));
    run(ProjectEdit::AddScene {
        at: 2,
        name: "Three".into(),
    });
    let report = run(ProjectEdit::PasteClip {
        track: TRACK,
        slot: 2,
        clip: Box::new(clip),
        carried,
    });
    assert_eq!(report, converted(1, &[FieldType::Float]));
    let moved = run(ProjectEdit::MoveClip {
        from: (TRACK, 2),
        to: (TrackId(2), 0),
    });
    assert_eq!(
        moved,
        EditReport {
            adopted_lanes: 1,
            ..EditReport::default()
        }
    );
    assert_eq!(hits(&p, 1, &event("kick")), [vec![OscArgument::Float(2.0)]]);
}

#[test]
fn event_uses_counts_the_lanes_and_clips_that_play_an_event() {
    let mut p = fixture();
    let track = &p[TRACK];
    assert_eq!(track.event_uses(&event("kick")), (2, 2));
    assert_eq!(track.event_uses(&event("kick-3")), (1, 1));
    assert_eq!(track.event_uses(&event("empty")), (1, 1));
    assert_eq!(track.event_uses(&event("nothing")), (0, 0));
    // A derived lane (hits, no declared lane) counts too.
    p.track_mut(TRACK).unwrap().slots[1]
        .clip
        .as_mut()
        .unwrap()
        .lanes
        .clear();
    assert_eq!(p[TRACK].event_uses(&event("kick")), (2, 2));
}
