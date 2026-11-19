//! Output stage and Threshold fields are omitted at their defaults in format 8 saves;
//! format 6 sets still load, and Every Frame keeps v3.0's wire name `Stream`.
use qtable_engine::file;
use qtable_engine::model::*;
use std::fs;

fn temp(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("qtable-{name}-{}.ron", std::process::id()))
}

// `v3.0-stream.ron` was saved by the v3.0.0 engine (2026-10-15): a track streaming at 50 Hz
// to Main and an output, and Main streaming at the default rate.
#[test]
fn a_v3_0_stream_set_loads_as_every_frame_and_saves_byte_for_byte() {
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/v3.0-stream.ron"
    );
    let source = fs::read_to_string(fixture).unwrap();
    let project = file::load(fixture).unwrap();
    let kick = &project.tracks[0].mixer_device.volume.output;
    assert_eq!(kick.send_mode, SendMode::EveryFrame);
    assert_eq!(kick.stream_rate, 50.0);
    assert_eq!(kick.threshold, 0.0);
    assert_eq!(
        project.main.mixer_device.volume.output.send_mode,
        SendMode::EveryFrame
    );
    for output in project.destinations() {
        assert_eq!(output.output_stage(), Some(&OutputStage::default()));
    }
    assert_eq!(
        project.every_frame_rate_notice(),
        Some(EVERY_FRAME_RATE_NOTICE)
    );
    let path = temp("v3-0-stream");
    file::save(&path, &project).unwrap();
    // Byte for byte apart from the format version and the redundant header that
    // older builds prepended. Keep the historical fixture unchanged.
    let source = source
        .strip_prefix("#![enable(implicit_some)]\n")
        .unwrap()
        .replace("version: 6", "version: 12");
    assert_eq!(fs::read_to_string(&path).unwrap(), source);
    fs::remove_file(path).unwrap();
}

#[test]
fn default_stage_and_threshold_are_omitted_on_save() {
    let mut project = Project::default();
    project
        .outputs
        .push(Track::new_output(TrackId(201), "A", "127.0.0.1", 9001));
    let path = temp("stage-defaults");
    file::save(&path, &project).unwrap();
    let saved = fs::read_to_string(&path).unwrap();
    for field in [
        "stage",
        "frame_rate",
        "budget",
        "bundle",
        "refresh",
        "_on",
        "threshold",
    ] {
        assert!(!saved.contains(field), "{field} in {saved}");
    }
    fs::remove_file(path).unwrap();
}

#[test]
fn nondefault_stage_fields_and_threshold_write_and_reload() {
    let mut project = Project::default();
    project
        .outputs
        .push(Track::new_output(TrackId(201), "A", "127.0.0.1", 9001));
    project.outputs[0].output.as_mut().unwrap().stage = OutputStage {
        frame_rate: 44.0,
        budget_on: true,
        budget: 600,
        bundle: true,
        refresh_on: true,
        refresh: 0.8,
    };
    // Main switches Bundle on and holds a typed Budget while Off: only those fields are
    // written, and the held value survives the round trip.
    let main_stage = &mut project.main.output.as_mut().unwrap().stage;
    main_stage.bundle = true;
    main_stage.budget = 250;
    let volume = &mut project.main.mixer_device.volume.output;
    volume.send_mode = SendMode::EveryFrame;
    volume.threshold = 0.01;
    let path = temp("stage-nondefault");
    file::save(&path, &project).unwrap();
    let saved = fs::read_to_string(&path).unwrap();
    for field in [
        "frame_rate: 44.0",
        "budget: 600",
        "budget: 250",
        "refresh: 0.8",
        "threshold: 0.01",
        "send_mode: Stream",
    ] {
        assert!(saved.contains(field), "{field} missing from {saved}");
    }
    assert_eq!(saved.matches("bundle: true").count(), 2);
    assert_eq!(saved.matches("frame_rate").count(), 1);
    assert_eq!(saved.matches("budget_on: true").count(), 1);
    assert_eq!(saved.matches("refresh_on: true").count(), 1);
    assert_eq!(saved.matches("refresh").count(), 2);
    let loaded = file::load(&path).unwrap();
    assert_eq!(loaded, project);
    assert_eq!(loaded.main.output_stage().unwrap().budget(), None);
    assert_eq!(loaded.main.output_stage().unwrap().budget, 250);
    file::save(&path, &loaded).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), saved);
    fs::remove_file(path).unwrap();
}

#[test]
fn out_of_range_stage_values_and_unknown_stage_fields_are_rejected_on_load() {
    let path = temp("stage-invalid");
    let write = |stage: &str| {
        fs::write(
            &path,
            format!(
                "(version: 6, project: (main: (id: (100), kind: Main, name: \"Main\", \
                 route: Outputs([]), slots: [], output: Some((destination: \
                 (host: \"127.0.0.1\", port: 9000), stage: {stage})))))"
            ),
        )
        .unwrap();
    };
    write("(frame_rate: 44.0, budget_on: true, budget: 10, refresh: 10.0)");
    let loaded = file::load(&path).unwrap();
    let stage = loaded.main.output_stage().unwrap();
    assert_eq!(stage.frame_rate, 44.0);
    assert_eq!((stage.budget(), stage.refresh()), (Some(10), None));
    for stage in [
        "(frame_rate: 61.0)",
        "(budget_on: true, budget: 5)",
        // A held value must be valid while Off too.
        "(budget: 5)",
        "(refresh: 0.0)",
        "(budget: Some(600))",
        "(frame_rate: 30.0, mtu: 1472)",
    ] {
        write(stage);
        assert!(file::load(&path).is_err(), "{stage}");
    }
    fs::remove_file(path).unwrap();
}
