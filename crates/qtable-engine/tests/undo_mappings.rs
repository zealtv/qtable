//! Mappings stay outside the undo history (decided 2026-11-12): a mapping edit makes no step,
//! and an undo or redo never restores them. The exception: a mapping that went with its
//! control comes back with the control, as in Live.
use qtable_engine::{
    edit::{MappingEdit, ProjectEdit},
    engine::{Command, Engine, Gesture, Receipt},
    model::*,
};
use std::time::Instant;

fn volume(track: u64) -> ControlRef {
    ControlRef::Argument(FieldRef::volume(TrackId(track)))
}

fn cc(number: u8) -> Source {
    Source::Midi(MidiSource {
        channel: 1,
        message: MidiMessage::Cc(number),
    })
}

fn engine() -> Engine {
    let tracks = (1..=2)
        .map(|n| Track::new(TrackId(n), format!("T{n}"), 1))
        .collect();
    Engine::new(Project {
        tracks,
        scenes: vec![Scene {
            id: SceneId(1),
            name: "Scene".into(),
            follow_actions: Default::default(),
        }],
        ..Project::default()
    })
    .unwrap()
}

fn edit(engine: &mut Engine, edit: ProjectEdit) {
    engine.execute(Command::Edit(edit)).unwrap();
}

fn map(engine: &mut Engine, number: u8, control: ControlRef) {
    edit(
        engine,
        ProjectEdit::EditMapping(MappingEdit::Add {
            source: cc(number),
            control,
            min: 0.0,
            max: 1.0,
        }),
    );
}

fn set_volume(engine: &mut Engine, track: u64, value: f64) {
    edit(
        engine,
        ProjectEdit::SetParameter {
            target: ParameterRef::volume(TrackId(track)),
            values: vec![(FieldKey("value".into()), value)],
        },
    );
}

fn stored(engine: &Engine, track: u64) -> f64 {
    let field = FieldRef::volume(TrackId(track));
    engine.project().field(&field).unwrap().value
}

fn rename(engine: &mut Engine, name: &str) {
    edit(
        engine,
        ProjectEdit::RenameTrack {
            track: TrackId(1),
            name: name.into(),
        },
    );
}

fn name(engine: &Engine) -> String {
    engine.project().track(TrackId(1)).unwrap().name.clone()
}

fn move_in(engine: &mut Engine, gesture: Option<Gesture>, value: f64) {
    let context = engine
        .command_context(Receipt {
            received_at: Instant::now(),
            sequence: None,
        })
        .with_gesture(gesture);
    engine
        .execute_with_context(
            Command::Edit(ProjectEdit::SetParameter {
                target: ParameterRef::volume(TrackId(1)),
                values: vec![(FieldKey("value".into()), value)],
            }),
            context,
        )
        .unwrap();
}

#[test]
fn a_mapping_edit_makes_no_step_and_leaves_the_redo_stack_alone() {
    let mut engine = engine();
    assert!(!engine.can_undo());
    map(&mut engine, 1, volume(1));
    assert!(!engine.can_undo(), "mapping alone makes no step");
    rename(&mut engine, "A");
    engine.execute(Command::Undo).unwrap();
    assert!(engine.can_redo());
    let id = engine.project().mappings[0].id;
    for edit_ in [
        MappingEdit::SetRange {
            mapping: id,
            min: 0.2,
            max: 0.8,
        },
        MappingEdit::SetMode {
            mapping: id,
            mode: SourceMode::Absolute,
        },
        MappingEdit::SetTakeover {
            mapping: id,
            takeover: Takeover::None,
        },
    ] {
        edit(&mut engine, ProjectEdit::EditMapping(edit_));
    }
    map(&mut engine, 2, volume(2));
    edit(
        &mut engine,
        ProjectEdit::EditMapping(MappingEdit::Remove { mapping: id }),
    );
    assert!(!engine.can_undo(), "no step from any mapping edit");
    assert!(engine.can_redo(), "the redo stack survives mapping edits");
    assert_eq!(engine.redo_label(), Some("Rename Track"));
    engine.execute(Command::Redo).unwrap();
    assert_eq!(name(&engine), "A");
}

#[test]
fn the_tour_case_map_move_undo_undo_keeps_the_mapping() {
    let mut engine = engine();
    rename(&mut engine, "Before");
    map(&mut engine, 7, volume(1));
    let mapping = engine.project().mappings.clone();
    let start = stored(&engine, 1);
    set_volume(&mut engine, 1, 0.9);
    engine.execute(Command::Undo).unwrap();
    assert_eq!(
        stored(&engine, 1),
        start,
        "the first undo moves the value back"
    );
    assert_eq!(engine.project().mappings, mapping);
    engine.execute(Command::Undo).unwrap();
    assert_eq!(name(&engine), "T1", "the second undo takes the step before");
    assert_eq!(
        engine.project().mappings,
        mapping,
        "the mapping is still there"
    );
    assert!(!engine.can_undo());
    engine.execute(Command::Redo).unwrap();
    engine.execute(Command::Redo).unwrap();
    assert_eq!(stored(&engine, 1), 0.9);
    assert_eq!(engine.project().mappings, mapping);
}

