//! cross-output-scheduling (2026-10-20): while one output's frame is part-way out, a clip hit due on
//! another output goes between its packets, and a hit on the frame's own output still waits for
//! every parameter before it.
use qtable_engine::model::*;
use qtable_engine::scheduler::{self, SchedulerCommand, SchedulerEvent};
use qtable_test_support::TestReceiver;
use std::time::Duration;

const A: TrackId = TrackId(1001);
const B: TrackId = TrackId(1002);
const PARAMETERS: u64 = 200;

fn clip_track(id: u64, output: TrackId, delay_ms: f64) -> Track {
    let mut track = Track::new(TrackId(id), "hits", 1);
    track.route = Route::single(output);
    track.delay_ms = delay_ms;
    let definition = track.ensure_event(OscAddress::Relative(format!("/hit/{id}")), &[], || {
        DeviceId(id)
    });
    let mut clip = Clip::new(ClipId(id), "hit", BeatTime(3840), LIVE_CLIP_PALETTE[0]).unwrap();
    clip.events = vec![Event {
        position: BeatTime::ZERO,
        definition,
        arguments: vec![],
    }];
    track.slots[0].clip = Some(clip);
    track
}

/// A launch from stopped: output A owes 200 parameter values (200 datagrams with Bundle Off); a
/// hit on B is due 0.1 ms in, and one on A 0.2 ms in. Plain: 200 volumes, sent as transport-start
/// values. `automated`: two Target tracks whose clips drive 100 parameters each, as in the
/// measurement harness's arm i, so the launch is also two exact jumps due at once. Returns
/// addresses in arrival order.
fn launch_order(automated: bool) -> Vec<String> {
    launch_order_with(automated, 0.1, 0.2, false)
}

/// As `launch_order`, with B's hit due `b_delay` ms in and A's `a_delay` ms in. `shared`: one
/// voice instead, track 1 routed to both outputs, its B copy later by B's output delay.
fn launch_order_with(automated: bool, b_delay: f64, a_delay: f64, shared: bool) -> Vec<String> {
    let receiver = TestReceiver::bind().unwrap();
    let port = receiver.addr().port();
    let mut project = Project {
        tempo: Tempo::new(120.0).unwrap(),
        scenes: vec![Scene {
            id: SceneId(1),
            name: "one".into(),
            follow_actions: SceneFollowActions::default(),
        }],
        ..Project::default()
    };
    for output in [A, B] {
        project
            .outputs
            .push(Track::new_output(output, "rx", "127.0.0.1", port));
    }
    if shared {
        let mut track = clip_track(1, A, a_delay);
        track.route = Route::Outputs(vec![A, B]);
        project.tracks.push(track);
        project.outputs[1].delay_ms = b_delay - a_delay;
    } else {
        project.tracks.push(clip_track(1, B, b_delay));
        project.tracks.push(clip_track(2, A, a_delay));
    }
    if automated {
        for (n, keys) in [(0, 0..PARAMETERS / 2), (1, PARAMETERS / 2..PARAMETERS)] {
            let device_id = DeviceId(500 + n);
            let mut track = Track::new(TrackId(500 + n), "envelopes", 1);
            track.route = Route::single(A);
            let device = track.first_target_mut_or_insert(|| device_id);
            let target = device.target_mut().unwrap();
            let mut clip = Clip::new(
                ClipId(500 + n),
                "automation",
                BeatTime(3840),
                LIVE_CLIP_PALETTE[0],
            )
            .unwrap();
            for i in keys {
                let key = format!("p{i}");
                target.parameters.push(ParameterDef {
                    key: EntryKey(key.clone()),
                    name: String::new(),
                    path: OscAddress::Relative(format!("/parameter/{key}")),
                    fields: vec![FieldDef::new(
                        FieldKey("value".into()),
                        FieldType::Float,
                        true,
                    )],
                    send_mode: SendMode::Continuous,
                });
                clip.lanes.push(Lane::Envelope(AutomationEnvelope {
                    region: None,
                    target: ArgumentRef {
                        device: DeviceRef::Chain(device_id),
                        parameter: ParameterKey(key),
                        field: FieldKey("value".into()),
                    },
                    points: vec![Breakpoint {
                        time: BeatTime::ZERO,
                        value: 0.25,
                        curve: 0.0,
                    }],
                }));
            }
            track.slots[0].clip = Some(clip);
            project.tracks.push(track);
        }
    } else {
        for i in 0..PARAMETERS {
            let mut track = Track::new(TrackId(10 + i), "parameter", 1);
            track.route = Route::single(A);
            let output = &mut track.mixer_device.volume.output;
            output.enabled = true;
            output.address = format!("/parameter/{i}");
            project.tracks.push(track);
        }
    }
    let handle =
        scheduler::spawn_with_diagnostics(project, Duration::from_millis(5), true).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    handle
        .send(SchedulerCommand::LaunchScene(SceneId(1)))
        .unwrap();
    let messages = receiver
        .wait_for_messages(PARAMETERS as usize + 2, Duration::from_secs(2))
        .unwrap();
    while let Ok(event) = handle.events.try_recv() {
        if let SchedulerEvent::Error(error) = event {
            panic!("{error}");
        }
    }
    messages.into_iter().map(|(_, m)| m.addr).collect()
}

