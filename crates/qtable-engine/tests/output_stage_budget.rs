//! Budget, Bundle and Refresh on the wire (output-stage-design, 2026-10-15).
use qtable_engine::engine::{Command, Engine};
use qtable_engine::model::*;
use qtable_engine::scheduler::{self, SchedulerCommand, SchedulerEvent};
use qtable_test_support::TestReceiver;
use rosc::{OscPacket, OscType};
use std::collections::HashSet;
use std::net::UdpSocket;
use std::time::{Duration, Instant};

#[path = "support/scheduler_project.rs"]
mod fixture;

/// `count` tracks with an enabled Volume, all routed to one output with the given stage.
fn parameters(port: u16, count: u64, stage: OutputStage) -> Project {
    let mut output = Track::new_output(TrackId(2000), "Out", "127.0.0.1", port);
    output.output.as_mut().unwrap().stage = stage;
    let mut project = Project {
        outputs: vec![output],
        ..Project::default()
    };
    for id in 1..=count {
        let mut track = Track::new(TrackId(id), "Track", 0);
        track.route = Route::single(TrackId(2000));
        track.mixer_device.volume.output.enabled = true;
        project.tracks.push(track);
    }
    project
}

/// Track `id`'s edit value, distinct per track and exact in f32 after rounding to thousandths.
fn edit_value(id: u64) -> f64 {
    id as f64 / 1000.0
}

fn thousandths(message: &rosc::OscMessage) -> u64 {
    let [OscType::Float(value)] = message.args.as_slice() else {
        panic!("expected a float");
    };
    (f64::from(*value) * 1000.0).round() as u64
}

#[test]
fn a_budget_caps_parameter_messages_and_every_final_value_still_arrives() {
    let receiver = TestReceiver::bind().unwrap();
    let stage = OutputStage {
        budget_on: true,
        budget: 100,
        ..OutputStage::default()
    };
    let mut project = parameters(receiver.addr().port(), 400, stage);
    let handle = scheduler::spawn(project.clone(), Duration::from_millis(12)).unwrap();
    // Submit whole rounds, as the scheduler receives them. Hundreds of individual engine
    // edits queue whole-project replacements and can consume this test's entire wait
    // before the stage gets a frame; that measures command backlog, not the Budget.
    // Two rounds of changes, so the first values are overwritten before they can all go.
    for offset in [0.5, 0.0] {
        for track in &mut project.tracks {
            track.mixer_device.volume.value = offset + edit_value(track.id.0);
        }
        handle
            .send(SchedulerCommand::UpdateProject(project.clone()))
            .unwrap();
    }
    let started = Instant::now();
    let mut arrivals = vec![];
    let mut values = HashSet::new();
    while started.elapsed() < Duration::from_secs(5) && values.len() < 400 {
        if let Ok(received) = receiver.wait_for_messages(1, Duration::from_millis(200)) {
            let (at, message) = &received[0];
            let value = thousandths(message);
            if (1..=400).contains(&value) {
                values.insert(value);
            }
            // Include any intermediate values in the rate limit, too.
            arrivals.push((*at, value));
        }
    }
    assert_eq!(values.len(), 400, "{} messages arrived", arrivals.len());
    eprintln!(
        "budget: {} final values, {} messages in {:?}",
        values.len(),
        arrivals.len(),
        started.elapsed()
    );
    for (index, (at, _)) in arrivals.iter().enumerate() {
        let within = arrivals[index..]
            .iter()
            .take_while(|(later, _)| *later < *at + Duration::from_secs(1))
            .count();
        assert!(within <= 110, "{within} messages inside one second");
    }
}

