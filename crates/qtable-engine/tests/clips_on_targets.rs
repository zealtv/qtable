//! Format 7: v6 lanes migrate into one Target per clip track, addresses stay byte-identical,
//! and clips that cross tracks bring their definitions.
use qtable_engine::edit::{self, Carried, ProjectEdit, TargetEdit};
use qtable_engine::file;
use qtable_engine::model::*;

fn fixture() -> Project {
    file::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/v6-lanes.ron"
    ))
    .unwrap()
}

fn relative(path: &str) -> OscAddress {
    OscAddress::Relative(path.into())
}

#[test]
fn v6_lanes_become_one_target_per_clip_track_in_the_designed_order() {
    let p = fixture();
    let track = &p[TrackId(1)];
    assert_eq!(track.devices.len(), 1);
    assert_eq!(track.devices[0].id, DeviceId(1));
    assert_eq!(track.devices[0].name, "Target");
    let (_, target) = track.targets().next().unwrap();
    assert!(target.path.is_empty());
    let found: Vec<_> = target
        .events
        .iter()
        .map(|e| {
            let signature: Vec<_> = e.signature().collect();
            (e.key.0.as_str(), e.name.as_str(), e.path.clone(), signature)
        })
        .collect();
    assert_eq!(
        found,
        vec![
            ("kick", "", relative("/kick"), vec![FieldType::Int]),
            ("kick-2", "", OscAddress::Absolute("/kick".into()), vec![]),
            ("kick-3", "", relative("/kick"), vec![FieldType::Float]),
            ("snare", "", relative("/snare"), vec![]),
            ("empty", "", relative("/empty"), vec![]),
        ]
    );
    let key = |r: &EventRef| r.key.0.clone();
    let a = track.slots[0].clip.as_ref().unwrap();
    let lanes: Vec<_> = a
        .lanes
        .iter()
        .filter_map(Lane::event)
        .map(|l| key(&l.definition))
        .collect();
    assert_eq!(lanes, ["empty", "kick", "kick-3", "kick-2", "snare"]);
    assert_eq!(
        labels(&p, 0),
        ["/empty", "/kick (i)", "/kick (f)", "abs /kick", "/snare"]
    );
    let hits: Vec<_> = a.events.iter().map(|e| key(&e.definition)).collect();
    assert_eq!(hits, ["kick", "kick-2", "kick-3", "snare"]);
    assert_eq!(a.events[2].arguments, vec![OscArgument::Float(0.5)]);
    let b = track.slots[1].clip.as_ref().unwrap();
    assert_eq!(b.lane_refs(), vec![b.events[0].definition.clone()]);
    assert_eq!(key(&b.events[0].definition), "kick");
    for id in [TrackId(10), TrackId(2), TrackId(201)] {
        assert!(p.track(id).unwrap().devices.is_empty(), "{id:?}");
    }
    assert!(p.main.devices.is_empty());
}

/// Each lane's label on track 1's clip in `slot`, in `lane_refs` order.
fn labels(p: &Project, slot: usize) -> Vec<String> {
    let track = &p[TrackId(1)];
    let clip = track.slots[slot].clip.as_ref().unwrap();
    clip.lane_refs()
        .iter()
        .map(|r| track.event_label(r).into_owned())
        .collect()
}

#[test]
fn a_renamed_lane_label_follows_its_path() {
    // Repro: `kick-3` is clip A's third lane and only clip A plays it, so typing
    // over it renames in place. A name baked at migration would still read `/kick (f)`.
    let mut p = fixture();
    let edit = ProjectEdit::EditLane {
        track: TrackId(1),
        slot: 0,
        edit: edit::LaneEdit::SetPath {
            lane: 2,
            path: "/tom".into(),
        },
    };
    edit::apply(&mut p, edit).unwrap();
    assert_eq!(
        labels(&p, 0),
        ["/empty", "/kick", "/tom", "abs /kick", "/snare"]
    );
}

