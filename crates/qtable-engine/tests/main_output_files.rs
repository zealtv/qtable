use qtable_engine::{file, model::*};
use std::fs;

#[test]
fn v4_main_migrates_without_a_destination_and_round_trips_as_v6() {
    let path = std::env::temp_dir().join(format!("qtable-main-v4-{}.ron", std::process::id()));
    fs::write(
        &path,
        r#"#![enable(implicit_some)]
(version: 4, project: Project(
    main: (delay_ms: 5.0),
    tracks: [Track(id: TrackId(1), name: "Cue", route: Outputs([TrackId(2)]), slots: [])],
    outputs: [Track(id: TrackId(2), kind: Output, name: "Lights", slots: [],
        output: (default_for_new_tracks: true, destination: (host: "localhost", port: 9001)))],
))"#,
    )
    .unwrap();
    let p = file::load(&path).unwrap();
    assert_eq!(p.main.id, TrackId(3));
    assert_eq!(p.main.kind, TrackKind::Main);
    assert_eq!(p.main.delay_ms, 5.0);
    assert!(p.main.output.is_none());
    assert_eq!(p.tracks[0].route, Route::single(TrackId(2)));
    file::save(&path, &p).unwrap();
    let saved = fs::read_to_string(&path).unwrap();
    assert!(saved.contains("version: 12") && saved.contains("kind: Main"));
    assert!(!saved.contains("default_for_new_tracks"));
    assert_eq!(file::load(&path).unwrap(), p);
    fs::remove_file(path).unwrap();
}

#[test]
fn main_cannot_be_removed_or_placed() {
    use qtable_engine::edit::{ProjectEdit, apply};
    let mut p = Project::default();
    p.tracks.push(Track::new(TrackId(1), "Cue", 0));
    let before = p.clone();
    let id = p.main.id;
    assert!(apply(&mut p, ProjectEdit::RemoveTracks { tracks: vec![id] }).is_err());
    assert!(
        apply(
            &mut p,
            ProjectEdit::PlaceTrack {
                track: id,
                before: None,
                group: None
            }
        )
        .is_err()
    );
    assert_eq!(p, before);
}

#[test]
fn defaults_use_main_and_invalid_destinations_are_rejected_atomically() {
    use qtable_engine::edit::{ProjectEdit, apply};
    let mut p = Project::default();
    for name in ["A", "B"] {
        apply(&mut p, ProjectEdit::AddTrack { name: name.into() }).unwrap();
    }
    assert!(p.tracks.iter().all(|t| t.route == Route::single(p.main.id)));
    let members = p.tracks.iter().map(|t| t.id).collect();
    apply(&mut p, ProjectEdit::CreateGroup { tracks: members }).unwrap();
    assert_eq!(p.tracks[0].route, Route::single(p.main.id));
    let mut p2 = p.clone();
    p2.tracks[1].route = Route::Group;
    assert_eq!(p2.resolved_route(p2.tracks[1].id), [p2.main.id]);
    let before = p.clone();
    let clip_track = p.tracks[1].id;
    assert!(
        apply(
            &mut p,
            ProjectEdit::SetTrackRoutes {
                routes: vec![(clip_track, Route::single(clip_track))]
            }
        )
        .is_err()
    );
    assert_eq!(p, before);
    let mut main = p.main.clone();
    main.destination_mut().unwrap().port = 9100;
    apply(
        &mut p,
        ProjectEdit::SetDestination {
            output: main.id,
            destination: main.output,
        },
    )
    .unwrap();
    assert_eq!(p.main.destination().unwrap().port, 9100);
}

#[test]
fn v5_file_with_another_main_colour_and_name_loads_normalised() {
    let mut p = Project::default();
    let path = std::env::temp_dir().join(format!("qtable-main-colour-{}.ron", std::process::id()));
    file::save(&path, &p).unwrap();
    let saved = fs::read_to_string(&path).unwrap();
    let old = format!("red: {}", Main::COLOUR.red);
    assert!(saved.contains(&old));
    fs::write(
        &path,
        saved
            .replacen(&old, "red: 128", 1)
            .replacen("\"Main\"", "\"Old\"", 1),
    )
    .unwrap();
    let loaded = file::load(&path).unwrap();
    assert_eq!(loaded.main.colour, Main::COLOUR);
    assert_eq!(loaded.main.name, Main::NAME);
    p.normalise_main();
    assert_eq!(loaded, p);
    fs::remove_file(path).unwrap();
}