#[test]
fn clip_events_all_arrive_on_time_while_a_budget_is_saturated() {
    let receiver = TestReceiver::bind().unwrap();
    let mut project = fixture::project(receiver.addr().port(), &[0.0]);
    project.outputs[0].output.as_mut().unwrap().stage = OutputStage {
        budget_on: true,
        budget: 10,
        ..OutputStage::default()
    };
    let clip = project.tracks[0].slots[0].clip.as_mut().unwrap();
    clip.looping = true;
    for id in 2..=21 {
        let mut track = Track::new(TrackId(id), "Volume", 2);
        track.route = Route::single(TrackId(201));
        track.mixer_device.volume.output.enabled = true;
        project.tracks.push(track);
    }
    let handle =
        scheduler::spawn_with_diagnostics(project.clone(), Duration::from_millis(12), true)
            .unwrap();
    handle
        .send(SchedulerCommand::Launch(TrackId(1), 0))
        .unwrap();
    // Every 20 ms the volumes change; three events a beat (100 ms) alone exceed 10 messages/s.
    let started = Instant::now();
    let mut step = 0;
    while started.elapsed() < Duration::from_millis(1500) {
        step += 1;
        for track in &mut project.tracks[1..] {
            track.mixer_device.volume.value = f64::from(step % 100) / 100.0;
        }
        handle
            .send(SchedulerCommand::UpdateProject(project.clone()))
            .unwrap();
        std::thread::sleep(Duration::from_millis(20));
    }
    handle.send(SchedulerCommand::Stop).unwrap();
    let mut events = 0;
    let mut worst = Duration::ZERO;
    while let Ok(event) = handle.events.recv_timeout(Duration::from_millis(200)) {
        if let SchedulerEvent::Sent { lateness, .. } = event {
            events += 1;
            worst = worst.max(lateness);
        }
    }
    let mut arrived = 0;
    while let Ok(received) = receiver.wait_for_messages(1, Duration::from_millis(100)) {
        if received[0].1.addr.starts_with("/t1/") {
            arrived += 1;
        }
    }
    // Three events a 100 ms beat for 1.5 s, less launch quantisation and the first beat: every
    // one that was due arrived, and none was sent late.
    assert!(
        arrived >= 36,
        "{arrived} clip events arrived of {events} sends"
    );
    assert!(worst < Duration::from_millis(5), "worst lateness {worst:?}");
}

#[test]
fn bundled_datagrams_stay_within_the_mtu_and_carry_immediate_timetags() {
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    socket
        .set_read_timeout(Some(Duration::from_millis(300)))
        .unwrap();
    let port = socket.local_addr().unwrap().port();
    let stage = OutputStage {
        bundle: true,
        ..OutputStage::default()
    };
    let mut engine = Engine::new(parameters(port, 400, stage)).unwrap();
    // Play sends every value at once, so all 400 messages share one stage pass.
    engine.execute(Command::Play).unwrap();
    let (mut datagrams, mut messages, mut buffer) = (0, 0, [0; 4096]);
    while let Ok(size) = socket.recv(&mut buffer) {
        assert!(size <= 1472, "{size} bytes");
        let (_, OscPacket::Bundle(bundle)) = rosc::decoder::decode_udp(&buffer[..size]).unwrap()
        else {
            panic!("expected a bundle");
        };
        assert_eq!(bundle.timetag, (0, 1).into());
        datagrams += 1;
        for packet in bundle.content {
            let OscPacket::Message(message) = packet else {
                panic!("nested bundle");
            };
            assert_eq!(message.addr, "/volume");
            messages += 1;
        }
    }
    assert_eq!(messages, 400);
    assert!((6..=12).contains(&datagrams), "{datagrams} datagrams");
}

#[test]
fn refresh_resends_each_value_about_once_a_second_while_stopped() {
    let receiver = TestReceiver::bind().unwrap();
    let stage = OutputStage {
        refresh_on: true,
        refresh: 1.0,
        ..OutputStage::default()
    };
    let _engine = Engine::new(parameters(receiver.addr().port(), 3, stage)).unwrap();
    let started = Instant::now();
    let mut refreshes = 0;
    while started.elapsed() < Duration::from_millis(3300) {
        if let Ok(received) = receiver.wait_for_messages(1, Duration::from_millis(100)) {
            assert_eq!(received[0].1.addr, "/volume");
            refreshes += 1;
        }
    }
    // Three values, three passes in 3.3 s: nine repeats, give or take one pass's edge.
    assert!((8..=10).contains(&refreshes), "{refreshes} refreshes");
}
