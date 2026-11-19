#[path = "support/scheduler_project.rs"]
mod fixture;
use qtable_engine::model::TrackId;
use qtable_engine::scheduler::{self, SchedulerCommand, SchedulerEvent};
use qtable_test_support::TestReceiver;
use std::time::Duration;

#[test]
fn replacement_stop_transport_and_tempo_commands() {
    let receiver = TestReceiver::bind().unwrap();
    let mut project = fixture::project(receiver.addr().port(), &[0.0]);
    for slot in &mut project.tracks[0].slots {
        slot.clip.as_mut().unwrap().looping = false;
    }
    let handle = scheduler::spawn(project, Duration::from_millis(5)).unwrap();
    handle.send(SchedulerCommand::Play).unwrap();
    assert!(matches!(
        handle.events.recv_timeout(Duration::from_secs(1)).unwrap(),
        SchedulerEvent::Playing(_)
    ));
    handle
        .send(SchedulerCommand::Launch(TrackId(1), 0))
        .unwrap();
    handle
        .send(SchedulerCommand::Launch(TrackId(1), 1))
        .unwrap();
    handle
        .send(SchedulerCommand::SetTempo(
            qtable_engine::model::Tempo::new(1200.0).unwrap(),
        ))
        .unwrap();
    let messages = receiver
        .wait_for_messages(3, Duration::from_secs(1))
        .unwrap();
    assert!(messages.iter().all(|(_, m)| m.addr.contains("/s1/")));
    assert!(matches!(
        handle.events.recv_timeout(Duration::from_secs(1)).unwrap(),
        SchedulerEvent::TrackSilent(TrackId(1))
    ));
    assert!(
        receiver
            .wait_for_messages(1, Duration::from_millis(80))
            .is_err()
    );
    handle.send(SchedulerCommand::Stop).unwrap();
    handle.send(SchedulerCommand::Play).unwrap();
    assert!(matches!(
        handle.events.recv_timeout(Duration::from_secs(1)).unwrap(),
        SchedulerEvent::Playing(_)
    ));
    handle
        .send(SchedulerCommand::Launch(TrackId(1), 0))
        .unwrap();
    handle
        .send(SchedulerCommand::StopTrack(TrackId(1)))
        .unwrap();
    assert!(matches!(
        handle.events.recv_timeout(Duration::from_secs(1)).unwrap(),
        SchedulerEvent::TrackSilent(TrackId(1))
    ));
    assert!(
        receiver
            .wait_for_messages(1, Duration::from_millis(300))
            .is_err()
    );
}
