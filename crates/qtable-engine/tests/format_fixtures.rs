//! Historical input bytes stay frozen when the current format advances.
use qtable_engine::{file, model::*};
use std::{fs, path::PathBuf};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/v8-envelopes.ron")
}

#[test]
fn loads_frozen_v8_mixed_lanes_and_envelope_domains() {
    let project = file::load(fixture()).unwrap();
    project.validate().unwrap();
    let track = &project.tracks[1];
    let clip = track.slots[0].clip.as_ref().unwrap();
    assert_eq!(track.group, Some(project.tracks[0].id));
    assert!(track.slots[1].clip.is_none());
    assert!(!track.slots[1].has_stop_button);
    assert_eq!(clip.length(), BeatTime::from_beats(4).unwrap());
    assert!(clip.looping);
    assert_eq!(clip.start, BeatTime::ZERO);
    assert_eq!(clip.loop_start, BeatTime::ZERO);
    assert_eq!(clip.end, clip.length());
    assert_eq!(clip.loop_end, clip.length());
    assert!(
        clip.lanes
            .iter()
            .filter_map(Lane::envelope)
            .all(|e| e.region.is_none())
    );
    assert_eq!(clip.lanes.len(), 6);
    assert_eq!(clip.events.len(), 2);
    for (lane, event) in [0, 2].into_iter().zip(&clip.events) {
        assert_eq!(
            clip.lanes[lane].event().unwrap().definition,
            event.definition
        );
    }
    assert_eq!(clip.events[0].arguments[4], OscArgument::Bool(false));
    assert!(clip.events[1].arguments.is_empty());

    // Pin identities, order, closed-timeline endpoints, coincident jump points,
    // curvature (including omitted zeroes), and discrete control-unit values.
    for (index, device, field, axis, points) in [
        (
            1,
            DeviceRef::Mixer,
            "value",
            Axis::Continuous,
            vec![
                (0.0, 0.25, 0.5),
                (2.0, 0.75, 0.0),
                (2.0, 0.5, -0.5),
                (4.0, 1.0, 0.0),
            ],
        ),
        (
            3,
            DeviceRef::Chain(DeviceId(1)),
            "toggle",
            Axis::Discrete,
            vec![(0.0, 0.0, 0.0), (1.0, 1.0, 0.0), (3.0, 0.0, 0.0)],
        ),
        (
            4,
            DeviceRef::Chain(DeviceId(1)),
            "choice",
            Axis::Discrete,
            vec![(0.0, 0.0, 0.0), (1.5, 1.0, 0.0), (4.0, 0.0, 0.0)],
        ),
        (
            5,
            DeviceRef::Chain(DeviceId(1)),
            "number",
            Axis::Stepped,
            vec![(0.0, 0.0, -0.25), (2.0, 1.0, 0.0), (4.0, 0.0, 0.0)],
        ),
    ] {
        let envelope = clip.lanes[index].envelope().unwrap();
        assert_eq!(
            envelope.target,
            ArgumentRef {
                device,
                parameter: ParameterKey(
                    if device == DeviceRef::Mixer {
                        "volume"
                    } else {
                        "values"
                    }
                    .into()
                ),
                field: FieldKey(field.into()),
            }
        );
        assert_eq!(track.argument_axis(&envelope.target), Some(axis));
        let expected: Vec<_> = points
            .into_iter()
            .map(|(time, value, curve)| Breakpoint {
                time: BeatTime((time * TICKS_PER_BEAT as f64) as u64),
                value,
                curve,
            })
            .collect();
        assert_eq!(envelope.points, expected);
    }
    let target = track.devices[0].target().unwrap();
    let fields = &target.parameters[0].fields;
    assert_eq!(fields[0].value_type, FieldType::Int);
    assert_eq!(fields[3].control, Control::Enum);
    assert_eq!(
        fields[3].options[1].value,
        OscArgument::String("Two".into())
    );
    assert_eq!(fields[4].control, Control::Toggle);
}

