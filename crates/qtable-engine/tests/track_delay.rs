use qtable_engine::edit::ProjectEdit;
use qtable_engine::engine::{Command, Engine};
use qtable_engine::model::{MAX_TRACK_DELAY_MS, TrackId};

#[path = "support/scheduler_project.rs"]
mod fixture;

#[test]
fn delay_validation_accepts_inclusive_limits_and_rejects_nonfinite_or_excessive_values() {
    let mut project = fixture::project(9999, &[0.0]);
    for value in [
        -MAX_TRACK_DELAY_MS,
        -12.5,
        -0.0,
        0.0,
        12.5,
        MAX_TRACK_DELAY_MS,
    ] {
        project.tracks[0].delay_ms = value;
        assert!(project.validate().is_ok(), "rejected {value} ms");
    }
    for value in [
        -1000.000001,
        1000.000001,
        -10000.0,
        10000.0,
        f64::MIN,
        f64::MAX,
        f64::NAN,
        f64::NEG_INFINITY,
        f64::INFINITY,
    ] {
        project.tracks[0].delay_ms = value;
        assert!(project.validate().is_err(), "accepted {value} ms");
    }
}

#[test]
fn engine_rejects_excessive_delay_edits_synchronously_without_changing_project() {
    let mut engine = Engine::new(fixture::project(9999, &[0.0])).unwrap();
    for value in [-MAX_TRACK_DELAY_MS, MAX_TRACK_DELAY_MS] {
        engine
            .execute(Command::Edit(ProjectEdit::SetTrackDelay {
                track: TrackId(1),
                delay_ms: value,
            }))
            .unwrap();
        for invalid in [-10000.0, 1000.000001, f64::INFINITY, f64::NAN] {
            assert!(
                engine
                    .execute(Command::Edit(ProjectEdit::SetTrackDelay {
                        track: TrackId(1),
                        delay_ms: invalid
                    }))
                    .is_err()
            );
            assert_eq!(engine.project().tracks[0].delay_ms, value);
        }
    }
}