#[test]
fn labels_mark_absolute_and_add_signatures_only_where_text_collides() {
    let mut p = fixture();
    let target = |edit| ProjectEdit::EditTarget {
        device: DeviceId(1),
        edit,
    };
    let key = |k: &str| EntryKey(k.into());
    // A name replaces the path text, and an absolute entry with a name isn't marked.
    for (entry, name) in [("kick-3", "Accent"), ("kick-2", "Gate")] {
        let edit = TargetEdit::SetName {
            key: key(entry),
            name: name.into(),
        };
        edit::apply(&mut p, target(edit)).unwrap();
    }
    assert_eq!(
        labels(&p, 0),
        ["/empty", "/kick", "Accent", "Gate", "/snare"]
    );
    // Two names that agree take their signatures; `kick-2` has none.
    let edit = TargetEdit::SetName {
        key: key("kick-2"),
        name: "Accent".into(),
    };
    edit::apply(&mut p, target(edit)).unwrap();
    assert_eq!(
        labels(&p, 0),
        [
            "/empty",
            "/kick",
            "Accent (f)",
            "Accent (no arguments)",
            "/snare"
        ]
    );
    // An absolute path with no relative entry of the same text isn't marked.
    let mut p = fixture();
    let edit = TargetEdit::RemoveEntries {
        keys: vec![key("kick"), key("kick-3")],
    };
    edit::apply(&mut p, target(edit)).unwrap();
    assert_eq!(labels(&p, 0), ["/empty", "/kick", "/snare"]);
    let gone = EventRef {
        device: DeviceId(1),
        key: key("kick"),
    };
    assert_eq!(p[TrackId(1)].event_label(&gone), "");
}

#[test]
fn migrated_hits_resolve_to_the_addresses_format_6_sent() {
    // Written from the format 6 rule before the change: output path, group paths, track
    // path, then the relative address; an absolute address alone.
    let expected = [
        (0, "/out/drums/kick"),
        (0, "/kick"),
        (0, "/out/drums/kick"),
        (0, "/out/drums/snare"),
        (1, "/out/drums/kick"),
    ];
    let p = fixture();
    let track = &p[TrackId(1)];
    let resolved: Vec<_> = track
        .slots
        .iter()
        .enumerate()
        .flat_map(|(slot, s)| {
            s.clip
                .iter()
                .flat_map(move |c| c.events.iter().map(move |e| (slot, e)))
        })
        .map(|(slot, event)| {
            let piece = p.event_path(track.id, &event.definition).unwrap();
            let address = p
                .resolve_event_address(track.id, TrackId(201), &piece)
                .unwrap();
            (slot, address)
        })
        .collect();
    let expected: Vec<_> = expected.iter().map(|(s, a)| (*s, a.to_string())).collect();
    assert_eq!(resolved, expected);
}

#[test]
fn current_format_round_trips_and_inactive_devices_resolve_nothing() {
    let mut p = fixture();
    let path = std::env::temp_dir().join(format!("qtable-format-7-{}.ron", std::process::id()));
    file::save(&path, &p).unwrap();
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .contains("version: 12")
    );
    assert_eq!(file::load(&path).unwrap(), p);
    std::fs::remove_file(path).unwrap();
    let hit = p[TrackId(1)].slots[0].clip.as_ref().unwrap().events[0].clone();
    p.device_mut(DeviceId(1)).unwrap().active = false;
    assert_eq!(p.event_path(TrackId(1), &hit.definition), None);
}

/// A second clip track (3) routed like track 1, with its own Target holding `/kick (i)`.
fn two_tracks() -> Project {
    let mut p = fixture();
    let mut other = Track::new(TrackId(3), "Other", 2);
    other.route = Route::single(TrackId(201));
    p.tracks.push(other);
    p.ensure_event(TrackId(3), relative("/kick"), &[FieldType::Int]);
    p.validate().unwrap();
    p
}

fn target_events(p: &Project, track: TrackId) -> Vec<(OscAddress, Vec<FieldType>)> {
    p[track]
        .targets()
        .flat_map(|(_, t)| &t.events)
        .map(|e| (e.path.clone(), e.signature().collect()))
        .collect()
}

#[test]
fn move_clip_to_another_track_adopts_reusing_a_matching_event() {
    let mut p = two_tracks();
    let edit = ProjectEdit::MoveClip {
        from: (TrackId(1), 0),
        to: (TrackId(3), 1),
    };
    edit::apply(&mut p, edit).unwrap();
    // /kick (i) is reused; the other four are copied in, keys fresh on that Target.
    let events = target_events(&p, TrackId(3));
    assert_eq!(events.len(), 5);
    assert_eq!(events[0], (relative("/kick"), vec![FieldType::Int]));
    let moved = p[TrackId(3)].slots[1].clip.as_ref().unwrap();
    assert_eq!(
        moved.events[0].definition,
        p[TrackId(3)].find_event(&relative("/kick")).unwrap()
    );
    assert!(p[TrackId(1)].slots[0].clip.is_none());
}

