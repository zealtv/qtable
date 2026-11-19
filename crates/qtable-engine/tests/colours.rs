use qtable_engine::edit::{ProjectEdit, apply};
use qtable_engine::model::*;

fn project() -> Project {
    Project {
        scenes: vec![Scene {
            id: SceneId(1),
            name: "Scene".into(),
            follow_actions: Default::default(),
        }],
        ..Project::default()
    }
}

fn add_track(project: &mut Project) {
    apply(
        project,
        ProjectEdit::AddTrack {
            name: "Track".into(),
        },
    )
    .unwrap();
}

#[test]
fn tracks_and_groups_share_a_persisted_palette_cycle() {
    let mut p = project();
    add_track(&mut p);
    assert_eq!(p.tracks[0].colour, LIVE_CLIP_PALETTE[0]);
    add_track(&mut p);
    assert_eq!(p.tracks[1].colour, LIVE_CLIP_PALETTE[1]);
    apply(
        &mut p,
        ProjectEdit::CreateGroup {
            tracks: vec![TrackId(1), TrackId(2)],
        },
    )
    .unwrap();
    assert_eq!(
        p.group_tracks().next().unwrap().colour,
        LIVE_CLIP_PALETTE[2]
    );
    let next = p.next_colour_index;
    assert!(
        apply(
            &mut p,
            ProjectEdit::CreateGroup {
                tracks: vec![TrackId(1), TrackId(1)]
            }
        )
        .is_err()
    );
    assert_eq!(p.next_colour_index, next);
    for expected in &LIVE_CLIP_PALETTE[3..] {
        add_track(&mut p);
        assert_eq!(p.tracks.last().unwrap().colour, *expected);
    }
    assert_eq!(p.next_colour_index, 0);
    add_track(&mut p);
    assert_eq!(p.tracks.last().unwrap().colour, LIVE_CLIP_PALETTE[0]);
    let last = p.tracks.last().unwrap().id;
    apply(&mut p, ProjectEdit::RemoveTracks { tracks: vec![last] }).unwrap();
    let source = ron::to_string(&p).unwrap();
    let mut restored: Project = ron::from_str(&source).unwrap();
    add_track(&mut restored);
    assert_eq!(restored.tracks.last().unwrap().colour, LIVE_CLIP_PALETTE[1]);
    restored.validate().unwrap();
}

#[test]
fn new_clips_inherit_current_track_colour_without_recolouring_existing_clips() {
    let mut p = project();
    add_track(&mut p);
    let create = ProjectEdit::CreateClip {
        track: TrackId(1),
        slot: 0,
        length: BeatTime(960),
    };
    apply(&mut p, create.clone()).unwrap();
    assert_eq!(
        p.tracks[0].slots[0].clip.as_ref().unwrap().colour,
        LIVE_CLIP_PALETTE[0]
    );
    let custom = Colour {
        red: 17,
        green: 34,
        blue: 51,
    };
    apply(
        &mut p,
        ProjectEdit::RecolourTrack {
            track: TrackId(1),
            colour: custom,
        },
    )
    .unwrap();
    assert_eq!(
        p.tracks[0].slots[0].clip.as_ref().unwrap().colour,
        LIVE_CLIP_PALETTE[0]
    );
    assert_eq!(p.next_colour_index, 1);
    apply(
        &mut p,
        ProjectEdit::DeleteClip {
            track: TrackId(1),
            slot: 0,
        },
    )
    .unwrap();
    apply(&mut p, create).unwrap();
    assert_eq!(p.tracks[0].slots[0].clip.as_ref().unwrap().colour, custom);
}

#[test]
fn old_files_keep_stored_colours_and_default_missing_colour_fields() {
    let old: Project = ron::from_str("(tracks:[(id:(1),name:\"old\",slots:[])])").unwrap();
    assert_eq!(old.next_colour_index, 0);
    assert_eq!(old.tracks[0].colour, Colour::UNSET);
    let mut stored = project();
    add_track(&mut stored);
    let custom = Colour {
        red: 1,
        green: 2,
        blue: 3,
    };
    stored.tracks[0].colour = custom;
    let source = ron::to_string(&stored).unwrap();
    let restored: Project = ron::from_str(&source).unwrap();
    assert_eq!(restored.tracks[0].colour, custom);
}

fn assert_no_black(p: &Project) {
    p.validate().unwrap();
    assert!(p.outputs.iter().all(|o| !o.colour.is_unset()));
    assert!(p.tracks.iter().all(|t| !t.colour.is_unset()));
    let clips = p
        .tracks
        .iter()
        .flat_map(|t| &t.slots)
        .filter_map(|s| s.clip.as_ref());
    assert!(clips.into_iter().all(|c| !c.colour.is_unset()));
}

