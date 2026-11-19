use qtable_engine::engine::{Command, Engine};
use qtable_engine::model::{Route, TrackId};
use std::time::{Duration, Instant};

#[path = "support/scheduler_project.rs"]
mod fixture;

#[test]
fn scheduler_errors_reach_the_snapshot_status() {
    let mut project = fixture::project(9, &[0.0]);
    project.outputs[0].destination_mut().unwrap().host = "no-such-host.invalid".into();
    let mut engine = Engine::new(project).unwrap();
    engine
        .execute(Command::LaunchClip {
            track: TrackId(1),
            slot: 0,
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        engine.poll_events();
        let status = engine.snapshot().status;
        if status.error_count > 0 {
            assert!(status.last_error.is_some());
            assert!(!status.recent_errors.is_empty());
            assert!(status.scheduler_failure.is_none());
            break;
        }
        assert!(Instant::now() < deadline, "no scheduler error surfaced");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn launching_a_clip_with_an_empty_route_is_silent_and_reported() {
    let mut project = fixture::project(9, &[0.0]);
    project.tracks[0].route = Route::empty();
    let mut engine = Engine::new(project).unwrap();
    engine
        .execute(Command::LaunchClip {
            track: TrackId(1),
            slot: 0,
        })
        .unwrap();
    assert!(engine.snapshot().tracks[0].empty_route);
    std::thread::sleep(Duration::from_millis(30));
    engine.poll_events();
    assert_eq!(engine.snapshot().status.error_count, 0);
}
