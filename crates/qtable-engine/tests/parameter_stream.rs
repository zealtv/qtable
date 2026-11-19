use qtable_engine::edit::ProjectEdit;
use qtable_engine::engine::{Command, Engine};
use qtable_engine::model::*;
use qtable_test_support::TestReceiver;
use rosc::OscType;
use std::time::{Duration, Instant};

fn project(receiver: &TestReceiver, mode: SendMode) -> Project {
    let mut track = Track::new(TrackId(1), "Stream", 0);
    track.route = Route::single(TrackId(2));
    track.mixer_device.volume.output.enabled = true;
    track.mixer_device.volume.output.send_mode = mode;
    Project {
        tracks: vec![track],
        outputs: vec![Track::new_output(
            TrackId(2),
            "Out",
            "127.0.0.1",
            receiver.addr().port(),
        )],
        ..Project::default()
    }
}

/// Every message until `quiet` passes without one.
fn drain(receiver: &TestReceiver, quiet: Duration) -> Vec<(Instant, f32)> {
    let mut messages = vec![];
    while let Ok(received) = receiver.wait_for_messages(1, quiet) {
        let (at, message) = &received[0];
        assert_eq!(message.addr, "/volume");
        let [OscType::Float(value)] = message.args.as_slice() else {
            panic!("expected a float");
        };
        messages.push((*at, *value));
    }
    messages
}

#[test]
fn every_frame_follows_the_output_frame_rate_and_stops_without_a_final_send() {
    let receiver = TestReceiver::bind().unwrap();
    let mut p = project(&receiver, SendMode::Continuous);
    // v3.0's own rate is stored but no longer used.
    p.tracks[0].mixer_device.volume.output.stream_rate = 50.0;
    let mut engine = Engine::new(p).unwrap();
    let target = ParameterRef::volume(TrackId(1));
    let mut output = engine
        .project()
        .parameter(&target)
        .unwrap()
        .output
        .clone()
        .into_owned();
    output.send_mode = SendMode::EveryFrame;
    engine
        .execute(Command::Edit(ProjectEdit::SetParameterOutput {
            target,
            output,
        }))
        .unwrap();
    assert!(
        receiver
            .wait_for_messages(1, Duration::from_millis(60))
            .is_err()
    );
    engine.execute(Command::Play).unwrap();
    let first = receiver
        .wait_for_messages(1, Duration::from_secs(1))
        .unwrap()
        .remove(0);
    let started = first.0;
    let mut times = vec![started];
    let end = started + Duration::from_millis(500);
    while Instant::now() < end {
        if let Ok(messages) =
            receiver.wait_for_messages(1, end.saturating_duration_since(Instant::now()))
        {
            assert_eq!(messages[0].1.addr, "/volume");
            times.push(messages[0].0);
        }
    }
    engine.execute(Command::Stop).unwrap();
    // Play sends at once; the frames then follow the 30 Hz grid.
    assert!((13..=17).contains(&times.len()), "{} messages", times.len());
    let grid = &times[1..];
    let mean = grid.last().unwrap().duration_since(grid[0]).as_secs_f64() / (grid.len() - 1) as f64;
    assert!((mean - 1.0 / 30.0).abs() <= 0.002, "mean interval {mean}");
    // Drain a frame racing with the Stop command, then require silence.
    let _ = receiver.wait_for_messages(1, Duration::from_millis(40));
    assert!(
        receiver
            .wait_for_messages(1, Duration::from_millis(80))
            .is_err()
    );
}

#[test]
fn fifty_edits_inside_one_frame_arrive_as_one_message_with_the_last_value() {
    let receiver = TestReceiver::bind().unwrap();
    let mut p = project(&receiver, SendMode::Continuous);
    // One frame a second. Establish a completed frame before the burst: enqueue-time
    // values posted during worker startup can be eligible on its initial grid tick,
    // which the next command must let finish under the dequeue-based frame hold.
    p.outputs[0].output.as_mut().unwrap().stage.frame_rate = 1.0;
    let mut engine = Engine::new(p).unwrap();
    engine
        .execute(Command::Edit(ProjectEdit::set_value(
            ParameterRef::volume(TrackId(1)),
            0.0,
        )))
        .unwrap();
    let initial = receiver
        .wait_for_messages(1, Duration::from_secs(2))
        .unwrap();
    assert_eq!(initial[0].1.args, vec![OscType::Float(0.0)]);
    let edited = Instant::now();
    for step in 1..=50 {
        engine
            .execute(Command::Edit(ProjectEdit::set_value(
                ParameterRef::volume(TrackId(1)),
                f64::from(step) / 100.0,
            )))
            .unwrap();
    }
    assert!(edited.elapsed() < Duration::from_millis(500));
    let messages = drain(&receiver, Duration::from_millis(1500));
    assert_eq!(messages.len(), 1, "{messages:?}");
    assert_eq!(messages[0].1, 0.5);
}

#[test]
fn every_frame_values_before_stop_still_arrive_after_a_200_ms_delay() {
    let receiver = TestReceiver::bind().unwrap();
    let mut p = project(&receiver, SendMode::EveryFrame);
    p.tracks[0].delay_ms = 200.0;
    let mut engine = Engine::new(p).unwrap();
    let played = Instant::now();
    engine.execute(Command::Play).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let stopped = Instant::now();
    engine.execute(Command::Stop).unwrap();
    let messages = drain(&receiver, Duration::from_millis(300));
    let first = messages.first().unwrap().0;
    assert!(first >= played + Duration::from_millis(200), "{messages:?}");
    // The tail: about 200 ms of frames after Stop, the last within a frame of Stop + 200 ms.
    let tail = messages.iter().filter(|(at, _)| *at > stopped).count();
    assert!((5..=8).contains(&tail), "{tail} after Stop: {messages:?}");
    let last = messages.last().unwrap().0;
    let shifted_stop = stopped + Duration::from_millis(200);
    assert!(
        last <= shifted_stop + Duration::from_millis(10),
        "{messages:?}"
    );
    assert!(
        last + Duration::from_millis(45) >= shifted_stop,
        "{messages:?}"
    );
}

#[test]
fn stream_rate_is_validated_and_format_six_round_trips() {
    let mut p = Project::default();
    for hz in [0.0, 201.0, f64::NAN, f64::INFINITY] {
        p.main.mixer_device.volume.output.send_mode = SendMode::EveryFrame;
        p.main.mixer_device.volume.output.stream_rate = hz;
        assert!(p.validate().is_err());
    }
    p.main.mixer_device.volume.output.send_mode = SendMode::EveryFrame;
    p.main.mixer_device.volume.output.stream_rate = 42.5;
    let text = ron::to_string(&p).unwrap();
    let loaded: Project = ron::from_str(&text).unwrap();
    assert_eq!(loaded.main.mixer_device, p.main.mixer_device);
    assert_eq!(STREAM_RATE.unit.suffix(), " Hz");
    assert_eq!(STREAM_RATE.default, 30.0);
}
