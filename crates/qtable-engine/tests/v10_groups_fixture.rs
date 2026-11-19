//! A project saved by the pre-bump format-10 writer, with flat groups and remote mappings
//! (saved 2026-11-06). The file was emitted by the v3.6 writer, not
//! constructed as RON, and stays frozen. Its expectations were recorded before format 11
//! existed and must hold after the upgrade.
use qtable_engine::{file, model::*};
use std::path::PathBuf;

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/v10-groups-mappings.ron")
}

fn t(id: u64) -> TrackId {
    TrackId(id)
}

#[test]
fn frozen_v10_fixture_was_written_by_the_format_10_writer() {
    let text = std::fs::read_to_string(fixture_path()).unwrap();
    assert!(text.starts_with("#![enable(implicit_some)]\n(\n    version: 10,\n"));
}

#[test]
fn loads_flat_groups_with_unchanged_ids_order_folds_and_slots() {
    let p = file::load(fixture_path()).unwrap();
    p.validate().unwrap();
    let ids: Vec<_> = p.tracks.iter().map(|t| t.id.0).collect();
    assert_eq!(ids, [10, 1, 2, 3, 11, 4]);
    assert_eq!(p.scenes.len(), 3);
    let parents: Vec<_> = p.tracks.iter().map(|t| t.group.map(|g| g.0)).collect();
    assert_eq!(parents, [None, Some(10), Some(10), None, None, Some(11)]);
    assert!(p[t(10)].is_group() && p[t(10)].fold_state);
    assert!(p[t(11)].is_group() && !p[t(11)].fold_state);
    assert_eq!(p.members(t(10)).map(|t| t.id.0).collect::<Vec<_>>(), [1, 2]);
    assert_eq!(p.members(t(11)).map(|t| t.id.0).collect::<Vec<_>>(), [4]);
    // Empty slots keep their positions: they decide follow-action groups.
    let filled = |id| -> Vec<bool> {
        p[t(id)]
            .slots
            .iter()
            .map(|slot| slot.clip.is_some())
            .collect()
    };
    assert_eq!(filled(1), [true, false, false]);
    assert_eq!(filled(2), [false, false, true]);
    assert_eq!(filled(3), [false, true, false]);
    assert!(!p[t(1)].slots[1].has_stop_button);
    assert!(
        p.group_tracks()
            .all(|g| g.slots.iter().all(|s| s.clip.is_none()))
    );
}

#[test]
fn loads_routes_paths_and_delays_unchanged() {
    let p = file::load(fixture_path()).unwrap();
    let (lights, sound) = (t(201), t(202));
    assert_eq!(p[t(10)].route, Route::single(lights));
    assert_eq!(p[t(1)].route, Route::Group);
    assert_eq!(p[t(2)].route, Route::single(sound));
    assert_eq!(p[t(3)].route, Route::Outputs(vec![lights, sound]));
    assert_eq!(p[t(11)].route, Route::single(sound));
    assert_eq!(p[t(4)].route, Route::Group);
    assert_eq!(p.resolved_route(t(1)), [lights]);
    assert_eq!(p.resolved_route(t(2)), [sound]);
    assert_eq!(p.resolved_route(t(3)), [lights, sound]);
    assert_eq!(p.resolved_route(t(4)), [sound]);
    // Track, group and Main delays add.
    let delay: Vec<_> = [1, 2, 3, 4].map(|id| p.effective_delay_ms(t(id))).to_vec();
    assert_eq!(delay, [10.5, 4.0, 3.0, 3.0]);
    assert_eq!(p.main.delay_ms, 3.0);
    assert_eq!(p.tempo.bpm(), 96.0);
    let paths = |tracks: &[Track]| -> Vec<String> {
        tracks.iter().map(|t| t.path.as_str().to_owned()).collect()
    };
    assert_eq!(paths(&p.outputs), ["/lights", "/sound"]);
    assert_eq!(paths(&p.tracks), ["/drums", "/kick", "", "/solo", "", ""]);
    assert_eq!(p.main.path.as_str(), "/main");
}

#[test]
fn loads_mappings_with_ids_sources_ranges_modes_and_takeover() {
    let p = file::load(fixture_path()).unwrap();
    let volume = |track| ControlRef::Argument(FieldRef::volume(t(track)));
    let rows: Vec<_> = p
        .mappings
        .iter()
        .map(|m| (m.id.0, m.control.clone()))
        .collect();
    assert_eq!(
        rows,
        [
            (1, volume(1)),
            (2, volume(10)),
            (3, volume(3)),
            (4, volume(2))
        ]
    );
    let cc = Source::Midi(MidiSource {
        channel: 1,
        message: MidiMessage::Cc(21),
    });
    // One source fans out to a track and a group.
    assert_eq!(p.mappings[0].source, cc);
    assert_eq!(p.mappings[1].source, cc);
    assert_eq!(
        p.mappings[2].source,
        Source::Midi(MidiSource {
            channel: 3,
            message: MidiMessage::Note(60)
        })
    );
    assert_eq!(
        p.mappings[3].source,
        Source::Key(KeySource {
            key: KeyName("k".into())
        })
    );
    assert_eq!((p.mappings[0].min, p.mappings[0].max), (0.9, 0.1));
    assert_eq!(
        p.mappings[0].mode,
        SourceMode::Relative(RelativeEncoding::TwosComplement)
    );
    assert_eq!(p.mappings[0].takeover, Takeover::PickUp);
    assert_eq!(p.mappings[1].mode, SourceMode::Absolute);
    assert_eq!(p.mappings[1].takeover, Takeover::None);
}

fn temp(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("qtable-v10-{name}-{}.ron", std::process::id()))
}