#[test]
fn undo_and_redo_keep_mappings_made_or_removed_after_the_step() {
    let mut engine = engine();
    map(&mut engine, 1, volume(1));
    rename(&mut engine, "A");
    // Made after the step: undo must not drop it. Removed after the step: undo must not
    // bring it back.
    map(&mut engine, 2, volume(2));
    let first = engine.project().mappings[0].id;
    edit(
        &mut engine,
        ProjectEdit::EditMapping(MappingEdit::Remove { mapping: first }),
    );
    let now = engine.project().mappings.clone();
    assert_eq!(now.len(), 1);
    engine.execute(Command::Undo).unwrap();
    assert_eq!(engine.project().mappings, now);
    engine.execute(Command::Redo).unwrap();
    assert_eq!(engine.project().mappings, now);
}

#[test]
fn a_mapping_edit_marks_the_set_changed_and_an_undo_never_passes_it_off_as_saved() {
    let mut engine = engine();
    map(&mut engine, 1, volume(1));
    // Saved here: one mapping.
    let saved = engine.content_id();
    rename(&mut engine, "A");
    let id = engine.project().mappings[0].id;
    edit(
        &mut engine,
        ProjectEdit::EditMapping(MappingEdit::Remove { mapping: id }),
    );
    assert_ne!(engine.content_id(), saved, "a mapping edit changes the set");
    // The snapshot the undo restores holds the mapping; the set keeps its current mappings,
    // so it is not what was saved.
    engine.execute(Command::Undo).unwrap();
    assert!(engine.project().mappings.is_empty());
    assert_ne!(engine.content_id(), saved);
    engine.execute(Command::Redo).unwrap();
    assert_ne!(engine.content_id(), saved);
}

#[test]
fn a_mapping_edit_does_not_close_an_open_gesture_step() {
    let mut engine = engine();
    let start = stored(&engine, 1);
    let gesture = Some(engine.gesture());
    let move_to = |engine: &mut Engine, value: f64| move_in(engine, gesture, value);
    move_to(&mut engine, 0.3);
    map(&mut engine, 1, volume(2));
    move_to(&mut engine, 0.6);
    engine.execute(Command::Undo).unwrap();
    assert_eq!(stored(&engine, 1), start, "both moves are one step");
    assert!(!engine.can_undo());
    assert_eq!(engine.project().mappings.len(), 1);
}

#[test]
fn a_gesture_that_returns_to_its_start_vanishes_even_with_a_mapping_made_meanwhile() {
    let mut engine = engine();
    let start = stored(&engine, 1);
    let before = engine.content_id();
    let gesture = Some(engine.gesture());
    let move_to = |engine: &mut Engine, value: f64| move_in(engine, gesture, value);
    move_to(&mut engine, 0.3);
    map(&mut engine, 1, volume(2));
    move_to(&mut engine, start);
    assert!(!engine.can_undo(), "the step ended where it began");
    assert_ne!(
        engine.content_id(),
        before,
        "the mapping still changed the file"
    );
    assert_eq!(engine.project().mappings.len(), 1);
}

#[test]
fn undoing_the_creation_of_a_mapped_control_drops_its_mapping_as_deleting_it_does() {
    let mut engine = engine();
    edit(&mut engine, ProjectEdit::AddTrack { name: "New".into() });
    let new = engine.project().tracks.last().unwrap().id;
    map(&mut engine, 1, volume(new.0));
    map(&mut engine, 2, volume(1));
    assert_eq!(engine.project().mappings.len(), 2);
    engine.execute(Command::Undo).unwrap();
    assert!(engine.project().track(new).is_none());
    let rows = &engine.project().mappings;
    assert_eq!(rows.len(), 1, "the mapping of the vanished control goes");
    assert_eq!(rows[0].control, volume(1), "the other stays");
    // The same outcome as deleting the track by an edit.
    let mut direct = self::engine();
    edit(&mut direct, ProjectEdit::AddTrack { name: "New".into() });
    let new = direct.project().tracks.last().unwrap().id;
    map(&mut direct, 1, volume(new.0));
    edit(&mut direct, ProjectEdit::RemoveTracks { tracks: vec![new] });
    assert!(direct.project().mappings.is_empty());
    // Redo brings the track back, and its mapping with it.
    engine.execute(Command::Redo).unwrap();
    assert!(engine.project().track(new).is_some());
    assert_eq!(engine.project().mappings.len(), 2);
    assert!(
        engine
            .project()
            .mappings
            .iter()
            .any(|m| m.control == volume(new.0))
    );
    engine.project().validate().unwrap();
}