#[test]
fn frozen_v8_save_round_trip_is_byte_stable() {
    let path =
        std::env::temp_dir().join(format!("qtable-v8-byte-stable-{}.ron", std::process::id()));
    let source = fs::read(fixture()).unwrap();
    assert!(source.starts_with(b"#![enable(implicit_some)]\n(\n    version: 8,\n"));
    let project = file::load(fixture()).unwrap();
    file::save(&path, &project).unwrap();
    let saved = fs::read(&path).unwrap();
    assert!(saved.starts_with(b"#![enable(implicit_some)]\n(\n    version: 12,\n"));
    let reloaded = file::load(&path).unwrap();
    assert_eq!(reloaded, project);
    file::save(&path, &reloaded).unwrap();
    assert_eq!(fs::read(&path).unwrap(), saved);
    // Keep the v8 input unchanged when a future writer migrates it to v9+.
    if file::FORMAT_VERSION == 8 {
        assert_eq!(saved, source);
    }
    fs::remove_file(path).unwrap();
}

#[test]
fn v9_round_trip_keeps_independent_markers_regions_and_outside_content() {
    use qtable_engine::model::region::Region;
    let mut project = file::load(fixture()).unwrap();
    let clip = project.tracks[1].slots[0].clip.as_mut().unwrap();
    clip.start = BeatTime(480);
    clip.end = BeatTime(960);
    clip.loop_start = BeatTime(240);
    clip.loop_end = BeatTime(720);
    assert_eq!(clip.length(), BeatTime(480));
    clip.events[1].position = BeatTime(8 * 960);
    let Lane::Envelope(envelope) = &mut clip.lanes[1] else {
        unreachable!()
    };
    envelope.region = Some(Region {
        start: BeatTime(120),
        end: BeatTime(600),
        loop_start: BeatTime(240),
        loop_end: BeatTime(480),
        looping: false,
    });
    // Existing envelope points beyond both sets of markers are retained too.
    project.validate().unwrap();
    let path = std::env::temp_dir().join(format!("qtable-v9-regions-{}.ron", std::process::id()));
    file::save(&path, &project).unwrap();
    assert_eq!(file::load(&path).unwrap(), project);
    let mut invalid = project.clone();
    let Lane::Envelope(envelope) = &mut invalid.tracks[1].slots[0].clip.as_mut().unwrap().lanes[1]
    else {
        unreachable!()
    };
    envelope.region.as_mut().unwrap().loop_end = BeatTime(240);
    assert!(invalid.validate().is_err());
    assert!(file::save(&path, &invalid).is_err());
    // A rejected save leaves the valid file intact.
    assert_eq!(file::load(&path).unwrap(), project);
    fs::remove_file(path).unwrap();
}

#[test]
fn frozen_v9_upgrades_with_empty_mappings_and_preserves_all_previous_data() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    // This file was emitted by the pre-bump v9 writer, not constructed as v10 RON.
    let path = root.join("v9-markers.ron");
    assert!(fs::read_to_string(&path).unwrap().contains("version: 9"));
    let upgraded = file::load(path).unwrap();
    let mut expected = file::load(root.join("v8-envelopes.ron")).unwrap();
    let clip = expected.tracks[1].slots[0].clip.as_mut().unwrap();
    clip.start = BeatTime(480);
    clip.end = BeatTime(960);
    clip.loop_start = BeatTime(240);
    clip.loop_end = BeatTime(720);
    clip.legato = true;
    clip.events[1].position = BeatTime(8 * 960);
    let Lane::Envelope(envelope) = &mut clip.lanes[1] else {
        unreachable!()
    };
    envelope.region = Some(region::Region {
        start: BeatTime(120),
        end: BeatTime(600),
        loop_start: BeatTime(240),
        loop_end: BeatTime(480),
        looping: false,
    });
    assert_eq!(upgraded, expected);
    assert!(upgraded.mappings.is_empty());
    let path = std::env::temp_dir().join(format!("qtable-upgraded-v9-{}.ron", std::process::id()));
    file::save(&path, &upgraded).unwrap();
    let saved = fs::read_to_string(&path).unwrap();
    assert!(saved.contains("version: 12"));
    assert_eq!(file::load(&path).unwrap(), upgraded);
    fs::remove_file(path).unwrap();
}