#[test]
fn paste_clip_adopts_carried_definitions_and_rejects_a_foreign_ref_without_them() {
    let mut p = two_tracks();
    let source = &p[TrackId(1)];
    let clip = source.slots[0].clip.clone().unwrap();
    let carried = Carried::from_clip(source, &clip);
    let bare = ProjectEdit::PasteClip {
        track: TrackId(3),
        slot: 0,
        clip: Box::new(clip.clone()),
        carried: Vec::new(),
    };
    assert!(edit::apply(&mut p, bare).is_err());
    let paste = ProjectEdit::PasteClip {
        track: TrackId(3),
        slot: 0,
        clip: Box::new(clip.clone()),
        carried,
    };
    edit::apply(&mut p, paste).unwrap();
    assert_eq!(target_events(&p, TrackId(3)).len(), 5);
    // The same paste onto its own track keeps its refs.
    let home = ProjectEdit::PasteClip {
        track: TrackId(1),
        slot: 1,
        clip: Box::new(clip.clone()),
        carried: Vec::new(),
    };
    edit::apply(&mut p, home).unwrap();
    let pasted = p[TrackId(1)].slots[1].clip.as_ref().unwrap();
    assert_eq!(pasted.events, clip.events);
}

#[test]
fn duplicate_clip_leaves_refs_alone() {
    let mut p = fixture();
    let edit = ProjectEdit::DuplicateClip {
        track: TrackId(1),
        slot: 1,
    };
    edit::apply(&mut p, edit).unwrap();
    let track = &p[TrackId(1)];
    let copy = track.slots[2].clip.as_ref().unwrap();
    assert_eq!(copy.events, track.slots[1].clip.as_ref().unwrap().events);
    assert_eq!(target_events(&p, TrackId(1)).len(), 5);
}

/// A set with one track whose first Target (device 1) has one event at `path`, and a clip
/// in slot 0 hitting it with `arguments`. Sets built this way share refs, as migrated ones do.
fn set_with(path: OscAddress, arguments: Vec<OscArgument>) -> Project {
    let mut p = Project {
        scenes: (1..=2)
            .map(|id| Scene {
                id: SceneId(id),
                name: String::new(),
                follow_actions: Default::default(),
            })
            .collect(),
        tracks: vec![Track::new(TrackId(1), "Cues", 2)],
        ..Project::default()
    };
    let length = BeatTime::from_beats(4).unwrap();
    let create = ProjectEdit::CreateClip {
        track: TrackId(1),
        slot: 0,
        length,
    };
    edit::apply(&mut p, create).unwrap();
    let event = p.hit(TrackId(1), BeatTime::ZERO, path, arguments).unwrap();
    let add = ProjectEdit::AddEvent {
        track: TrackId(1),
        slot: 0,
        event,
        carried: None,
    };
    edit::apply(&mut p, add).unwrap();
    p
}

fn copy(p: &Project, slot: usize) -> (Clip, Vec<Carried>) {
    let clip = p[TrackId(1)].slots[slot].clip.clone().unwrap();
    let carried = Carried::from_clip(&p[TrackId(1)], &clip);
    (clip, carried)
}

fn paste(p: &mut Project, slot: usize, clip: &Clip, carried: &[Carried]) {
    let paste = ProjectEdit::PasteClip {
        track: TrackId(1),
        slot,
        clip: Box::new(clip.clone()),
        carried: carried.to_vec(),
    };
    edit::apply(p, paste).unwrap();
}

/// The path and arguments the first hit in `slot` plays.
fn first_hit(p: &Project, slot: usize) -> (OscAddress, Vec<OscArgument>) {
    let hit = &p[TrackId(1)].slots[slot].clip.as_ref().unwrap().events[0];
    let (_, definition) = p[TrackId(1)].event_def(&hit.definition).unwrap();
    (definition.path.clone(), hit.arguments.clone())
}

fn target_edit(p: &mut Project, edit: TargetEdit) {
    let edit = ProjectEdit::EditTarget {
        device: DeviceId(1),
        edit,
    };
    edit::apply(p, edit).unwrap();
}

