//! flush-before-planning (2026-10-20): held Bundle On hits leave before the worker plans, and hits
//! due at one instant still leave together, one bundle per output, with or without commands
//! arriving between instants.
use qtable_engine::model::*;
use qtable_engine::scheduler::{self, SchedulerCommand, SchedulerEvent};
use rosc::OscPacket;
use std::collections::BTreeSet;
use std::net::UdpSocket;
use std::time::{Duration, Instant};

const A: TrackId = TrackId(1001);
const B: TrackId = TrackId(1002);

/// A one-bar clip of sixteenths on `output`, sent to `/hit/<id>`.
fn sixteenths(id: u64, output: TrackId) -> Track {
    let mut track = Track::new(TrackId(id), "hits", 2);
    track.route = Route::single(output);
    let definition = track.ensure_event(OscAddress::Relative(format!("/hit/{id}")), &[], || {
        DeviceId(id)
    });
    let mut clip = Clip::new(ClipId(id), "hits", BeatTime(3840), LIVE_CLIP_PALETTE[0]).unwrap();
    clip.events = (0..16)
        .map(|step| Event {
            position: BeatTime(step * 240),
            definition: definition.clone(),
            arguments: vec![],
        })
        .collect();
    track.slots[0].clip = Some(clip);
    track
}

/// Two tracks on each of two Bundle On outputs, all hitting together every sixteenth.
fn project(port: u16) -> Project {
    let mut project = Project {
        tempo: Tempo::new(120.0).unwrap(),
        scenes: (1..=2)
            .map(|id| Scene {
                id: SceneId(id),
                name: format!("scene {id}"),
                follow_actions: SceneFollowActions::default(),
            })
            .collect(),
        ..Project::default()
    };
    for output in [A, B] {
        let mut track = Track::new_output(output, "rx", "127.0.0.1", port);
        track.output.as_mut().unwrap().stage.bundle = true;
        project.outputs.push(track);
    }
    for (id, output) in [(1, A), (2, A), (3, B), (4, B)] {
        project.tracks.push(sixteenths(id, output));
    }
    // Not in scene 1: slot 1 holds a clip with one hit on the downbeat, launched by test.
    let mut late = sixteenths(5, A);
    let mut clip = late.slots[0].clip.take().unwrap();
    clip.events.truncate(1);
    late.slots[1].clip = Some(clip);
    project.tracks.push(late);
    project
}

/// Play for `seconds`, calling `during` every millisecond, and return each datagram's addresses.
fn datagrams(
    seconds: f64,
    during: impl FnMut(&scheduler::SchedulerHandle, &Project),
) -> Vec<BTreeSet<String>> {
    received(seconds, during)
        .into_iter()
        .map(|(_, addresses)| addresses)
        .collect()
}

/// `datagrams`, each with its arrival.
fn received(
    seconds: f64,
    mut during: impl FnMut(&scheduler::SchedulerHandle, &Project),
) -> Vec<(Instant, BTreeSet<String>)> {
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    socket
        .set_read_timeout(Some(Duration::from_millis(1)))
        .unwrap();
    let project = project(socket.local_addr().unwrap().port());
    let handle =
        scheduler::spawn_with_diagnostics(project.clone(), Duration::from_millis(5), true).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    handle
        .send(SchedulerCommand::LaunchScene(SceneId(1)))
        .unwrap();
    let until = Instant::now() + Duration::from_secs_f64(seconds);
    let mut received = Vec::new();
    let mut buffer = [0u8; 4096];
    while Instant::now() < until {
        during(&handle, &project);
        if let Ok(size) = socket.recv(&mut buffer) {
            let (_, packet) = rosc::decoder::decode_udp(&buffer[..size]).unwrap();
            let addresses = match packet {
                OscPacket::Bundle(bundle) => bundle
                    .content
                    .into_iter()
                    .map(|p| match p {
                        OscPacket::Message(m) => m.addr,
                        OscPacket::Bundle(_) => panic!("nested bundle"),
                    })
                    .collect(),
                OscPacket::Message(m) => BTreeSet::from([m.addr]),
            };
            received.push((Instant::now(), addresses));
        }
    }
    handle.send(SchedulerCommand::Stop).unwrap();
    while let Ok(event) = handle.events.try_recv() {
        if let SchedulerEvent::Error(error) = event {
            panic!("{error}");
        }
    }
    received
}

fn assert_one_bundle_per_output_and_instant(received: &[BTreeSet<String>]) {
    let a = BTreeSet::from(["/hit/1".to_string(), "/hit/2".to_string()]);
    let b = BTreeSet::from(["/hit/3".to_string(), "/hit/4".to_string()]);
    assert!(
        received.len() >= 2 * 7,
        "about 8 sixteenths in a second: {}",
        received.len()
    );
    for datagram in received {
        assert!(
            *datagram == a || *datagram == b,
            "split or mixed: {datagram:?}"
        );
    }
    let count = |set: &BTreeSet<String>| received.iter().filter(|d| *d == set).count();
    assert!(count(&a).abs_diff(count(&b)) <= 1);
}

#[test]
fn hits_due_together_leave_as_one_bundle_per_output() {
    assert_one_bundle_per_output_and_instant(&datagrams(1.0, |_, _| {}));
}

#[test]
fn commands_arriving_between_instants_never_split_a_bundle() {
    // Each command clears the worker's pending events; the replan must not split an instant.
    let mut next = Instant::now();
    let received = datagrams(1.0, |handle, project| {
        if Instant::now() >= next {
            handle
                .send(SchedulerCommand::UpdateProject(project.clone()))
                .unwrap();
            next += Duration::from_millis(1);
        }
    });
    assert_one_bundle_per_output_and_instant(&received);
}