// ---- Mappings come back with their control (2026-11-12) ----

/// Map the control with a learned source, then give the row a non-default range and takeover so
/// a restore that rebuilt the row from defaults would show.
fn map_tuned(engine: &mut Engine, number: u8, control: ControlRef) -> RemoteMapping {
    map(engine, number, control.clone());
    let row = engine
        .project()
        .mappings
        .iter()
        .find(|m| m.control == control)
        .unwrap()
        .clone();
    for edit_ in [
        MappingEdit::SetRange {
            mapping: row.id,
            min: 0.2,
            max: 0.8,
        },
        MappingEdit::SetTakeover {
            mapping: row.id,
            takeover: Takeover::PickUp,
        },
    ] {
        edit(engine, ProjectEdit::EditMapping(edit_));
    }
    engine
        .project()
        .mappings
        .iter()
        .find(|m| m.control == control)
        .unwrap()
        .clone()
}

fn remove_tracks(engine: &mut Engine, tracks: &[u64]) {
    edit(
        engine,
        ProjectEdit::RemoveTracks {
            tracks: tracks.iter().map(|n| TrackId(*n)).collect(),
        },
    );
}

#[test]
fn delete_a_mapped_track_then_undo_brings_the_mapping_back_unchanged() {
    let mut engine = engine();
    let row = map_tuned(&mut engine, 7, volume(1));
    map(&mut engine, 8, volume(2));
    let before = engine.project().clone();
    let before_id = engine.content_id();
    remove_tracks(&mut engine, &[1]);
    assert_eq!(engine.project().mappings.len(), 1, "the edit drops its row");
    engine.execute(Command::Undo).unwrap();
    assert_eq!(engine.project(), &before);
    assert!(
        engine.project().mappings.contains(&row),
        "same id, source, range, takeover"
    );
    assert_eq!(
        engine.content_id(),
        before_id,
        "the set matches what it was"
    );
    engine.project().validate().unwrap();
}

#[test]
fn undoing_a_creation_then_redoing_it_brings_the_mapping_back() {
    let mut engine = engine();
    edit(&mut engine, ProjectEdit::AddTrack { name: "New".into() });
    let new = engine.project().tracks.last().unwrap().id;
    let row = map_tuned(&mut engine, 3, volume(new.0));
    engine.execute(Command::Undo).unwrap();
    assert!(engine.project().mappings.is_empty());
    let undone = engine.content_id();
    engine.execute(Command::Redo).unwrap();
    assert_eq!(engine.project().mappings, vec![row.clone()]);
    // And undoing the creation once more drops it again, then redo restores it again.
    engine.execute(Command::Undo).unwrap();
    assert!(engine.project().mappings.is_empty());
    assert_eq!(engine.content_id(), undone);
    engine.execute(Command::Redo).unwrap();
    assert_eq!(engine.project().mappings, vec![row]);
}

#[test]
fn an_undo_redo_undo_cycle_restores_the_mapping_each_time() {
    let mut engine = engine();
    let row = map_tuned(&mut engine, 7, volume(1));
    remove_tracks(&mut engine, &[1]);
    for _ in 0..3 {
        engine.execute(Command::Undo).unwrap();
        assert_eq!(engine.project().mappings, vec![row.clone()]);
        engine.execute(Command::Redo).unwrap();
        assert!(engine.project().mappings.is_empty());
    }
    engine.execute(Command::Undo).unwrap();
    assert_eq!(engine.project().mappings, vec![row]);
}

#[test]
fn a_mapping_changed_with_the_control_present_is_never_reverted_by_the_return() {
    let mut engine = engine();
    let row = map_tuned(&mut engine, 7, volume(1));
    remove_tracks(&mut engine, &[1]);
    engine.execute(Command::Undo).unwrap();
    // The control is back; edit its mapping, then redo and undo the delete.
    edit(
        &mut engine,
        ProjectEdit::EditMapping(MappingEdit::SetRange {
            mapping: row.id,
            min: 0.0,
            max: 0.5,
        }),
    );
    let changed = engine.project().mappings.clone();
    assert_eq!(changed[0].max, 0.5);
    engine.execute(Command::Redo).unwrap();
    assert!(engine.project().mappings.is_empty());
    engine.execute(Command::Undo).unwrap();
    assert_eq!(
        engine.project().mappings,
        changed,
        "the row that went is the one made last, not the old one"
    );
}

