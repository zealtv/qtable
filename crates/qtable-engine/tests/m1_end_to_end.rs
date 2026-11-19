use qtable_engine::model::Route;
#[path = "support/send_timing.rs"]
mod send_timing;
use std::time::{Duration, Instant};

use qtable_engine::engine::{Command, Engine};
use qtable_engine::model::{
    BeatTime, Clip, ClipId, DeviceId, OscAddress, OscArgument, Project, Scene, SceneFollowActions,
    SceneId, Slot, Tempo, Track, TrackId,
};
use qtable_test_support::{ReceivedMessage, TestReceiver};
use rosc::OscType;

const BPM: f64 = 240.0;
const BEAT_MS: f64 = 60_000.0 / BPM;
const CYCLES: usize = 3;
const ARRIVAL_SANITY_MS: f64 = 10.0;

fn project(port_a: u16, port_b: u16) -> Project {
    let mut project = Project {
        tempo: Tempo::new(BPM).unwrap(),

        outputs: vec![
            Track::new_output(TrackId(201), "alpha", "127.0.0.1", port_a).with_path("/alpha"),
            Track::new_output(TrackId(202), "beta", "127.0.0.1", port_b).with_path("/beta"),
        ],
        scenes: vec![Scene {
            id: SceneId(1),
            name: "Only scene".into(),
            follow_actions: SceneFollowActions::default(),
        }],
        ..Project::default()
    };
    project.tracks = vec![
        track(1, "zero delay", TrackId(201), 0.0, "a"),
        track(2, "positive delay", TrackId(202), 20.0, "b"),
        track(3, "negative delay", TrackId(201), -20.0, "c"),
    ];
    project
}

fn track(id: u64, name: &str, target: TrackId, delay_ms: f64, stem: &str) -> Track {
    let mut track = Track::new(TrackId(id), name, 1);
    track.route = Route::single(target);
    track.delay_ms = delay_ms;
    let mut clip = Clip::new(
        ClipId(id),
        format!("{name} clip"),
        BeatTime(4 * 960),
        qtable_engine::model::LIVE_CLIP_PALETTE[0],
    )
    .unwrap();
    clip.events = vec![
        track.hit(
            BeatTime::ZERO,
            OscAddress::Relative(format!("/{stem}/pulse")),
            vec![OscArgument::Int(10), OscArgument::Bool(true)],
            || DeviceId(id),
        ),
        track.hit(
            BeatTime(1_440),
            OscAddress::Relative(format!("/{stem}/level")),
            vec![OscArgument::Float(0.5), OscArgument::String("warm".into())],
            || DeviceId(id),
        ),
        track.hit(
            BeatTime(2_880),
            OscAddress::Absolute(format!("/absolute/{stem}")),
            vec![
                OscArgument::Int(-2),
                OscArgument::Float(1.25),
                OscArgument::String("done".into()),
                OscArgument::Bool(false),
            ],
            || DeviceId(id),
        ),
    ];
    track.slots[0] = Slot {
        clip: Some(clip),
        has_stop_button: true,
    };
    track
}

#[derive(Clone)]
struct Expected {
    offset_ms: f64,
    address: String,
    args: Vec<OscType>,
}

fn expected(stem: &str, prefix: &str, delay_ms: f64, cycles: usize) -> Vec<Expected> {
    let events = [
        (
            0.0,
            format!("{prefix}/{stem}/pulse"),
            vec![OscType::Int(10), OscType::Bool(true)],
        ),
        (
            1.5,
            format!("{prefix}/{stem}/level"),
            vec![OscType::Float(0.5), OscType::String("warm".into())],
        ),
        (
            3.0,
            format!("/absolute/{stem}"),
            vec![
                OscType::Int(-2),
                OscType::Float(1.25),
                OscType::String("done".into()),
                OscType::Bool(false),
            ],
        ),
    ];
    let mut result = Vec::new();
    for cycle in 0..cycles {
        for (beat, address, args) in &events {
            result.push(Expected {
                offset_ms: cycle as f64 * 4.0 * BEAT_MS + beat * BEAT_MS + delay_ms,
                address: address.clone(),
                args: args.clone(),
            });
        }
    }
    result
}

fn check_messages(
    label: &str,
    received: &[ReceivedMessage],
    expected: &[Expected],
    anchor: Instant,
) -> Vec<f64> {
    assert_eq!(received.len(), expected.len(), "{label} message count");
    let mut errors = Vec::with_capacity(received.len());
    for ((arrived, message), expected) in received.iter().zip(expected) {
        assert_eq!(message.addr, expected.address, "{label} address/order");
        assert_eq!(
            message.args, expected.args,
            "{label} typed args for {}",
            message.addr
        );
        let actual_ms = if *arrived >= anchor {
            arrived.duration_since(anchor).as_secs_f64() * 1000.0
        } else {
            -anchor.duration_since(*arrived).as_secs_f64() * 1000.0
        };
        let error = (actual_ms - expected.offset_ms).abs();
        if error >= ARRIVAL_SANITY_MS {
            eprintln!(
                "{label} {} timing error {error:.3} ms (actual {actual_ms:.3}, expected {:.3})",
                message.addr, expected.offset_ms
            );
        }
        errors.push(error);
    }
    errors
}

