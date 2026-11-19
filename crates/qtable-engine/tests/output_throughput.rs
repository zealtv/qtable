//! The engine snapshot exposes the same counts for Main and ordinary destinations.
use qtable_engine::engine::{Command, Engine};
use qtable_engine::model::*;
use qtable_test_support::TestReceiver;
use std::time::{Duration, Instant};

#[test]
fn bundled_start_sends_reach_snapshots_and_expire_while_stopped() {
    let receiver = TestReceiver::bind().unwrap();
    let mut output = Track::new_output(TrackId(3), "Out", "127.0.0.1", receiver.addr().port());
    output.output.as_mut().unwrap().stage.bundle = true;
    let mut p = Project {
        outputs: vec![output],
        ..Project::default()
    };
    p.main.output = p.outputs[0].output.clone();
    for id in 1..=2 {
        let mut track = Track::new(TrackId(id), "Track", 0);
        track.route = Route::Outputs(vec![p.main.id, TrackId(3)]);
        track.mixer_device.volume.output.enabled = true;
        p.tracks.push(track);
    }
    let mut engine = Engine::with_diagnostics(p, true).unwrap();
    engine.execute(Command::Play).unwrap();
    receiver
        .wait_for_messages(4, Duration::from_secs(1))
        .unwrap();
    engine.execute(Command::Stop).unwrap();
    let deadline = Instant::now() + Duration::from_secs(1);
    let snapshot = loop {
        let snapshot = engine.snapshot();
        if snapshot.output_throughput.len() == 2
            && snapshot
                .output_throughput
                .iter()
                .all(|(_, value)| value.messages == 2)
        {
            break snapshot;
        }
        assert!(Instant::now() < deadline, "{snapshot:?}");
        std::thread::sleep(Duration::from_millis(1));
    };
    for (_, value) in snapshot.output_throughput {
        assert_eq!(value.packets, 1);
        assert_eq!(value.bytes, 56); // 16-byte bundle + two (4-byte size + 16-byte message).
        assert!(!value.deferring);
    }
    let events: Vec<_> = engine.scheduler_events().try_iter().collect();
    let frames: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            qtable_engine::scheduler::SchedulerEvent::FrameSent {
                output, messages, ..
            } => Some((*output, *messages)),
            _ => None,
        })
        .collect();
    assert_eq!(frames, vec![(TrackId(0), 2), (TrackId(3), 2)]);
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, qtable_engine::scheduler::SchedulerEvent::Sent { .. }))
    );
    std::thread::sleep(Duration::from_millis(1050));
    for (_, value) in engine.snapshot().output_throughput {
        assert_eq!(value, qtable_engine::scheduler::Throughput::default());
    }
}
