//! Format 12: the byte-level golden plan for the mapping additions (Gate, Value Scaling,
//! sensitivity and event arguments).
//!
//! `v11-mappings.ron` was emitted by the unmodified format-11 writer (v3.9) before the version
//! bump and stays frozen: it is the evidence that every format-11 set loads unchanged and saves
//! the bytes it saved before, but for the version line. `v12-mappings.ron` was written by the
//! format-12 writer and holds each addition the format makes.
use qtable_engine::{file, model::*};
use std::path::PathBuf;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn read(name: &str) -> String {
    std::fs::read_to_string(fixtures().join(name)).unwrap()
}

fn golden(name: &str) -> String {
    read(&format!("saved-bytes/{name}"))
}

fn temp(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("qtable-v12-{name}-{}.ron", std::process::id()))
}

/// What the current writer saves for a fixture, and that saving that again repeats it.
fn saved(name: &str) -> String {
    let project = file::load(fixtures().join(name)).unwrap();
    let path = temp(name);
    file::save(&path, &project).unwrap();
    let first = std::fs::read_to_string(&path).unwrap();
    let reloaded = file::load(&path).unwrap();
    assert_eq!(reloaded, project, "{name}: a reload changed the model");
    file::save(&path, &reloaded).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), first, "{name}");
    std::fs::remove_file(path).unwrap();
    first
}

const HEADER_11: &str = "#![enable(implicit_some)]\n(\n    version: 11,\n";
const HEADER_12: &str = "#![enable(implicit_some)]\n(\n    version: 12,\n";

#[test]
fn the_v11_fixture_was_written_by_the_format_11_writer_and_holds_no_format_12_spelling() {
    let text = read("v11-mappings.ron");
    assert!(text.starts_with(HEADER_11));
    for spelling in ["Gate", "ValueScaling", "sensitivity", "EventArgument"] {
        assert!(!text.contains(spelling), "{spelling}");
    }
}

#[test]
fn v11_mappings_load_as_before_with_standard_sensitivity_and_no_gate_or_value_scaling() {
    let project = file::load(fixtures().join("v11-mappings.ron")).unwrap();
    project.validate().unwrap();
    assert_eq!(project.mappings.len(), 18);
    for row in &project.mappings {
        assert_eq!(row.sensitivity, 1.0, "{row:?}");
        assert_ne!(row.mode, SourceMode::Gate, "{row:?}");
        assert_ne!(row.takeover, Takeover::ValueScaling, "{row:?}");
        assert!(
            !matches!(row.control, ControlRef::EventArgument { .. }),
            "{row:?}"
        );
    }
    // The shapes the file holds still read as they were saved.
    let count =
        |keep: &dyn Fn(&RemoteMapping) -> bool| project.mappings.iter().filter(|r| keep(r)).count();
    assert_eq!(count(&|r| r.takeover == Takeover::PickUp), 3);
    assert_eq!(count(&|r| matches!(r.mode, SourceMode::Relative(_))), 3);
    assert_eq!(count(&|r| r.min > r.max), 1);
    assert_eq!(count(&|r| r.min == r.max), 1);
    assert_eq!(
        count(&|r| matches!(r.control, ControlRef::TrackActivator(_))),
        1
    );
    assert_eq!(count(&|r| matches!(r.source, Source::Key(_))), 3);
    let nested: Vec<_> = project.tracks.iter().filter_map(|t| t.group).collect();
    assert_eq!(nested, [TrackId(20), TrackId(21)], "nested groups load");
}

#[test]
fn a_v11_set_saved_by_the_current_writer_differs_from_its_v11_bytes_by_the_version_line_alone() {
    let fixture = read("v11-mappings.ron");
    assert_eq!(
        saved("v11-mappings.ron"),
        fixture.replacen("version: 11", "version: 12", 1)
    );
    // Its golden is the same bytes: the format-11 writer's, with the new version line.
    assert_eq!(
        golden("engine-v11-mappings.ron"),
        fixture.replacen("version: 11", "version: 12", 1)
    );
}

