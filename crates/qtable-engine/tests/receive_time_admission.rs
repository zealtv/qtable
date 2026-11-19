//! Actual worker/engine receipt path over UDP. These are functional checks, not latency measurements.
#[path = "support/scheduler_project.rs"]
mod fixture;
use qtable_engine::{
    clock::{InternalClock, TempoClock},
    engine::{Command, Engine, Receipt},
    model::*,
    scheduler::{AdmissionOutcome, SchedulerEvent},
};
use qtable_test_support::TestReceiver;
use std::time::{Duration, Instant};

fn next(engine: &Engine, accept: impl Fn(&SchedulerEvent) -> bool) -> SchedulerEvent {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let event = engine
            .scheduler_events()
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap();
        assert!(!matches!(event, SchedulerEvent::Error(_)), "{event:?}");
        if accept(&event) {
            return event;
        }
    }
}

fn play(engine: &mut Engine) -> Instant {
    engine.execute(Command::Play).unwrap();
    let SchedulerEvent::Playing(anchor) = next(engine, |e| matches!(e, SchedulerEvent::Playing(_)))
    else {
        unreachable!()
    };
    anchor
}

#[test]
fn worker_preserves_receipts_and_rejects_expired_and_previous_generation_context() {
    let receiver = TestReceiver::bind().unwrap();
    let mut project = fixture::project(receiver.addr().port(), &[0.0]);
    project.global_quantisation =
        GlobalQuantisation::Value(QuantisationValue::Note(NoteValue::Quarter));
    for slot in &mut project.tracks[0].slots {
        let clip = slot.clip.as_mut().unwrap();
        clip.events.retain(|event| event.position == BeatTime::ZERO);
        clip.looping = false;
    }
    let mut engine = Engine::with_grid_lead(project.clone(), Duration::ZERO).unwrap();
    let anchor = play(&mut engine);
    let r = Instant::now();
    let context = engine.command_context(Receipt {
        received_at: r,
        sequence: Some(42),
    });
    engine
        .execute_with_context(
            Command::LaunchClip {
                track: TrackId(1),
                slot: 0,
            },
            context,
        )
        .unwrap();
    let event = next(&engine, |e| {
        matches!(e, SchedulerEvent::LaunchAdmission { .. })
    });
    let SchedulerEvent::LaunchAdmission {
        received,
        enqueued,
        dequeued,
        effective,
        submission,
        sequence,
        outcome,
    } = event
    else {
        unreachable!()
    };
    assert_eq!(received, r);
    assert!(received <= enqueued && enqueued <= dequeued);
    assert_eq!(outcome, AdmissionOutcome::Admitted);
    assert_eq!(effective, r);
    assert_eq!(sequence, Some(42));
    assert_eq!(submission, Some(2));
    let first = receiver
        .wait_for_messages(1, Duration::from_secs(2))
        .unwrap();
    assert_eq!(first[0].1.addr, "/t1/s0/e1");
    let mut clock = InternalClock::new(project.tempo, project.time_signature);
    clock.play(anchor);
    let beat = project
        .global_quantisation
        .next_boundary(clock.beat_at(r).unwrap(), project.time_signature)
        .unwrap();
    let due = clock.instant_at(beat).unwrap();
    let SchedulerEvent::Sent { due: sent_due, .. } =
        next(&engine, |e| matches!(e, SchedulerEvent::Sent { .. }))
    else {
        unreachable!()
    };
    assert_eq!(sent_due, due);

    // Intentionally stale receipt: no near-boundary OS scheduling assumption.
    std::thread::sleep(Duration::from_millis(20));
    engine
        .execute_with_context(
            Command::LaunchClip {
                track: TrackId(1),
                slot: 1,
            },
            context,
        )
        .unwrap();
    let SchedulerEvent::LaunchAdmission {
        outcome,
        effective,
        dequeued,
        ..
    } = next(&engine, |e| {
        matches!(e, SchedulerEvent::LaunchAdmission { .. })
    })
    else {
        unreachable!()
    };
    assert_eq!(outcome, AdmissionOutcome::Expired);
    assert_eq!(effective, dequeued);
    let messages = receiver
        .wait_for_messages(1, Duration::from_secs(2))
        .unwrap();
    assert_eq!(messages[0].1.addr, "/t1/s1/e1");

    engine.execute(Command::LoadProject(project)).unwrap();
    play(&mut engine);
    engine
        .execute_with_context(
            Command::LaunchClip {
                track: TrackId(1),
                slot: 0,
            },
            context,
        )
        .unwrap();
    let SchedulerEvent::LaunchAdmission {
        outcome,
        effective,
        dequeued,
        ..
    } = next(&engine, |e| {
        matches!(e, SchedulerEvent::LaunchAdmission { .. })
    })
    else {
        unreachable!()
    };
    assert_eq!(outcome, AdmissionOutcome::InvalidContext);
    assert_eq!(effective, dequeued);
    let messages = receiver
        .wait_for_messages(1, Duration::from_secs(2))
        .unwrap();
    assert_eq!(messages[0].1.addr, "/t1/s0/e1");
}
