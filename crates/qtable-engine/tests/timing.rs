#[path = "support/scheduler_project.rs"]
mod fixture;
#[path = "support/send_timing.rs"]
mod send_timing;
use qtable_engine::model::{GlobalQuantisation, TrackId};
use qtable_engine::scheduler::{self, SchedulerCommand, SchedulerEvent};
use qtable_test_support::TestReceiver;
use std::time::Duration;

// Keep the spin-wait measurement in its own integration-test binary so another
// scheduler test's busy loop cannot compete for CPU during the jitter sample.
#[test]
fn real_thread_order_wrap_signed_delays_and_jitter() {
    let receiver = TestReceiver::bind().unwrap();
    let handle = scheduler::spawn_with_diagnostics(
        fixture::project(receiver.addr().port(), &[-15.0, 15.0]),
        Duration::from_millis(5),
        true,
    )
    .unwrap();
    handle.send(SchedulerCommand::Play).unwrap();
    let anchor = match handle.events.recv_timeout(Duration::from_secs(1)).unwrap() {
        SchedulerEvent::Playing(at) => at,
        e => panic!("{e:?}"),
    };
    handle
        .send(SchedulerCommand::Launch(TrackId(1), 0))
        .unwrap();
    handle
        .send(SchedulerCommand::Launch(TrackId(2), 0))
        .unwrap();
    let received = receiver
        .wait_for_messages(12, Duration::from_secs(2))
        .unwrap();
    let mut expected = Vec::new();
    for cycle in 0..2 {
        for (track, delay) in [(1, -15_i64), (2, 15)] {
            for (offset, event) in [(0, 1), (50, 0), (50, 2)] {
                let ms = 400 + cycle * 100 + offset + delay;
                expected.push((
                    anchor + Duration::from_millis(ms as u64),
                    format!("/t{track}/s0/e{event}"),
                ));
            }
        }
    }
    expected.sort_by_key(|e| e.0);
    let errors: Vec<_> = received
        .iter()
        .zip(&expected)
        .map(|((arrival, message), (due, address))| {
            assert_eq!(&message.addr, address);
            arrival
                .checked_duration_since(*due)
                .unwrap_or_else(|| due.duration_since(*arrival))
                .as_secs_f64()
                * 1000.0
        })
        .collect();
    let mean = errors.iter().sum::<f64>() / errors.len() as f64;
    let max = errors.iter().copied().fold(0.0, f64::max);
    eprintln!(
        "scheduler jitter: n={}, mean_abs={mean:.3} ms max_abs={max:.3} ms",
        errors.len()
    );
    let mut send_errors = Vec::new();
    for (due, _) in &expected {
        match handle.events.recv_timeout(Duration::from_secs(1)).unwrap() {
            SchedulerEvent::Sent {
                due: actual_due,
                lateness,
            } => {
                assert_eq!(*due, actual_due);
                send_errors.push(lateness.as_secs_f64() * 1000.0);
            }
            e => panic!("{e:?}"),
        }
    }
    let send_mean = send_errors.iter().sum::<f64>() / send_errors.len() as f64;
    let send_max = send_errors.iter().copied().fold(0.0, f64::max);
    eprintln!("send lateness: mean={send_mean:.3} ms max={send_max:.3} ms");
    // Send latency also includes OS preemption; strict timing is a quiet-machine
    // measurement (QTABLE_ASSERT_SEND=1), separate from functional assertions.
    send_timing::assert_send_timing(send_mean, send_max);
    // Arrival includes the OS waking the test receiver, which on a busy machine
    // (e.g. a concurrent build) reached 21 ms while sends stayed < 0.19 ms
    // (scheduler-realtime notes, 2026-10-10). So arrival is always reported but
    // only asserted when QTABLE_ASSERT_ARRIVAL=1, for measuring on a quiet machine.
    if std::env::var_os("QTABLE_ASSERT_ARRIVAL").is_some() {
        assert!(
            max < 10.0,
            "max absolute loopback arrival error {max:.3} ms"
        );
        assert!(mean < 1.0, "mean absolute arrival error {mean:.3} ms");
    }
    // This test measures jitter, then disarms immediately; quantised stops are
    // covered by the dedicated launch/stop behaviour tests.
    handle
        .send(SchedulerCommand::SetGlobalQuantisation(
            GlobalQuantisation::None,
        ))
        .unwrap();
    handle.send(SchedulerCommand::StopAll).unwrap();
    for _ in 0..2 {
        assert!(matches!(
            handle.events.recv_timeout(Duration::from_secs(1)).unwrap(),
            SchedulerEvent::TrackSilent(_)
        ));
    }
    assert!(
        receiver
            .wait_for_messages(1, Duration::from_millis(150))
            .is_err()
    );
}
