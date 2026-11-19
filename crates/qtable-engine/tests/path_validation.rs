use qtable_engine::edit::{self, LaneEdit, ProjectEdit};
use qtable_engine::engine::{Command, Engine};
use qtable_engine::{file, model::*};
use std::fs;

const FORBIDDEN: [char; 9] = [' ', '#', '*', ',', '?', '[', ']', '{', '}'];

fn project() -> Project {
    let mut p = file::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/v5-prefixes.ron"
    ))
    .unwrap();
    let mut clip = Clip::new(
        ClipId(1),
        "Cue",
        BeatTime::from_beats(4).unwrap(),
        LIVE_CLIP_PALETTE[0],
    )
    .unwrap();
    let play = OscAddress::Relative("/play".into());
    clip.events
        .push(p.hit(TrackId(1), BeatTime::ZERO, play, vec![]).unwrap());
    p.tracks
        .iter_mut()
        .find(|t| t.id == TrackId(1))
        .unwrap()
        .slots[0]
        .clip = Some(clip);
    p
}

#[test]
fn every_forbidden_character_is_rejected_in_a_part_and_all_address_fields() {
    for character in FORBIDDEN.into_iter().chain(['/']) {
        let part = format!("a{character}b");
        assert!(validate_address_part(&part).is_err(), "{character:?}");
    }
    for character in FORBIDDEN {
        let invalid = format!("/a{character}b/child");
        assert!(Path::new(&invalid).is_err());
        assert!(validate_address(&invalid).is_err());
        let mut p = project();
        let original = p.clone();
        for id in [TrackId(1), TrackId(10), TrackId(201), p.main.id] {
            let error = edit::apply(
                &mut p,
                ProjectEdit::SetTrackPath {
                    track: id,
                    path: invalid.clone(),
                },
            )
            .unwrap_err()
            .to_string();
            assert!(error.contains("cannot contain"), "{error}");
            assert_eq!(p, original);
            let mut output = p.track(id).unwrap().mixer_device.volume.output.clone();
            output.address = invalid.clone();
            assert!(
                edit::apply(
                    &mut p,
                    ProjectEdit::SetParameterOutput {
                        target: qtable_engine::model::ParameterRef::volume(id),
                        output
                    }
                )
                .is_err()
            );
            assert_eq!(p, original);
        }
        assert!(
            edit::apply(
                &mut p,
                ProjectEdit::EditLane {
                    track: TrackId(1),
                    slot: 0,
                    edit: LaneEdit::AddPath(invalid.clone()),
                }
            )
            .is_err()
        );
        assert!(
            edit::apply(
                &mut p,
                ProjectEdit::EditLane {
                    track: TrackId(1),
                    slot: 0,
                    edit: LaneEdit::SetPath {
                        lane: 0,
                        path: invalid.clone()
                    },
                }
            )
            .is_err()
        );
        // A hit can only play an event its track defines.
        let mut stray = p[TrackId(1)].slots[0].clip.as_ref().unwrap().events[0].clone();
        stray.definition.key = EntryKey("stray".into());
        assert!(
            edit::apply(
                &mut p,
                ProjectEdit::AddEvent {
                    track: TrackId(1),
                    slot: 0,
                    event: stray,
                    carried: None,
                }
            )
            .is_err()
        );
        assert_eq!(p, original);
    }

    for valid in ["/a/b", "/kick-1/volume_db", "/a//b"] {
        assert!(Path::new(valid).is_ok());
        assert!(validate_address(valid).is_ok());
    }
}

#[test]
fn old_v5_and_v6_values_load_round_trip_and_can_be_repaired_independently() {
    let path =
        std::env::temp_dir().join(format!("qtable-address-compat-{}.ron", std::process::id()));
    let v5 = fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/v5-prefixes.ron"
    ))
    .unwrap();
    let clip = r#"(id: (1), name: "Cue", length: 4.0, events: [
        (position: 0.0, address: Relative("/play")),
    ])"#;
    let v5 = v5.replace(
        "group: (10), slots: [()]",
        &format!("group: (10), slots: [(clip: {clip})]"),
    );
    for character in FORBIDDEN {
        let source = v5
            .replace("/bop", &format!("/bo{character}p"))
            .replace("/kites", &format!("/ki{character}tes"))
            .replace("/show", &format!("/sho{character}w"))
            .replace("/play", &format!("/pla{character}y"))
            .replace("/vol\"", &format!("/vo{character}l\""));
        fs::write(&path, source).unwrap();
        let mut p = file::load(&path).unwrap();
        // Version 6 added ordinary-track paths and explicit empty lanes as well.
        p.tracks
            .iter_mut()
            .find(|t| t.id == TrackId(1))
            .unwrap()
            .path = ron::from_str(&format!("\"/track{character}path\"")).unwrap();
        let empty = OscAddress::Absolute(format!("/empty{character}lane"));
        let definition = p.ensure_event(TrackId(1), empty, &[]).unwrap();
        let clip = p
            .tracks
            .iter_mut()
            .find(|t| t.id == TrackId(1))
            .unwrap()
            .slots[0]
            .clip
            .as_mut()
            .unwrap();
        clip.lanes
            .push(qtable_engine::model::Lane::Events(EventLane { definition }));
        assert!(p.validate().is_err());
        file::save(&path, &p).unwrap();
        assert_eq!(file::load(&path).unwrap(), p);
        let mut engine = Engine::new(p).unwrap();
        engine
            .execute(Command::Edit(ProjectEdit::SetTrackPath {
                track: TrackId(201),
                path: "/fixed".into(),
            }))
            .unwrap();
        engine
            .execute(Command::Edit(ProjectEdit::RenameTrack {
                track: TrackId(1),
                name: "Still editable".into(),
            }))
            .unwrap();
        engine
            .execute(Command::Edit(ProjectEdit::EditLane {
                track: TrackId(1),
                slot: 0,
                edit: LaneEdit::SetPath {
                    lane: 0,
                    path: "/fixed".into(),
                },
            }))
            .unwrap();
        assert_eq!(engine.project()[TrackId(201)].path.as_str(), "/fixed");
    }
    fs::remove_file(path).unwrap();
}