fn position(order: &[String], address: &str) -> usize {
    order
        .iter()
        .position(|a| a == address)
        .unwrap_or_else(|| panic!("no {address} in {order:?}"))
}

#[test]
fn a_hit_due_on_another_output_goes_between_a_frames_packets() {
    let order = launch_order(false);
    let last_parameter = order
        .iter()
        .rposition(|a| a.starts_with("/parameter/"))
        .unwrap();
    assert!(position(&order, "/hit/1") < last_parameter, "{order:?}");
}

#[test]
fn a_hit_due_on_another_output_goes_between_an_exact_jumps_packets() {
    let order = launch_order(true);
    let last_parameter = order
        .iter()
        .rposition(|a| a.starts_with("/parameter/"))
        .unwrap();
    assert!(position(&order, "/hit/1") < last_parameter, "{order:?}");
}

#[test]
fn a_hit_on_the_frames_own_output_waits_for_every_parameter() {
    let order = launch_order(true);
    let last_parameter = order
        .iter()
        .rposition(|a| a.starts_with("/parameter/"))
        .unwrap();
    assert!(position(&order, "/hit/2") > last_parameter, "{order:?}");
    assert_eq!(
        order
            .iter()
            .filter(|a| a.starts_with("/parameter/"))
            .count(),
        PARAMETERS as usize
    );
}

#[test]
fn a_hit_due_on_another_output_passes_a_hit_waiting_for_its_frame() {
    // hits-pass-a-blocked-front: A's hit is due first and waits for A's frame; B's hit, due
    // after it, no longer waits behind it.
    for automated in [false, true] {
        let order = launch_order_with(automated, 0.2, 0.1, false);
        let last_parameter = order
            .iter()
            .rposition(|a| a.starts_with("/parameter/"))
            .unwrap();
        assert!(position(&order, "/hit/1") < last_parameter, "{order:?}");
        assert!(position(&order, "/hit/2") > last_parameter, "{order:?}");
    }
}

#[test]
fn a_voice_routed_to_both_outputs_keeps_its_order() {
    // The same voice's B copy is due after its A copy, which waits for A's frame: the B copy
    // doesn't pass it, so both leave after the frame.
    for automated in [false, true] {
        let order = launch_order_with(automated, 0.2, 0.1, true);
        let last_parameter = order
            .iter()
            .rposition(|a| a.starts_with("/parameter/"))
            .unwrap();
        let hits: Vec<_> = (0..order.len()).filter(|&i| order[i] == "/hit/1").collect();
        assert_eq!(hits.len(), 2, "{order:?}");
        assert!(hits.iter().all(|&i| i > last_parameter), "{order:?}");
    }
}

// backlog-turns-implementation (bundle-off-b-residue validation revision 2, section 2): after A's
// exact jump completes with A's hits due behind it (A's backlog), the frame was A's turn, so a B
// hit already due goes next, and while the backlog drains the outputs alternate. Asserted only
// where the recorded timeline shows the case arose; real-time numbers are supporting evidence,
// not a bound.

/// The ramp's clip: 480 ticks, so A jumps every 0.25 s at 120 BPM.
const WRAP: u64 = 480;
/// A's hits, ten from each jump to 0.09 ms: they always wait for its frame, a long backlog.
fn a_delays() -> Vec<f64> {
    (0..10).map(|i| i as f64 * 0.01).collect()
}

