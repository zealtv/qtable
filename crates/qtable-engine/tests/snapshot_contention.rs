//! Hold the UI snapshot lock across both commands and timed follow/send events.
#[path = "support/send_timing.rs"]
mod send_timing;
use qtable_engine::model::*;
use qtable_engine::scheduler::{self, SchedulerCommand, SchedulerEvent};
use qtable_test_support::TestReceiver;
use std::time::{Duration, Instant};

#[path = "support/scheduler_project.rs"]
mod fixture;

#[test]
fn locked_snapshot_does_not_block_commands_follow_or_sends_and_eventually_catches_up() {
    let receiver = TestReceiver::bind().unwrap();
    let mut project = fixture::project(receiver.addr().port(), &[0.0, 0.0]);
    project.global_quantisation = GlobalQuantisation::None;
    let clip = project.tracks[0].slots[0].clip.as_mut().unwrap();
    clip.follow_actions.actions.enabled = true;
    clip.follow_actions.actions.action_a = FollowAction::Stop;
    clip.follow_actions.linked = false;
    clip.follow_actions.time = BeatTime(960);
    let handle =
        scheduler::spawn_with_diagnostics(project, Duration::from_millis(12), true).unwrap();
    let read = handle.snapshot.lock().unwrap();
    handle
        .send(SchedulerCommand::LaunchScene(SceneId(1)))
        .unwrap();
    assert!(matches!(
        handle.events.recv_timeout(Duration::from_secs(1)).unwrap(),
        SchedulerEvent::Playing(_)
    ));
    let packets = receiver
        .wait_for_messages(9, Duration::from_secs(1))
        .unwrap();
    assert_eq!(
        packets
            .iter()
            .filter(|(_, message)| message.addr.starts_with("/t1/"))
            .count(),
        3
    );
    assert_eq!(
        packets
            .iter()
            .filter(|(_, message)| message.addr.starts_with("/t2/"))
            .count(),
        6
    );
    assert!(!read.playing, "the reader still holds the initial snapshot");

    let mut lateness = Vec::new();
    let mut silenced = Vec::new();
    while lateness.len() < 9 || silenced.is_empty() {
        match handle.events.recv_timeout(Duration::from_secs(1)).unwrap() {
            SchedulerEvent::Sent { lateness: late, .. } => {
                lateness.push(late.as_secs_f64() * 1000.0)
            }
            SchedulerEvent::TrackSilent(track) => silenced.push(track),
            other => panic!("unexpected scheduler event: {other:?}"),
        }
    }
    assert_eq!(silenced, [TrackId(1)]);
    let max = lateness.iter().copied().fold(0.0, f64::max);
    let mean = lateness.iter().sum::<f64>() / lateness.len() as f64;
    eprintln!("snapshot contention send lateness: mean={mean:.3} ms max={max:.3} ms");
    send_timing::assert_send_timing(mean, max);
    drop(read);

    let limit = Instant::now() + Duration::from_secs(1);
    loop {
        let state = handle.snapshot.lock().unwrap();
        if state.last_triggered_scene == Some(SceneId(1)) && state.tracks.len() == 1 {
            assert!(state.playing);
            assert_eq!(state.tracks[0].track, TrackId(2));
            break;
        }
        drop(state);
        assert!(
            Instant::now() < limit,
            "snapshot did not catch up after unlock"
        );
        std::thread::sleep(Duration::from_millis(1));
    }

    // A stopped/idle scheduler must also retry without another command or event.
    let read = handle.snapshot.lock().unwrap();
    handle.send(SchedulerCommand::Stop).unwrap();
    loop {
        match handle.events.recv_timeout(Duration::from_secs(1)).unwrap() {
            SchedulerEvent::TrackSilent(TrackId(2)) => break,
            SchedulerEvent::Sent { .. } => {}
            other => panic!("unexpected scheduler event: {other:?}"),
        }
    }
    assert!(read.playing);
    drop(read);
    let limit = Instant::now() + Duration::from_secs(1);
    loop {
        let state = handle.snapshot.lock().unwrap();
        if !state.playing {
            assert!(state.tracks.is_empty());
            break;
        }
        drop(state);
        assert!(
            Instant::now() < limit,
            "idle snapshot did not catch up after unlock"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}