#[test]
fn muting_one_track_drops_only_its_messages_from_shared_bundles() {
    // track-valve-build: from the mute on, A's bundles carry track 1 alone, never a lone or
    // late /hit/2; edits keep arriving every millisecond, between and at instants.
    let mut muted_at = None;
    let start = Instant::now();
    let received = received(1.5, |handle, project| {
        if start.elapsed() < Duration::from_millis(600) {
            return;
        }
        let mut muted = project.clone();
        muted.tracks[1].mixer.active = false;
        muted_at.get_or_insert_with(Instant::now);
        handle.send(SchedulerCommand::UpdateProject(muted)).unwrap();
    });
    let muted_at = muted_at.unwrap();
    let set =
        |hits: &[u64]| -> BTreeSet<String> { hits.iter().map(|id| format!("/hit/{id}")).collect() };
    let (both, alone, b) = (set(&[1, 2]), set(&[1]), set(&[3, 4]));
    // Allow the command channel and one sixteenth for the first mute to apply.
    let settled = muted_at + Duration::from_millis(130);
    let mut after = 0;
    for (at, datagram) in &received {
        if *at >= settled {
            assert!(
                *datagram == alone || *datagram == b,
                "after mute: {datagram:?}"
            );
            after += usize::from(*datagram == alone);
        } else {
            assert!(
                *datagram == both || *datagram == alone || *datagram == b,
                "{datagram:?}"
            );
        }
    }
    assert!(after >= 5, "track 1 keeps sending: {after}");
}

/// One attempt: output A's bundles from just before the second bar to 100 ms after it, with
/// track 5 launched 0.5 ms before that bar.
fn launch_just_before_the_bar() -> Vec<BTreeSet<String>> {
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    socket
        .set_read_timeout(Some(Duration::from_millis(5)))
        .unwrap();
    let project = project(socket.local_addr().unwrap().port());
    let handle =
        scheduler::spawn_with_diagnostics(project, Duration::from_millis(5), true).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    handle
        .send(SchedulerCommand::LaunchScene(SceneId(1)))
        .unwrap();
    let anchor = loop {
        match handle.events.recv_timeout(Duration::from_secs(2)).unwrap() {
            SchedulerEvent::Playing(at) => break at,
            SchedulerEvent::Error(error) => panic!("{error}"),
            _ => {}
        }
    };
    let bar = anchor + Duration::from_secs(2);
    let mut buffer = [0u8; 4096];
    // Drop the first bar's datagrams, then wait for the moment just before the second.
    // A two-second sleep can coalesce past the bar on macOS. Draining then throws away
    // the bar's own bundles, and the late launch quantises to the following bar. Keep
    // timer slack bounded, as the worker does, so this tests held commands rather than
    // the test thread's ability to wake from a long sleep.
    let drain_at = bar - Duration::from_millis(10);
    while let Some(wait) = drain_at.checked_duration_since(Instant::now()) {
        std::thread::sleep(wait.min(Duration::from_millis(1)));
    }
    socket.set_nonblocking(true).unwrap();
    while socket.recv(&mut buffer).is_ok() {}
    socket.set_nonblocking(false).unwrap();
    while Instant::now() < bar - Duration::from_micros(500) {
        std::hint::spin_loop();
    }
    handle
        .send(SchedulerCommand::Launch(TrackId(5), 1))
        .unwrap();
    let mut received = Vec::new();
    while Instant::now() < bar + Duration::from_millis(100) {
        if let Ok(size) = socket.recv(&mut buffer) {
            let (_, packet) = rosc::decoder::decode_udp(&buffer[..size]).unwrap();
            let OscPacket::Bundle(bundle) = packet else {
                panic!("expected bundles");
            };
            let addresses: BTreeSet<String> = bundle
                .content
                .into_iter()
                .map(|p| match p {
                    OscPacket::Message(m) => m.addr,
                    OscPacket::Bundle(_) => panic!("nested bundle"),
                })
                .collect();
            if !addresses.contains("/hit/3") {
                received.push(addresses);
            }
        }
    }
    handle.send(SchedulerCommand::Stop).unwrap();
    received
}

#[test]
fn a_hit_a_held_command_adds_at_an_instant_already_sent_goes_in_its_own_bundle() {
    // Documents behaviour from before flush-before-planning, kept by it: launch track 5 just
    // before the bar. The command waits while the bar's hits go (Hold), and the bar's bundle
    // leaves on the next pass, before the command applies; the command then launches as of its
    // arrival, so its downbeat hit is due at the bar too, and leaves alone, at once.
    let set = |addresses: &[&str]| {
        addresses
            .iter()
            .map(|a| a.to_string())
            .collect::<BTreeSet<_>>()
    };
    // Output A around the bar: the bar's own bundle intact, then the launched hit.
    // Under load the launch can miss the bar; it then quantises to the next one, outside the
    // window, and the attempt shows only the bar's own bundle. Retry those, up to three times.
    for _ in 0..3 {
        let received = launch_just_before_the_bar();
        if received == [set(&["/hit/1", "/hit/2"])] {
            continue;
        }
        assert_eq!(received, [set(&["/hit/1", "/hit/2"]), set(&["/hit/5"])]);
        return;
    }
    panic!("the launch missed the bar in every attempt");
}