#[test]
fn the_v12_fixture_holds_each_spelling_the_format_adds() {
    let text = read("v12-mappings.ron");
    assert!(text.starts_with(HEADER_12));
    for spelling in [
        "mode: Gate,",
        "takeover: ValueScaling,",
        "sensitivity: 2.5,",
        "control: EventArgument(\n                    track: (1),\n                    event: (\n                        device: (1),\n                        key: (\"e_float_slider\"),\n                    ),\n                    field: (\"value\"),\n                ),",
    ] {
        assert!(text.contains(spelling), "{spelling}");
    }
    // The format-11 rows come first and are the v11 fixture's, byte for byte.
    let v11 = read("v11-mappings.ron");
    let body = |text: &str| text.split_once("mappings: [").unwrap().1.to_owned();
    assert!(
        body(&text).starts_with(
            body(&v11)
                .trim_end_matches(|c| c != '}')
                .trim_end_matches('}')
        )
    );
}

#[test]
fn the_v12_fixture_loads_and_its_rows_read_as_written() {
    let project = file::load(fixtures().join("v12-mappings.ron")).unwrap();
    project.validate().unwrap();
    let rows = &project.mappings[18..];
    assert_eq!(rows.len(), 11);
    let gates: Vec<_> = rows.iter().filter(|r| r.mode == SourceMode::Gate).collect();
    assert_eq!(gates.len(), 3);
    assert!(
        gates
            .iter()
            .all(|r| r.source.supports_mode(SourceMode::Gate))
    );
    // Invert is Min above Max: a Gate on a switch carries it, and so does a reversed Number.
    assert!(gates.iter().any(|r| r.min > r.max));
    assert!(
        rows.iter()
            .any(|r| r.min > r.max && r.mode == SourceMode::Absolute)
    );
    let scaling: Vec<_> = rows
        .iter()
        .filter(|r| r.takeover == Takeover::ValueScaling)
        .collect();
    assert_eq!(scaling.len(), 2);
    assert!(scaling.iter().any(|r| matches!(
        r.source,
        Source::Midi(MidiSource {
            message: MidiMessage::PitchBend,
            ..
        })
    )));
    let sensitivities: Vec<_> = rows
        .iter()
        .map(|r| r.sensitivity)
        .filter(|s| *s != 1.0)
        .collect();
    assert_eq!(sensitivities, [2.5, 0.25]);
    let arguments: Vec<_> = rows
        .iter()
        .filter(|r| matches!(r.control, ControlRef::EventArgument { .. }))
        .collect();
    assert_eq!(arguments.len(), 6);
    for row in arguments {
        assert!(
            describe_control(&project, &row.control).is_some(),
            "{row:?}"
        );
        let address = control_address(&row.control);
        assert_eq!(
            parse_control_address(&address),
            Some(row.control.clone()),
            "{address}"
        );
    }
}

#[test]
fn the_v12_fixture_saves_its_own_bytes_and_a_second_save_repeats_them() {
    let fixture = read("v12-mappings.ron");
    assert_eq!(saved("v12-mappings.ron"), fixture);
    assert_eq!(golden("engine-v12-mappings.ron"), fixture);
}

#[test]
fn a_row_at_sensitivity_one_writes_no_sensitivity_line() {
    let text = read("v12-mappings.ron");
    let project = file::load(fixtures().join("v12-mappings.ron")).unwrap();
    let off_standard = project
        .mappings
        .iter()
        .filter(|r| r.sensitivity != 1.0)
        .count();
    let lines = text
        .lines()
        .filter(|l| l.trim_start().starts_with("sensitivity:"))
        .count();
    assert_eq!((off_standard, lines), (2, 2));
    assert!(project.mappings.len() > lines + 10, "most rows sit at 1.0");
}

#[test]
fn a_format_11_reader_refuses_a_format_12_file_by_version() {
    let path = temp("old-reader");
    std::fs::write(&path, read("v12-mappings.ron")).unwrap();
    // Optional real pre-bump reader: a v3.9.0 `qtable play` binary, whose `file.rs` is the
    // format-11 reader (for example dist/v3.9.0/qtable in a checkout). It must
    // refuse by version, not by judging the project. Skipped when the variable is unset.
    if let Some(reader) = std::env::var_os("QTABLE_FORMAT11_READER") {
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
                .contains("unsupported project format version 12; this build supports version 11"),
            "{stderr}"
        );
        println!("pre-format-12 reader refused version 12");
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn this_build_refuses_a_version_it_does_not_know_with_its_own_number() {
    let path = temp("future");
    std::fs::write(
        &path,
        read("v12-mappings.ron").replacen("version: 12", "version: 13", 1),
    )
    .unwrap();
    let message = file::load(&path).unwrap_err().to_string();
    assert!(
        message.contains("unsupported project format version 13; this build supports version 12"),
        "{message}"
    );
    std::fs::remove_file(path).unwrap();
}