#[test]
fn main_refuses_rename_recolour_and_switches_by_capability() {
    use qtable_engine::edit::{ProjectEdit, apply};
    let mut p = Project::default();
    p.tracks.push(Track::new(TrackId(1), "Cue", 0));
    let before = p.clone();
    let main = p.main.id;
    let cue = TrackId(1);
    let edits = [
        ProjectEdit::RenameTrack {
            track: main,
            name: "X".into(),
        },
        ProjectEdit::RecolourTrack {
            track: main,
            colour: Main::COLOUR,
        },
        ProjectEdit::SetActive {
            strips: vec![cue, main],
            active: false,
        },
        ProjectEdit::SetSolo {
            strips: vec![cue, main],
            solo: true,
            exclusive: false,
        },
        ProjectEdit::SetSolo {
            strips: vec![main],
            solo: true,
            exclusive: true,
        },
    ];
    for edit in edits {
        let error = apply(&mut p, edit).unwrap_err();
        assert!(
            error.to_string().contains("cannot be") || error.to_string().contains("no Activator")
        );
        assert_eq!(p, before);
    }
}

#[test]
fn set_track_delay_accepts_main_and_output_is_not_main() {
    use qtable_engine::edit::ProjectEdit;
    use qtable_engine::engine::{Command, Engine};
    let mut engine = Engine::new(Project::default()).unwrap();
    let main = engine.project().main.id;
    engine
        .execute(Command::Edit(ProjectEdit::SetTrackDelay {
            track: main,
            delay_ms: 7.0,
        }))
        .unwrap();
    assert_eq!(engine.project().main.delay_ms, 7.0);
    assert!(engine.project().output(main).is_none());
    assert!(engine.project().destination(main).is_some());
}

#[test]
fn main_has_no_route_to_edit() {
    use qtable_engine::edit::{ProjectEdit, apply};
    let mut p = Project::default();
    p.tracks.push(Track::new(TrackId(1), "Cue", 0));
    let before = p.clone();
    let main = p.main.id;
    let edits = [
        ProjectEdit::SetTrackRoutes {
            routes: vec![(main, Route::single(TrackId(1)))],
        },
        ProjectEdit::ToggleOutputInRoute {
            tracks: vec![main],
            output: main,
        },
    ];
    for edit in edits {
        assert!(apply(&mut p, edit).is_err());
        assert_eq!(p, before);
    }
}

fn load_v4(name: &str, body: &str) -> Project {
    let path = std::env::temp_dir().join(format!("qtable-{name}-{}.ron", std::process::id()));
    fs::write(&path, body).unwrap();
    let p = file::load(&path).unwrap();
    fs::remove_file(path).unwrap();
    p
}

#[test]
fn v4_output_id_colliding_with_a_track_is_renumbered_and_main_lands_above_it() {
    let p = load_v4(
        "collide",
        r#"#![enable(implicit_some)]
(version: 4, project: Project(
    tracks: [Track(id: TrackId(1), name: "Cue", route: Outputs([TrackId(1)]), slots: [])],
    outputs: [Track(id: TrackId(1), kind: Output, name: "Lights", slots: [],
        output: (default_for_new_tracks: false, destination: (host: "localhost", port: 9001)))],
))"#,
    );
    let output = p.outputs[0].id;
    assert_ne!(output, p.tracks[0].id);
    assert!(p.main.id.0 > output.0 && p.main.id.0 > p.tracks[0].id.0);
    assert!(p.main.output.is_none());
}

#[test]
fn an_empty_v4_project_gives_main_the_first_id() {
    let p = load_v4(
        "empty",
        "#![enable(implicit_some)]\n(version: 4, project: Project())",
    );
    assert_eq!(p.main.id, TrackId(1));
    assert!(p.main.output.is_none());
}
