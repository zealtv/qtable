use qtable_engine::{file, model::*};
use std::fs;

fn path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("qtable-output-{name}-{}.ron", std::process::id()))
}

#[test]
fn v1_shared_destination_targets_and_group_routes_migrate_and_round_trip() {
    let path = path("migration");
    fs::write(&path, r#"#![enable(implicit_some)]
(version: 1, project: Project(
    destinations: [Destination(id: DestinationId(8), name: "Receiver", host: "127.0.0.1", port: 9000, mode: Broadcast)],
    targets: [Target(id: TargetId(1), name: "Stage Lights", destination: DestinationId(8), prefix: "/lights"),
              Target(id: TargetId(2), name: "Media", destination: DestinationId(8), prefix: "/media")],
    tracks: [Track(id: TrackId(1), name: "Inherited", slots: []),
             Track(id: TrackId(2), name: "Override", target: TargetId(2), slots: [])],
    groups: [Group(id: GroupId(101), name: "Group", default_target: TargetId(1), tracks: [TrackId(1), TrackId(2)])],
))"#).unwrap();
    let mut p = file::load(&path).unwrap();
    assert_eq!(p.outputs.len(), 2);
    assert_eq!(
        p.outputs[0].destination_mut().unwrap().mode,
        UdpMode::Broadcast
    );
    assert_eq!(p.outputs[0].destination_mut().unwrap().host, "127.0.0.1");
    assert_eq!(p.outputs[0].destination_mut().unwrap().port, 9000);
    assert_eq!(p.outputs[1].path.as_str(), "/media");
    assert!(
        p.outputs
            .iter()
            .all(|o| o.mixer.active && o.delay_ms == 0.0)
    );
    assert_eq!(p[TrackId(1)].route, Route::Group);
    // Target ids 1 and 2 collide with the tracks' ids, so loading renumbers the outputs above
    // every id: tracks 1 and 2, the migrated group track 3, then outputs 4 and 5.
    let (lights, media) = (p.outputs[0].id, p.outputs[1].id);
    assert_eq!([lights, media], [TrackId(4), TrackId(5)]);
    assert_eq!(p.resolved_route(TrackId(1)), [lights]);
    assert_eq!(p.resolved_route(TrackId(2)), [media]);
    p.outputs[0].delay_ms = -50.0;
    p.outputs[1].mixer.active = false;
    p.tracks[1].route = Route::Outputs(vec![media, lights]);
    file::save(&path, &p).unwrap();
    let saved = fs::read_to_string(&path).unwrap();
    assert!(saved.contains("version: 12"));
    assert!(!saved.contains("destinations:"));
    assert!(!saved.contains("targets:"));
    assert_eq!(file::load(&path).unwrap(), p);
    fs::remove_file(path).unwrap();
}

#[test]
fn v2_handwritten_outputs_derive_colour_and_optional_defaults() {
    let path = path("defaults");
    fs::write(&path, r#"(version: 2, project: Project(outputs: [Output(id: OutputId(201), name: "Stage Lights", host: "localhost", port: 9000)]))"#).unwrap();
    let p = file::load(&path).unwrap();
    assert!(LIVE_CLIP_PALETTE.contains(&p.outputs[0].colour));
    assert!(p.outputs[0].mixer.active);
    assert!(p.main.output.is_none());
    fs::remove_file(path).unwrap();
}

#[test]
fn old_v4_tags_are_ignored_and_not_saved() {
    for (case, tags) in [("duplicate", ["LX", "LX"]), ("blank", ["", " "])] {
        let path = path(case);
        let old = format!(
            r#"(version: 4, project: Project(outputs: [
            Track(id: TrackId(201), kind: Output, name: "Lights", slots: [], output: Some((short_tag: "{}", destination: (host: "localhost", port: 9000)))),
            Track(id: TrackId(202), kind: Output, name: "Video", slots: [], output: Some((short_tag: "{}", destination: (host: "localhost", port: 9001))))
        ]))"#,
            tags[0], tags[1]
        );
        fs::write(&path, old).unwrap();
        let p = file::load(&path).unwrap();
        assert_eq!(p.outputs[0].name, "Lights");
        assert_eq!(p.outputs[1].destination().unwrap().port, 9001);
        file::save(&path, &p).unwrap();
        assert!(!fs::read_to_string(&path).unwrap().contains("short_tag"));
        assert_eq!(file::load(&path).unwrap(), p);
        fs::remove_file(path).unwrap();
    }
}

#[test]
fn old_flat_output_tags_are_ignored_in_v2_and_v3() {
    for version in [2, 3] {
        let path = path(&format!("flat-{version}"));
        fs::write(&path, format!(r#"(version: {version}, project: Project(outputs: [
            Output(id: OutputId(201), name: "Lights", host: "localhost", port: 9000, short_tag: ""),
            Output(id: OutputId(202), name: "Video", host: "localhost", port: 9001, short_tag: "LX"),
            Output(id: OutputId(203), name: "Stage", host: "localhost", port: 9002, short_tag: "LX")
        ]))"#)).unwrap();
        let project = file::load(&path).unwrap();
        assert_eq!(project.outputs.len(), 3);
        assert_eq!(project.outputs[2].name, "Stage");
        file::save(&path, &project).unwrap();
        assert!(!fs::read_to_string(&path).unwrap().contains("short_tag"));
        assert_eq!(file::load(&path).unwrap(), project);
        fs::remove_file(path).unwrap();
    }
}