#[test]
fn upgrade_to_the_current_format_changes_only_the_version_header() {
    let fixture = std::fs::read_to_string(fixture_path()).unwrap();
    let project = file::load(fixture_path()).unwrap();
    let path = temp("identity");
    file::save(&path, &project).unwrap();
    let saved = std::fs::read_to_string(&path).unwrap();
    // Nothing is renumbered, reordered or rewritten: the writer emits the same bytes
    // for the same model, so the saved set is the fixture with a new version.
    assert_eq!(saved, fixture.replacen("version: 10", "version: 12", 1));
    assert_eq!(file::load(&path).unwrap(), project);
    file::save(&path, &file::load(&path).unwrap()).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), saved);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn format_11_flat_sets_keep_flat_groups_through_a_round_trip() {
    let project = file::load(fixture_path()).unwrap();
    let path = temp("flat-11");
    file::save(&path, &project).unwrap();
    let reloaded = file::load(&path).unwrap();
    assert_eq!(reloaded, project);
    assert!(reloaded.group_tracks().all(|g| g.group.is_none()));
    assert_eq!(reloaded.mappings.len(), 4);
    assert_eq!(reloaded.mappings, project.mappings);
    std::fs::remove_file(path).unwrap();
}

/// The fixture with Pads (11) moved into Drums (10), as `Drums(Kick, Snare, Pads(Pad)), Solo`.
fn nested() -> Project {
    let mut project = file::load(fixture_path()).unwrap();
    let mut rest = project.tracks.split_off(3);
    let solo = rest.remove(0);
    project.tracks.extend(rest);
    project.tracks.push(solo);
    project[t(11)].group = Some(t(10));
    project
}

fn body(project: &Project) -> String {
    ron::ser::to_string_pretty(project, ron::ser::PrettyConfig::default()).unwrap()
}

#[test]
fn nesting_is_rejected_in_a_v10_file_and_accepted_in_the_current_format() {
    let project = nested();
    let ids: Vec<_> = project.tracks.iter().map(|t| t.id.0).collect();
    assert_eq!(ids, [10, 1, 2, 11, 4, 3]);
    // The nested set is a valid forest: only the flat group rules reject it.
    assert!(Topology::build(&project.tracks).is_ok());
    let path = temp("nested");
    file::save(&path, &project).unwrap();
    assert_eq!(file::load(&path).unwrap(), project);
    for version in [10, 11, 12] {
        std::fs::write(
            &path,
            format!("(version: {version}, project: {})", body(&project)),
        )
        .unwrap();
        if version == 10 {
            assert!(matches!(file::load(&path), Err(file::FileError::Model(_))));
        } else {
            assert_eq!(file::load(&path).unwrap(), project);
        }
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn older_files_keep_the_flat_rule_at_the_same_boundary() {
    // The same nested track list in a v9 container never reaches the current model.
    let mut project = nested();
    // A v9 file has no mapping list, and an empty one is not written.
    project.mappings.clear();
    let path = temp("nested-v9");
    let v9 = body(&project);
    std::fs::write(&path, format!("(version: 9, project: {v9})")).unwrap();
    assert!(matches!(file::load(&path), Err(file::FileError::Model(_))));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn every_frozen_fixture_upgrades_to_the_current_format_and_round_trips() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut count = 0;
    for entry in std::fs::read_dir(&root).unwrap() {
        let source = entry.unwrap().path();
        if source.extension().is_none_or(|e| e != "ron") {
            continue;
        }
        let original = std::fs::read(&source).unwrap();
        let modified = std::fs::metadata(&source).unwrap().modified().unwrap();
        let project = file::load(&source).unwrap();
        assert_eq!(std::fs::read(&source).unwrap(), original);
        assert_eq!(
            std::fs::metadata(&source).unwrap().modified().unwrap(),
            modified
        );
        project.validate().unwrap();
        let path = temp("every");
        file::save(&path, &project).unwrap();
        let saved = std::fs::read_to_string(&path).unwrap();
        assert!(saved.contains("version: 12"), "{}", source.display());
        assert_eq!(file::load(&path).unwrap(), project, "{}", source.display());
        // The save is stable, and no earlier file is touched by reading it.
        file::save(&path, &file::load(&path).unwrap()).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), saved);
        assert_eq!(std::fs::read(&source).unwrap(), original);
        assert_eq!(
            std::fs::metadata(&source).unwrap().modified().unwrap(),
            modified
        );
        println!(
            "unchanged read and current-format round-trip: {}",
            source.display()
        );
        std::fs::remove_file(path).unwrap();
        count += 1;
    }
    assert!(
        count >= 13,
        "expected every historical fixture, found {count}"
    );
}

#[test]
fn a_format_10_reader_refuses_a_format_11_file_by_version() {
    let project = file::load(fixture_path()).unwrap();
    let path = temp("old-reader");
    // The current writer saves format 12; this fixture holds nothing format 12 adds, so with the
    // header of format 11 it is the file a v3.9 writer saved.
    file::save(&path, &project).unwrap();
    let saved = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, saved.replacen("version: 12", "version: 11", 1)).unwrap();
    // Optional real pre-bump reader: a v3.6.0 `qtable play` binary, whose `file.rs` is the
    // format-10 reader. It must refuse by version, not by judging the project.
    if let Some(reader) = std::env::var_os("QTABLE_FORMAT10_READER") {
        let output = std::process::Command::new(reader)
            .arg("play")
            .arg(&path)
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap();
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr
                .contains("unsupported project format version 11; this build supports version 10"),
            "{stderr}"
        );
        println!("pre-format-11 reader refused version 11");
    }
    std::fs::remove_file(path).unwrap();
}
