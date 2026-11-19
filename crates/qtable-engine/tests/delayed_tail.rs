use qtable_engine::model::*;
use qtable_engine::scheduler::{self, SchedulerCommand, SchedulerEvent};
use qtable_test_support::TestReceiver;
use std::time::Duration;

#[path = "support/scheduler_project.rs"]
mod fixture;

#[test]
fn transport_stop_reports_silence_before_delayed_udp_tail() {
    let receiver = TestReceiver::bind().unwrap();
    let mut project = fixture::project(receiver.addr().port(), &[0.0]);
    project.tempo = Tempo::new(120.0).unwrap();
    project.outputs[0].delay_ms = 600.0;
    let clip = project.tracks[0].slots[0].clip.as_mut().unwrap();
    clip.looping = true;
    clip.end = BeatTime(3840);
    clip.loop_end = BeatTime(3840);
    clip.events.retain(|event| event.position == BeatTime::ZERO);
    let handle = scheduler::spawn(project, Duration::from_millis(12)).unwrap();
    handle
        .send(SchedulerCommand::Launch(TrackId(1), 0))
        .unwrap();
    assert!(matches!(
        handle.events.recv_timeout(Duration::from_secs(1)).unwrap(),
        SchedulerEvent::Playing(_)
    ));
    // Stop after beat zero has played musically, before its delayed packet is due.
    std::thread::sleep(Duration::from_millis(20));
    handle.send(SchedulerCommand::Stop).unwrap();
    assert!(matches!(
        handle
            .events
            .recv_timeout(Duration::from_millis(300))
            .unwrap(),
        SchedulerEvent::TrackSilent(TrackId(1))
    ));
    let messages = receiver
        .wait_for_messages(1, Duration::from_secs(2))
        .unwrap();
    assert_eq!(messages[0].1.addr, "/t1/s0/e1");
    let state = handle.snapshot.lock().unwrap();
    assert!(!state.playing);
    assert!(state.tracks.is_empty());
    drop(state);
    assert!(
        receiver
            .wait_for_messages(1, Duration::from_millis(100))
            .is_err()
    );
    assert!(handle.events.try_recv().is_err());
}