#[test]
fn no_create_path_produces_black() {
    let mut p = project();
    let steps = vec![
        ProjectEdit::AddTrack { name: "A".into() },
        ProjectEdit::AddTrack { name: "B".into() },
        ProjectEdit::AddScene {
            at: 0,
            name: "S".into(),
        },
        ProjectEdit::CreateGroup {
            tracks: vec![TrackId(1), TrackId(2)],
        },
        ProjectEdit::CreateClip {
            track: TrackId(1),
            slot: 0,
            length: BeatTime(3840),
        },
        ProjectEdit::DuplicateClip {
            track: TrackId(1),
            slot: 0,
        },
        ProjectEdit::MoveClip {
            from: (TrackId(1), 0),
            to: (TrackId(2), 0),
        },
        ProjectEdit::AddOutput {
            name: "Out".into(),
            host: "127.0.0.1".into(),
            port: 9000,
        },
    ];
    for step in steps {
        apply(&mut p, step).unwrap();
        assert_no_black(&p);
    }
    // A pasted clip with no colour of its own takes its track's colour.
    let mut unset = Clip::new(ClipId(9), "raw", BeatTime(960), Colour::UNSET).unwrap();
    unset.colour = Colour::UNSET;
    apply(
        &mut p,
        ProjectEdit::PasteClip {
            track: TrackId(2),
            slot: 1,
            clip: Box::new(unset),
            carried: Vec::new(),
        },
    )
    .unwrap();
    assert_no_black(&p);
    // Explicit recolouring to black is refused, since Live has no black.
    let black = ProjectEdit::RecolourTrack {
        track: TrackId(1),
        colour: Colour::UNSET,
    };
    assert!(apply(&mut p, black).is_err());
}

#[test]
fn loading_fills_missing_and_explicit_black_from_the_palette() {
    let path = std::env::temp_dir().join(format!("qtable-colours-{}.ron", std::process::id()));
    let source = "#![enable(implicit_some)]\n(version: 2, project: (next_colour_index: 2,\n\
        scenes: [(id: (1), name: \"S\")],\n\
        outputs: [(id: (1), name: \"O\", host: \"127.0.0.1\", port: 9000, colour: (red: 0, green: 0, blue: 0))],\n\
        groups: [(id: (1), name: \"G\", route: Outputs([]), tracks: [(1), (2)])],\n\
        tracks: [\n\
          (id: (1), name: \"explicit\", colour: (red: 0, green: 0, blue: 0), slots: [(clip: (id: (1), name: \"c\", length: 1.0))]),\n\
          (id: (2), name: \"missing\", slots: [(clip: (id: (2), name: \"d\", length: 1.0))]),\n\
          (id: (3), name: \"chosen\", colour: (red: 1, green: 2, blue: 3), slots: [(clip: (id: (3), name: \"e\", length: 1.0))]),\n\
        ]))";
    std::fs::write(&path, source).unwrap();
    let p = qtable_engine::file::load(&path).unwrap();
    let _ = std::fs::remove_file(&path);
    assert_no_black(&p);
    // The cycle continues from the stored index: tracks first, then groups.
    assert_eq!(p[TrackId(1)].colour, LIVE_CLIP_PALETTE[2]);
    assert_eq!(p[TrackId(2)].colour, LIVE_CLIP_PALETTE[3]);
    assert_eq!(
        p.group_tracks().next().unwrap().colour,
        LIVE_CLIP_PALETTE[4]
    );
    assert_eq!(p.next_colour_index, 5);
    assert_eq!(
        p[TrackId(3)].colour,
        Colour {
            red: 1,
            green: 2,
            blue: 3
        }
    );
    for t in p.clip_tracks() {
        assert_eq!(t.slots[0].clip.as_ref().unwrap().colour, t.colour);
    }
    assert_eq!(p.outputs[0].colour, LIVE_CLIP_PALETTE[0]);
}

#[test]
fn shipped_examples_load_without_black() {
    for file in ["bopos", "follow-demo", "generic-receiver", "grid-demo"] {
        let path = format!("{}/../../examples/{file}.ron", env!("CARGO_MANIFEST_DIR"));
        assert_no_black(&qtable_engine::file::load(path).unwrap());
    }
}

/// The format-1 originals of the shipped examples stay as migration fixtures.
#[test]
fn migrated_format_1_examples_load_without_black() {
    for file in ["bopos", "follow-demo", "generic-receiver", "grid-demo"] {
        let path = format!(
            "{}/tests/fixtures/v1-example-{file}.ron",
            env!("CARGO_MANIFEST_DIR")
        );
        assert_no_black(&qtable_engine::file::load(path).unwrap());
    }
}