#[test]
fn a_clip_pasted_into_another_set_adopts_a_ref_that_names_another_event_there() {
    let x = set_with(relative("/cue"), vec![OscArgument::Int(3)]);
    let (clip, carried) = copy(&x, 0);
    for arguments in [vec![OscArgument::Int(0)], vec![OscArgument::Float(0.0)]] {
        let mut y = set_with(OscAddress::Absolute("/show/cue".into()), arguments);
        assert_eq!(
            copy(&y, 0).0.events[0].definition,
            clip.events[0].definition
        );
        paste(&mut y, 1, &clip, &carried);
        assert_eq!(
            first_hit(&y, 1),
            (relative("/cue"), vec![OscArgument::Int(3)])
        );
        assert_eq!(first_hit(&y, 0).0, OscAddress::Absolute("/show/cue".into()));
    }
}

#[test]
fn a_device_id_freed_and_reused_doesnt_capture_a_copied_clip() {
    let mut p = set_with(relative("/kick"), vec![OscArgument::Int(1)]);
    let (clip, carried) = copy(&p, 0);
    let remove = ProjectEdit::RemoveDevices {
        devices: vec![DeviceId(1)],
    };
    edit::apply(&mut p, remove).unwrap();
    let add = ProjectEdit::AddDevice {
        track: TrackId(1),
        kind: NewDevice::Target,
        before: None,
    };
    edit::apply(&mut p, add).unwrap();
    // The new device reuses id 1, and its `kick` entry now sends /snare with the same type.
    let kick = EntryKey("kick".into());
    for edit in [
        TargetEdit::AddEvent {
            path: relative("/kick"),
            name: String::new(),
        },
        TargetEdit::SetEntryPath {
            key: kick.clone(),
            path: relative("/snare"),
        },
        TargetEdit::AddField {
            entry: kick,
            value_type: FieldType::Int,
        },
    ] {
        target_edit(&mut p, edit);
    }
    paste(&mut p, 1, &clip, &carried);
    assert_eq!(
        first_hit(&p, 1),
        (relative("/kick"), vec![OscArgument::Int(1)])
    );
    assert_eq!(
        target_events(&p, TrackId(1)),
        vec![
            (relative("/snare"), vec![FieldType::Int]),
            (relative("/kick"), vec![FieldType::Int]),
        ]
    );
}

#[test]
fn pasting_after_an_argument_is_added_conforms_instead_of_rejecting() {
    let mut p = set_with(relative("/cue"), vec![OscArgument::Int(3)]);
    let (clip, carried) = copy(&p, 0);
    let cue = EntryKey("cue".into());
    target_edit(
        &mut p,
        TargetEdit::AddField {
            entry: cue,
            value_type: FieldType::Float,
        },
    );
    let conformed = vec![OscArgument::Int(3), OscArgument::Float(0.0)];
    paste(&mut p, 1, &clip, &carried);
    assert_eq!(first_hit(&p, 1), (relative("/cue"), conformed.clone()));
    // A pasted hit conforms too, with or without its carried definition.
    for carried in [carried.first().cloned(), None] {
        let add = ProjectEdit::AddEvent {
            track: TrackId(1),
            slot: 1,
            event: clip.events[0].clone(),
            carried,
        };
        edit::apply(&mut p, add).unwrap();
    }
    let hits = &p[TrackId(1)].slots[1].clip.as_ref().unwrap().events;
    assert!(hits.iter().all(|hit| hit.arguments == conformed));
    assert_eq!(target_events(&p, TrackId(1)).len(), 1);
}

#[test]
fn pasting_after_an_entry_path_change_on_its_own_track_pastes_the_copied_path() {
    // Decided 2026-10-15: paste what you copied.
    let mut p = set_with(relative("/cue"), vec![OscArgument::Int(3)]);
    let (clip, carried) = copy(&p, 0);
    target_edit(
        &mut p,
        TargetEdit::SetEntryPath {
            key: EntryKey("cue".into()),
            path: relative("/go"),
        },
    );
    paste(&mut p, 1, &clip, &carried);
    assert_eq!(first_hit(&p, 0).0, relative("/go"));
    assert_eq!(first_hit(&p, 1).0, relative("/cue"));
    // Without its carried definitions, the clip follows the rename.
    paste(&mut p, 1, &clip, &[]);
    assert_eq!(first_hit(&p, 1).0, relative("/go"));
}