#[test]
fn engine_sends_typed_looping_osc_on_time_with_signed_track_delay() {
    let receiver_a = TestReceiver::bind().unwrap();
    let receiver_b = TestReceiver::bind().unwrap();
    let mut engine = Engine::with_diagnostics(
        project(receiver_a.addr().port(), receiver_b.addr().port()),
        true,
    )
    .unwrap();

    engine.execute(Command::Play).unwrap();
    for track in [TrackId(1), TrackId(2), TrackId(3)] {
        engine
            .execute(Command::LaunchClip { track, slot: 0 })
            .unwrap();
    }

    // Three cycles prove the one-bar clips loop at least twice. Receiver A also
    // captures track C, whose negative delay intentionally puts it before A.
    let received_a = receiver_a
        .wait_for_messages(CYCLES * 6, Duration::from_secs(6))
        .unwrap();
    let received_b = receiver_b
        .wait_for_messages(CYCLES * 3, Duration::from_secs(2))
        .unwrap();
    let mut arrivals: Vec<_> = received_a.iter().chain(&received_b).collect();
    arrivals.sort_by_key(|(at, _)| *at);
    let mut sends = Vec::new();
    let mut transport_anchor = None;
    while sends.len() < arrivals.len() {
        match engine
            .scheduler_events()
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
        {
            qtable_engine::scheduler::SchedulerEvent::Sent { due, lateness } => {
                sends.push((due, lateness))
            }
            qtable_engine::scheduler::SchedulerEvent::Playing(at) => transport_anchor = Some(at),
            qtable_engine::scheduler::SchedulerEvent::Error(error) => panic!("{error}"),
            _ => {}
        }
    }
    for ((arrival, message), (due, lateness)) in arrivals.iter().zip(&sends) {
        let arrival_error = arrival.duration_since(*due).as_secs_f64() * 1000.0;
        eprintln!(
            "packet {} send={:.3} ms arrival={arrival_error:.3} ms",
            message.addr,
            lateness.as_secs_f64() * 1000.0
        );
    }
    let send_mean = sends
        .iter()
        .map(|(_, late)| late.as_secs_f64() * 1000.0)
        .sum::<f64>()
        / sends.len() as f64;
    let send_max = sends
        .iter()
        .map(|(_, late)| late.as_secs_f64() * 1000.0)
        .fold(0.0, f64::max);
    eprintln!("engine send lateness: mean={send_mean:.3} ms max={send_max:.3} ms");
    // Measure against the scheduler's transport clock, so a late reference
    // packet cannot mask send lateness in all the other packets.
    let anchor = transport_anchor.expect("transport Playing notification") + Duration::from_secs(1);

    let mut expected_a = expected("a", "/alpha", 0.0, CYCLES);
    expected_a.extend(expected("c", "/alpha", -20.0, CYCLES));
    expected_a.sort_by(|left, right| left.offset_ms.total_cmp(&right.offset_ms));
    let errors_a = check_messages("receiver A", &received_a, &expected_a, anchor);
    let errors_b = check_messages(
        "receiver B",
        &received_b,
        &expected("b", "/beta", 20.0, CYCLES),
        anchor,
    );
    let errors: Vec<_> = errors_a.into_iter().chain(errors_b).collect();
    let mean = errors.iter().sum::<f64>() / errors.len() as f64;
    let max = errors.iter().copied().fold(0.0, f64::max);
    eprintln!(
        "engine arrival jitter: n={}, mean_abs={mean:.3} ms max_abs={max:.3} ms",
        errors.len()
    );
    // Send latency also includes OS preemption; strict timing is a quiet-machine
    // measurement (QTABLE_ASSERT_SEND=1), separate from functional assertions.
    send_timing::assert_send_timing(send_mean, send_max);
    // Arrival includes the OS waking the test receiver, which on a busy machine
    // (e.g. a concurrent build) reached 21 ms while sends stayed < 0.19 ms
    // (scheduler-realtime notes, 2026-10-10). So arrival is always reported but
    // only asserted when QTABLE_ASSERT_ARRIVAL=1, for measuring on a quiet machine.
    if std::env::var_os("QTABLE_ASSERT_ARRIVAL").is_some() {
        assert!(
            max < ARRIVAL_SANITY_MS,
            "max absolute arrival error {max:.3} ms"
        );
        assert!(mean < 1.0, "mean absolute arrival error {mean:.3} ms");
    }

    engine
        .execute(Command::StopTrack { track: TrackId(2) })
        .unwrap();
    engine.execute(Command::StopAll).unwrap();
    // At 240 BPM in 4/4, one bar lasts one second.
    assert!(
        receiver_a
            .wait_for_messages(1, Duration::from_secs(1))
            .is_err(),
        "receiver A got a packet after StopAll"
    );
    assert!(
        receiver_b
            .wait_for_messages(1, Duration::from_secs(1))
            .is_err(),
        "receiver B got a packet after StopTrack/StopAll"
    );
}