/// B's hits every 100 µs from 0.1 to 4 ms, so some fall due around a completion and while A's
/// backlog drains, however long the jump takes (a debug build's ran 1.7–11 ms).
fn b_delays() -> Vec<f64> {
    (1..=40).map(|i| i as f64 * 0.1).collect()
}

fn looping_track(id: u64, output: TrackId, delay_ms: f64) -> Track {
    let mut track = clip_track(id, output, delay_ms);
    let clip = track.slots[0].clip.as_mut().unwrap();
    clip.end = BeatTime(WRAP);
    clip.loop_end = BeatTime(WRAP);
    track
}

fn backlog_project(port: u16, bundle: bool) -> Project {
    let mut project = Project {
        tempo: Tempo::new(120.0).unwrap(),
        scenes: vec![Scene {
            id: SceneId(1),
            name: "one".into(),
            follow_actions: SceneFollowActions::default(),
        }],
        ..Project::default()
    };
    for output in [A, B] {
        let mut track = Track::new_output(output, "rx", "127.0.0.1", port);
        track.output.as_mut().unwrap().stage.bundle = bundle;
        project.outputs.push(track);
    }
    for (i, delay) in a_delays().into_iter().enumerate() {
        project.tracks.push(looping_track(i as u64 + 1, A, delay));
    }
    for (i, delay) in b_delays().into_iter().enumerate() {
        project.tracks.push(looping_track(i as u64 + 100, B, delay));
    }
    let device_id = DeviceId(500);
    let mut track = Track::new(TrackId(500), "envelopes", 1);
    track.route = Route::single(A);
    let device = track.first_target_mut_or_insert(|| device_id);
    let target = device.target_mut().unwrap();
    let mut clip = Clip::new(ClipId(500), "ramp", BeatTime(WRAP), LIVE_CLIP_PALETTE[0]).unwrap();
    for i in 0..16 {
        let key = format!("p{i}");
        target.parameters.push(ParameterDef {
            key: EntryKey(key.clone()),
            name: String::new(),
            path: OscAddress::Relative(format!("/parameter/{key}")),
            fields: vec![FieldDef::new(
                FieldKey("value".into()),
                FieldType::Float,
                true,
            )],
            send_mode: SendMode::Continuous,
        });
        clip.lanes.push(Lane::Envelope(AutomationEnvelope {
            region: None,
            target: ArgumentRef {
                device: DeviceRef::Chain(device_id),
                parameter: ParameterKey(key),
                field: FieldKey("value".into()),
            },
            points: vec![
                Breakpoint {
                    time: BeatTime::ZERO,
                    value: 0.0,
                    curve: 0.0,
                },
                Breakpoint {
                    time: BeatTime(WRAP),
                    value: 1.0,
                    curve: 0.0,
                },
            ],
        }));
    }
    track.slots[0].clip = Some(clip);
    project.tracks.push(track);
    project
}

/// One recorded hit: its output, due time and send (just before its datagram's send call).
#[derive(Clone, Copy, Debug)]
struct Hit {
    output: TrackId,
    due: std::time::Instant,
    sent: std::time::Instant,
}

/// Run the backlog project for ~3 s; returns each exact jump on A (its due time and completion)
/// with the hits due within 5 ms after it.
fn backlog_run(bundle: bool) -> Vec<(std::time::Instant, std::time::Instant, Vec<Hit>)> {
    let receiver = TestReceiver::bind().unwrap();
    let project = backlog_project(receiver.addr().port(), bundle);
    let handle =
        scheduler::spawn_with_diagnostics(project, Duration::from_millis(5), true).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    handle
        .send(SchedulerCommand::LaunchScene(SceneId(1)))
        .unwrap();
    let until = std::time::Instant::now() + Duration::from_millis(3300);
    let (mut jumps, mut sends) = (Vec::new(), Vec::new());
    while std::time::Instant::now() < until {
        let Ok(event) = handle.events.recv_timeout(Duration::from_millis(50)) else {
            continue;
        };
        match event {
            SchedulerEvent::Error(error) => panic!("{error}"),
            SchedulerEvent::FrameSent {
                output,
                due,
                lateness,
                profile,
                ..
            } if output == A && profile.jump.is_some() => jumps.push((due, due + lateness)),
            SchedulerEvent::FrameSent {
                output,
                due,
                lateness,
                ..
            } if std::env::var("BACKLOG_TRACE").is_ok() => {
                eprintln!("frame {output:?} due {due:?} completed +{:?}", lateness);
            }
            SchedulerEvent::Sent { due, lateness } => sends.push((due, due + lateness)),
            _ => {}
        }
    }
    drop(receiver);
    let us = |d: f64| Duration::from_nanos((d * 1e6).round() as u64);
    jumps
        .into_iter()
        .map(|(due, completed)| {
            let hits = sends
                .iter()
                .filter(|&&(at, _)| at >= due && at < due + Duration::from_millis(5))
                .map(|&(at, sent)| {
                    let offset = at - due;
                    let near = |d: f64| offset.abs_diff(us(d)) < Duration::from_micros(2);
                    let output = if a_delays().iter().any(|&d| near(d)) {
                        A
                    } else if b_delays().iter().any(|&d| near(d)) {
                        B
                    } else {
                        panic!("a hit {offset:?} after a jump on no known track")
                    };
                    Hit {
                        output,
                        due: at,
                        sent,
                    }
                })
                .collect();
            (due, completed, hits)
        })
        .collect()
}

