#[path = "support/scheduler_project.rs"]
mod fixture;
#[path = "support/send_timing.rs"]
mod send_timing;

use qtable_engine::engine::{Command, Engine};
use qtable_engine::model::{
    BeatTime, GlobalQuantisation, LaunchQuantisation, NoteValue, QuantisationValue, Tempo, TrackId,
};
use qtable_engine::scheduler::SchedulerEvent;
use qtable_test_support::TestReceiver;
use std::time::{Duration, Instant};

// Keep timed samples in a separate binary so other scheduler spins cannot compete.
#[test]
fn quarter_and_bar_launches_use_transport_grid_and_publish_triggers() {
    let receiver = TestReceiver::bind().unwrap();
    let mut project = fixture::project(receiver.addr().port(), &[0.0, 0.0]);
    project.tempo = Tempo::new(240.0).unwrap();
    for track in &mut project.tracks {
        let clip = track.slots[0].clip.as_mut().unwrap();
        clip.events.retain(|event| event.position == BeatTime::ZERO);
        clip.looping = false;
    }
    project.tracks[1].slots[0]
        .clip
        .as_mut()
        .unwrap()
        .quantisation = LaunchQuantisation::Value(QuantisationValue::Bars(1));
    let mut engine = Engine::with_diagnostics(project, true).unwrap();
    engine
        .execute(Command::SetGlobalQuantisation(GlobalQuantisation::Value(
            QuantisationValue::Note(NoteValue::Quarter),
        )))
        .unwrap();
    engine.execute(Command::Play).unwrap();
    let anchor = match engine
        .scheduler_events()
        .recv_timeout(Duration::from_secs(1))
        .unwrap()
    {
        SchedulerEvent::Playing(at) => at,
        event => panic!("unexpected {event:?}"),
    };
    for track in [TrackId(1), TrackId(2)] {
        engine
            .execute(Command::LaunchClip { track, slot: 0 })
            .unwrap();
    }
    let timeout = Instant::now() + Duration::from_millis(200);
    loop {
        let snapshot = engine.snapshot();
        if snapshot
            .tracks
            .iter()
            .all(|track| track.triggered.is_some())
        {
            assert_eq!(
                snapshot.global_quantisation,
                GlobalQuantisation::Value(QuantisationValue::Note(NoteValue::Quarter))
            );
            assert_eq!(
                snapshot.tracks[0].triggered.as_ref().unwrap().launch_beat,
                BeatTime(960)
            );
            assert_eq!(
                snapshot.tracks[1].triggered.as_ref().unwrap().launch_beat,
                BeatTime(3840)
            );
            assert!(
                snapshot
                    .tracks
                    .iter()
                    .all(|track| track.playing_slot.is_none())
            );
            break;
        }
        assert!(
            Instant::now() < timeout,
            "both launches should be published before the quarter boundary"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    let messages = receiver
        .wait_for_messages(2, Duration::from_secs(2))
        .unwrap();
    let mut errors = Vec::new();
    for ((arrival, message), (offset, address)) in messages.iter().zip([
        (Duration::from_millis(250), "/t1/s0/e1"),
        (Duration::from_secs(1), "/t2/s0/e1"),
    ]) {
        assert_eq!(message.addr, address);
        let due = anchor + offset;
        let error = if *arrival >= due {
            arrival.duration_since(due)
        } else {
            due.duration_since(*arrival)
        };
        let ms = error.as_secs_f64() * 1000.0;
        errors.push(ms);
    }
    let mean = errors.iter().sum::<f64>() / errors.len() as f64;
    let max = errors.iter().copied().fold(0.0, f64::max);
    eprintln!("launch quantisation arrival jitter: mean_abs={mean:.3} ms max_abs={max:.3} ms");
    let mut deadlines = Vec::new();
    let mut send_errors = Vec::new();
    while deadlines.len() < 2 {
        match engine
            .scheduler_events()
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
        {
            SchedulerEvent::Sent { due, lateness } => {
                deadlines.push(due);
                send_errors.push(lateness.as_secs_f64() * 1000.0);
            }
            SchedulerEvent::Error(error) => panic!("{error}"),
            _ => {}
        }
    }
    assert_eq!(
        deadlines,
        [
            anchor + Duration::from_millis(250),
            anchor + Duration::from_secs(1)
        ]
    );
    let send_mean = send_errors.iter().sum::<f64>() / send_errors.len() as f64;
    let send_max = send_errors.iter().copied().fold(0.0, f64::max);
    eprintln!("launch quantisation send lateness: mean={send_mean:.3} ms max={send_max:.3} ms");
    // Send latency also includes OS preemption; strict timing is a quiet-machine
    // measurement (QTABLE_ASSERT_SEND=1), separate from functional assertions.
    send_timing::assert_send_timing(send_mean, send_max);
    if std::env::var_os("QTABLE_ASSERT_ARRIVAL").is_some() {
        assert!(max <= 2.0, "max absolute arrival error {max:.3} ms");
        assert!(mean < 1.0, "mean absolute arrival error {mean:.3} ms");
    }
    engine.execute(Command::Stop).unwrap();
    engine
        .execute(Command::LaunchClip {
            track: TrackId(1),
            slot: 0,
        })
        .unwrap();
    let restarted = loop {
        match engine
            .scheduler_events()
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
        {
            SchedulerEvent::Playing(at) => break at,
            SchedulerEvent::Error(error) => panic!("{error}"),
            _ => {}
        }
    };
    let immediate = receiver
        .wait_for_messages(1, Duration::from_secs(1))
        .unwrap();
    assert_eq!(immediate[0].1.addr, "/t1/s0/e1");
    let due = loop {
        match engine
            .scheduler_events()
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
        {
            SchedulerEvent::Sent { due, .. } => break due,
            SchedulerEvent::Error(error) => panic!("{error}"),
            _ => {}
        }
    };
    assert_eq!(
        due, restarted,
        "launch while stopped ignores the selected quarter grid"
    );
    let snapshot = engine.snapshot();
    assert!(snapshot.playing);
    assert_eq!(snapshot.tracks[0].playing_slot, Some(0));
    assert!(snapshot.tracks[0].triggered.is_none());
    engine.execute(Command::Stop).unwrap();
}
