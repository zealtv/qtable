//! Frozen v3.2 format: keep this fixture independent of the current model's defaults.
use super::*;
use crate::model::*;

fn temporary(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("qtable-{name}-{}.ron", std::process::id()))
}

fn fixture() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/v7-targets.ron")
}

#[test]
fn loads_frozen_v7_targets_lanes_destinations_and_group() {
    let project = load(fixture()).unwrap();
    project.validate().unwrap();
    let group = &project.tracks[0];
    let track = &project.tracks[1];
    assert!(group.is_group());
    assert_eq!(track.group, Some(group.id));
    assert_eq!(track.route, Route::Group);
    assert_eq!(
        group.route,
        Route::Outputs(vec![TrackId(201), project.main.id])
    );
    assert_eq!(project.scenes.len(), 2);
    assert!(track.slots[1].clip.is_none());
    assert!(!track.slots[1].has_stop_button);
    let target = track.devices[0].target().unwrap();
    assert_eq!(target.path.as_str(), "/target");
    let fields = &target.events[0].fields;
    assert_eq!(
        fields.iter().map(|f| f.control).collect::<Vec<_>>(),
        vec![
            Control::NumberBox,
            Control::Slider,
            Control::Dial,
            Control::Enum,
            Control::Toggle,
            Control::Text,
        ]
    );
    assert_eq!(fields[5].text.as_deref(), Some("current cue"));
    assert_eq!(fields[5].default, OscArgument::String("default cue".into()));
    assert_eq!(target.parameters[0].send_mode, SendMode::OnRelease);
    assert_eq!(target.parameters[0].fields[1].value, Some(0.25));
    assert_eq!(target.parameters[0].fields[1].threshold, 0.01);
    assert_eq!(target.parameters[0].fields[2].value, Some(0.75));
    assert_eq!(fields[3].options.len(), 2);
    assert!(target.events[1].fields.is_empty());
    let clip = track.slots[0].clip.as_ref().unwrap();
    assert_eq!(clip.lanes.len(), 2);
    assert_eq!(clip.events.len(), 2);
    for (index, event) in clip.events.iter().enumerate() {
        assert_eq!(
            event.definition,
            clip.lanes[index].event().unwrap().definition
        );
        assert_eq!(event.definition.device, track.devices[0].id);
        assert_eq!(event.definition.key, target.events[index].key);
    }
    assert_eq!(
        clip.events[0].arguments,
        vec![
            OscArgument::Int(0),
            OscArgument::Float(0.0),
            OscArgument::Float(0.0),
            OscArgument::String("One".into()),
            OscArgument::Bool(false),
            OscArgument::String("default cue".into()),
        ]
    );
    assert_eq!(clip.events[0].position, BeatTime::ZERO);
    assert_eq!(clip.events[1].position, BeatTime::from_beats(1).unwrap());
    assert_eq!(target.events[1].path, OscAddress::Absolute("/go".into()));
    assert!(clip.events[1].arguments.is_empty());
    let stage = project.outputs[0].output_stage().unwrap();
    assert_eq!(
        *stage,
        OutputStage {
            frame_rate: 44.0,
            budget_on: true,
            budget: 600,
            bundle: true,
            refresh_on: true,
            refresh: 0.8,
        }
    );
    assert_eq!(project.main.output_stage(), Some(stage));
    assert_eq!(project.main.output.as_ref().unwrap().destination.port, 9002);
    assert_eq!(project.main.mixer_device.volume.value, 0.75);
}

#[test]
fn frozen_v7_save_round_trip_is_byte_stable() {
    let path = temporary("v7-byte-stable");
    let project = load(fixture()).unwrap();
    save(&path, &project).unwrap();
    let saved = fs::read(&path).unwrap();
    let reloaded = load(&path).unwrap();
    assert_eq!(reloaded, project);
    save(&path, &reloaded).unwrap();
    assert_eq!(fs::read(&path).unwrap(), saved);
    fs::remove_file(path).unwrap();
}

#[test]
fn save_writes_exactly_one_implicit_some_header() {
    let path = temporary("single-header");
    save(&path, &Project::default()).unwrap();
    let bytes = fs::read(&path).unwrap();
    assert!(bytes.starts_with(b"#![enable(implicit_some)]\n(\n    version: 12,\n"));
    assert_eq!(
        String::from_utf8(bytes)
            .unwrap()
            .matches("#![enable(implicit_some)]")
            .count(),
        1
    );
    fs::remove_file(path).unwrap();
}

#[test]
fn loads_v7_with_the_duplicate_header_written_by_older_builds() {
    let path = temporary("duplicate-header");
    let source = fs::read_to_string(fixture()).unwrap();
    fs::write(&path, format!("#![enable(implicit_some)]\n{source}")).unwrap();
    assert_eq!(load(&path).unwrap(), load(fixture()).unwrap());
    save(&path, &load(&path).unwrap()).unwrap();
    assert_eq!(load(&path).unwrap(), load(fixture()).unwrap());
    fs::remove_file(path).unwrap();
}
