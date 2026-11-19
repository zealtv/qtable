//! A version 5 file with prefixes in all three old places loads as version 6 paths and
//! composes the same addresses.
use qtable_engine::{file, model::*};
use std::fs;

fn load() -> Project {
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/v5-prefixes.ron"
    );
    file::load(fixture).unwrap()
}

#[test]
fn v5_prefixes_become_paths_and_addresses_stay_the_same() {
    let p = load();
    assert_eq!(p[TrackId(201)].path.as_str(), "/bop");
    assert_eq!(p[TrackId(10)].path.as_str(), "/kites");
    assert_eq!(p.main.path.as_str(), "/show");
    assert!(p[TrackId(202)].path.is_empty() && p[TrackId(1)].path.is_empty());
    assert_eq!(p[TrackId(201)].mixer_device.volume.output.address, "/vol",);
    let play = OscAddress::Relative("/play".into());
    let resolve = |source, destination| {
        p.resolve_event_address(TrackId(source), TrackId(destination), &play)
            .unwrap()
    };
    // The v5 rule: destination prefix, outer-to-inner group prefixes, then the address.
    assert_eq!(resolve(1, 201), "/bop/kites/play");
    assert_eq!(resolve(2, 100), "/show/play");
    assert_eq!(resolve(2, 202), "/play");
    let volume = OscAddress::Relative("/vol".into());
    assert_eq!(
        p.resolve_event_address(TrackId(201), TrackId(201), &volume)
            .unwrap(),
        "/bop/vol"
    );
}

#[test]
fn migrated_volume_addresses_include_the_tracks_own_path() {
    let p = load();
    // Main's volume self-target: its path enters once, as the destination.
    assert_eq!(
        p.volume_address(TrackId(100), TrackId(100)).unwrap(),
        "/show/mainvol"
    );
    // An intended change from v5 (`/bop/volume`): a group's own path now enters its volume
    // address (decided 2026-10-14).
    assert_eq!(
        p.volume_address(TrackId(10), TrackId(201)).unwrap(),
        "/bop/kites/volume"
    );
}

#[test]
fn a_v5_file_saves_as_v6_with_paths_and_no_old_fields() {
    let p = load();
    let file_path =
        std::env::temp_dir().join(format!("qtable-v5-paths-{}.ron", std::process::id()));
    file::save(&file_path, &p).unwrap();
    let saved = fs::read_to_string(&file_path).unwrap();
    assert!(saved.contains("version: 12"));
    for old in ["prefix", "group_prefix", "default_for_new_tracks"] {
        assert!(!saved.contains(old), "{old} still written:\n{saved}");
    }
    for path in [
        "path: \"/bop\"",
        "path: \"/kites\"",
        "path: \"/show\"",
        "address: \"/vol\"",
    ] {
        assert!(saved.contains(path), "{path} missing:\n{saved}");
    }
    assert_eq!(file::load(&file_path).unwrap(), p);
    fs::remove_file(file_path).unwrap();
}

#[test]
fn a_v6_file_with_a_bad_path_fails_on_load() {
    let p = load();
    let file_path = std::env::temp_dir().join(format!("qtable-v6-bad-{}.ron", std::process::id()));
    file::save(&file_path, &p).unwrap();
    let text = fs::read_to_string(&file_path)
        .unwrap()
        .replace("path: \"/kites\"", "path: \"kites\"");
    fs::write(&file_path, text).unwrap();
    let error = file::load(&file_path).unwrap_err().to_string();
    assert!(error.contains("path must start with a slash"), "{error}");
    fs::remove_file(file_path).unwrap();
}