#[test]
fn deleting_a_group_with_a_mapped_track_then_undoing_brings_every_mapping_back() {
    let mut engine = engine();
    // A track outside the group, since a set keeps at least one.
    edit(
        &mut engine,
        ProjectEdit::AddTrack {
            name: "Spare".into(),
        },
    );
    edit(
        &mut engine,
        ProjectEdit::CreateGroup {
            tracks: vec![TrackId(1), TrackId(2)],
        },
    );
    let group = engine
        .project()
        .tracks
        .iter()
        .find(|t| t.is_group())
        .unwrap()
        .id;
    map_tuned(&mut engine, 1, volume(1));
    map_tuned(&mut engine, 2, volume(2));
    map_tuned(&mut engine, 3, volume(group.0));
    let before = engine.project().clone();
    remove_tracks(&mut engine, &[group.0]);
    assert!(engine.project().mappings.is_empty());
    engine.execute(Command::Undo).unwrap();
    assert_eq!(engine.project(), &before);
    engine.execute(Command::Redo).unwrap();
    assert!(engine.project().mappings.is_empty());
    engine.execute(Command::Undo).unwrap();
    assert_eq!(engine.project(), &before);
}

fn device_engine() -> Engine {
    let mut track = Track::new(TrackId(1), "T1", 2);
    let device = track.first_target_mut_or_insert(|| DeviceId(1));
    let target = device.target_mut().unwrap();
    target.parameters.push(ParameterDef {
        key: EntryKey("p".into()),
        name: String::new(),
        path: OscAddress::Relative("/p".into()),
        fields: vec![FieldDef::new(
            FieldKey("value".into()),
            FieldType::Float,
            true,
        )],
        send_mode: SendMode::Continuous,
    });
    let second = Track::new(TrackId(2), "T2", 2);
    let scenes = (1..=2)
        .map(|n| Scene {
            id: SceneId(n),
            name: format!("S{n}"),
            follow_actions: Default::default(),
        })
        .collect();
    Engine::new(Project {
        tracks: vec![track, second],
        scenes,
        ..Project::default()
    })
    .unwrap()
}

#[test]
fn device_scene_and_slot_controls_come_back_with_their_owner() {
    let mut engine = device_engine();
    let parameter = ControlRef::Argument(FieldRef {
        parameter: ParameterRef {
            track: TrackId(1),
            device: DeviceRef::Chain(DeviceId(1)),
            key: ParameterKey("p".into()),
        },
        field: FieldKey("value".into()),
    });
    let rows = [
        map_tuned(&mut engine, 1, parameter),
        map_tuned(&mut engine, 2, ControlRef::Scene(SceneId(2))),
        map_tuned(
            &mut engine,
            3,
            ControlRef::ClipSlot {
                track: TrackId(2),
                scene: SceneId(2),
            },
        ),
        map_tuned(&mut engine, 4, ControlRef::TrackSolo(TrackId(2))),
    ];
    let before = engine.project().clone();
    // A device, a scene (with its slot control), then a track.
    edit(
        &mut engine,
        ProjectEdit::RemoveDevices {
            devices: vec![DeviceId(1)],
        },
    );
    edit(&mut engine, ProjectEdit::RemoveScene { scene: SceneId(2) });
    remove_tracks(&mut engine, &[2]);
    assert!(engine.project().mappings.is_empty());
    for _ in 0..3 {
        engine.execute(Command::Undo).unwrap();
    }
    assert_eq!(engine.project(), &before);
    for row in rows {
        assert!(engine.project().mappings.contains(&row));
    }
    for _ in 0..3 {
        engine.execute(Command::Redo).unwrap();
    }
    assert!(engine.project().mappings.is_empty());
}

#[test]
fn a_step_that_falls_off_the_history_takes_its_mappings_with_it() {
    let mut engine = engine();
    map(&mut engine, 7, volume(2));
    remove_tracks(&mut engine, &[2]);
    for n in 0..200 {
        rename(&mut engine, &format!("R{n}"));
    }
    while engine.can_undo() {
        engine.execute(Command::Undo).unwrap();
    }
    assert!(
        engine.project().track(TrackId(2)).is_none(),
        "the delete fell off the 200 steps kept"
    );
    assert!(engine.project().mappings.is_empty());
}

#[test]
fn saving_never_writes_the_mappings_a_delete_dropped() {
    let mut engine = engine();
    map(&mut engine, 7, volume(1));
    remove_tracks(&mut engine, &[1]);
    let path = std::env::temp_dir().join(format!(
        "qtable-undo-brings-mappings-back-{}.ron",
        std::process::id()
    ));
    qtable_engine::file::save(&path, engine.project()).unwrap();
    let saved = qtable_engine::file::load(&path).unwrap();
    assert!(saved.mappings.is_empty());
    // Undo, save again: the row is in the file only once it is live again.
    engine.execute(Command::Undo).unwrap();
    qtable_engine::file::save(&path, engine.project()).unwrap();
    assert_eq!(qtable_engine::file::load(&path).unwrap().mappings.len(), 1);
    let _ = std::fs::remove_file(&path);
}
