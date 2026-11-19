use qtable_engine::edit::ProjectEdit;
use qtable_engine::engine::{Command, Engine};
use qtable_engine::model::{Track, TrackId};
use qtable_engine::scheduler::SchedulerEvent;
use qtable_test_support::TestReceiver;
use std::time::{Duration, Instant};

#[path = "support/scheduler_project.rs"]
mod fixture;

fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn destination(host: &str, port: u16) -> Track {
    let mut output = fixture::project(port, &[]).outputs[0].clone();
    if let Some(d) = output.destination_mut() {
        d.host = host.into();
        d.port = port;
    }
    output
}

#[test]
fn unresolvable_host_neither_blocks_nor_rejects_open_or_edits() {
    let receiver = TestReceiver::bind().unwrap();
    let mut project = fixture::project(receiver.addr().port(), &[0.0]);
    project.outputs[0].destination_mut().unwrap().host = "no-such-host.invalid".into();
    let mut engine = Engine::new(project.clone()).unwrap();
    wait_until("unresolved status", || {
        engine.snapshot().unresolved_outputs.len() == 1
    });
    assert_eq!(engine.snapshot().unresolved_outputs[0].id, TrackId(201));
    engine.execute(Command::LoadProject(project)).unwrap();
    engine
        .execute(Command::Edit(ProjectEdit::SetTrackDelay {
            track: TrackId(1),
            delay_ms: 5.0,
        }))
        .unwrap();
    // Pointing the destination at a resolvable host clears the status.
    engine
        .execute(Command::Edit(ProjectEdit::SetDestination {
            output: TrackId(201),
            destination: destination("127.0.0.1", receiver.addr().port()).output,
        }))
        .unwrap();
    wait_until("status cleared", || {
        engine.snapshot().unresolved_outputs.is_empty()
    });
}

#[test]
fn sends_to_unresolved_outputs_are_skipped_with_rate_limited_errors() {
    let receiver = TestReceiver::bind().unwrap();
    let mut project = fixture::project(receiver.addr().port(), &[0.0]);
    project.outputs[0].destination_mut().unwrap().host = "no-such-host.invalid".into();
    let mut engine = Engine::new(project).unwrap();
    engine
        .execute(Command::LaunchClip {
            track: TrackId(1),
            slot: 0,
        })
        .unwrap();
    std::thread::sleep(Duration::from_millis(1500));
    let errors = engine
        .scheduler_events()
        .try_iter()
        .filter(|event| matches!(event, SchedulerEvent::Error(_)))
        .count();
    // The clip sends several times a second at 600 BPM; only one report fits.
    assert_eq!(errors, 1);
}

#[test]
fn hostname_resolves_in_the_background_and_then_sends() {
    let receiver = TestReceiver::bind().unwrap();
    let mut project = fixture::project(receiver.addr().port(), &[0.0]);
    project.outputs[0].destination_mut().unwrap().host = "localhost".into();
    let mut engine = Engine::new(project).unwrap();
    wait_until("localhost resolution", || {
        engine.snapshot().unresolved_outputs.is_empty()
    });
    engine
        .execute(Command::LaunchClip {
            track: TrackId(1),
            slot: 0,
        })
        .unwrap();
    receiver
        .wait_for_messages(1, Duration::from_secs(2))
        .unwrap();
}

#[test]
fn unsetting_main_mid_clip_stops_its_sends_without_errors() {
    let receiver = TestReceiver::bind().unwrap();
    let mut project = fixture::project(receiver.addr().port(), &[0.0]);
    let main = project.main.id;
    project.tracks[0].route = qtable_engine::model::Route::single(main);
    project.main.destination_mut().unwrap().port = receiver.addr().port();
    let mut engine = Engine::new(project).unwrap();
    engine
        .execute(Command::LaunchClip {
            track: TrackId(1),
            slot: 0,
        })
        .unwrap();
    receiver
        .wait_for_messages(2, Duration::from_secs(2))
        .unwrap();
    engine
        .execute(Command::Edit(ProjectEdit::SetDestination {
            output: main,
            destination: None,
        }))
        .unwrap();
    // Sends already in flight may still land; then the stream stops.
    while receiver
        .wait_for_messages(1, Duration::from_millis(400))
        .is_ok()
    {}
    assert!(
        receiver
            .wait_for_messages(1, Duration::from_millis(800))
            .is_err(),
        "Main kept sending after it was unset"
    );
    let errors = engine
        .scheduler_events()
        .try_iter()
        .filter(|event| matches!(event, SchedulerEvent::Error(_)))
        .count();
    assert_eq!(errors, 0);
}