/// Per jump, the cases that arose and any violations: (cases, violations).
fn backlog_cases(bundle: bool) -> (usize, Vec<String>) {
    let (mut cases, mut violations) = (0, Vec::new());
    let runs = backlog_run(bundle);
    assert!(runs.len() >= 4, "{} jumps", runs.len());
    for (due, completed, hits) in runs {
        let mut order = hits.clone();
        order.sort_by_key(|h| h.sent);
        if std::env::var("BACKLOG_TRACE").is_ok() {
            eprintln!("jump {due:?}: completed {:?}", completed - due);
            for h in order
                .iter()
                .filter(|h| h.sent + Duration::from_micros(300) >= completed)
                .take(30)
            {
                eprintln!(
                    "  {:?} due {:?} sent {:?}",
                    h.output,
                    h.due - due,
                    h.sent - due
                );
            }
        }
        for output in [A, B] {
            let dues: Vec<_> = order
                .iter()
                .filter(|h| h.output == output)
                .map(|h| h.due)
                .collect();
            if !dues.windows(2).all(|w| w[0] <= w[1]) {
                violations.push(format!("jump at {due:?}: {output:?} out of order"));
            }
        }
        // A's backlog: its hits due by the completion and still unsent then.
        let backlog: Vec<_> = order
            .iter()
            .filter(|h| h.output == A && h.due <= completed && h.sent >= completed)
            .collect();
        if backlog.is_empty() {
            continue;
        }
        let drained = backlog.last().unwrap().sent;
        // A B hit due and unsent at the completion goes before A's backlog.
        if let Some(b) = order
            .iter()
            .find(|h| h.output == B && h.due <= completed && h.sent >= completed)
        {
            cases += 1;
            if b.sent > backlog[0].sent {
                violations.push(format!(
                    "jump at {due:?}: B's hit due {:?} before the completion went after A's backlog head",
                    completed - b.due
                ));
            }
        }
        // While A's backlog drains, outputs alternate: two of A's sends in a row only while no B
        // hit was waiting. The second was chosen after the first's send, so a B hit due before
        // that and sent after the second waited out a turn it should have had. (B may also wait
        // behind its own earlier hits: per-output order.)
        cases += order
            .iter()
            .filter(|h| h.output == B && h.due > completed && h.due < drained)
            .count();
        let draining: Vec<_> = order
            .iter()
            .filter(|h| h.sent >= completed && h.sent <= drained)
            .collect();
        for pair in draining.windows(2) {
            let (first, second) = (pair[0], pair[1]);
            if first.output != A || second.output != A {
                continue;
            }
            if let Some(b) = order
                .iter()
                .find(|h| h.output == B && h.due < first.sent && h.sent > second.sent)
            {
                violations.push(format!(
                    "jump at {due:?}: A sent twice in a row ({:?}, {:?} after the completion) while B's hit due {:?} after it waited",
                    first.sent - completed,
                    second.sent - completed,
                    b.due.saturating_duration_since(completed)
                ));
            }
        }
    }
    (cases, violations)
}

#[test]
fn after_a_frame_completes_a_due_hit_on_another_output_takes_the_next_turn() {
    for bundle in [false, true] {
        let (cases, violations) = backlog_cases(bundle);
        eprintln!("bundle {bundle}: {cases} cases");
        assert!(cases > 0, "bundle {bundle}: the case never arose");
        assert!(violations.is_empty(), "bundle {bundle}: {violations:#?}");
    }
}
