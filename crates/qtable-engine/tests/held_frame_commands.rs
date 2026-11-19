//! option-a-implementation (2026-10-21), validation revision 2, W1 and W2: the real worker with
//! commands posted every few milliseconds while a cluster of hits follows every frame tick, so the
//! cluster rule holds every ordinary frame. W1 checks every ordinary frame on each output arrives
//! complete and contiguous, with its automation sampled at its tick; W2 reports command latency and
//! hit timing and checks FIFO. Skipped ticks are reported, not asserted: a stalled worker can merge
//! ticks. Real-time numbers here are supporting evidence, not a timing bound.
use qtable_engine::model::*;
use qtable_engine::scheduler::{self, SchedulerCommand, SchedulerEvent};
use qtable_test_support::{ReceivedMessage, TestReceiver};
use std::collections::HashMap;
use std::time::{Duration, Instant};

const A: TrackId = TrackId(1001);
const B: TrackId = TrackId(1002);
/// One bar at 120 BPM: the ramp's period, so a ramp value × this is its sample time.
const BAR_MS: f64 = 2000.0;
/// At 120 BPM a 60 Hz frame period is exactly 32 clip ticks.
const TICKS_PER_FRAME: u64 = 32;

fn clip_track(
    id: u64,
    output: TrackId,
    positions: impl Iterator<Item = u64>,
    slots: usize,
) -> Track {
    let mut track = Track::new(TrackId(id), "clips", slots);
    track.route = Route::single(output);
    let definition = track.ensure_event(OscAddress::Relative(format!("/clip/{id}")), &[], || {
        DeviceId(id)
    });
    let mut clip = Clip::new(ClipId(id), "hits", BeatTime(3840), LIVE_CLIP_PALETTE[0]).unwrap();
    clip.events = positions
        .map(|position| Event {
            position: BeatTime(position),
            definition: definition.clone(),
            arguments: vec![],
        })
        .collect();
    track.slots[0].clip = Some(clip);
    track
}

/// A Target track on A ramping 16 parameters from 0 to 1 over each bar.
fn ramp_track(slots: usize) -> Track {
    let mut track = Track::new(TrackId(500), "ramp", slots);
    track.route = Route::single(A);
    let device = track.first_target_mut_or_insert(|| DeviceId(500));
    let target = device.target_mut().unwrap();
    let mut clip = Clip::new(ClipId(500), "ramp", BeatTime(3840), LIVE_CLIP_PALETTE[0]).unwrap();
    for index in 0..16 {
        let key = format!("p{index}");
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
                device: DeviceRef::Chain(DeviceId(500)),
                parameter: ParameterKey(key),
                field: FieldKey("value".into()),
            },
            points: vec![
                Breakpoint {
                    time: BeatTime(0),
                    value: 0.0,
                    curve: 0.0,
                },
                Breakpoint {
                    time: BeatTime(3840),
                    value: 1.0,
                    curve: 0.0,
                },
            ],
        }));
    }
    track.slots[0].clip = Some(clip);
    track
}

fn volumes(output: TrackId, first: u64, count: u64, slots: usize) -> impl Iterator<Item = Track> {
    (0..count).map(move |i| {
        let mut track = Track::new(TrackId(first + i), "volume", slots);
        track.route = Route::single(output);
        let volume = &mut track.mixer_device.volume.output;
        volume.enabled = true;
        volume.address = format!("/volume/{}", first + i);
        volume.send_mode = SendMode::EveryFrame;
        track
    })
}

/// A at 60 Hz: the ramp, 16 Every Frame volumes (11–26) and hits 0.5–1.6 ms after every frame
/// tick (track 1). B at 50 Hz, so its ticks are staggered against A's: 4 volumes and hits between
/// A's (track 2). Track 3 on A has no clip hits, to fire. Both outputs to their own receivers.
fn project(a: u16, b: u16, bundle: bool) -> Project {
    let mut project = Project {
        tempo: Tempo::new(120.0).unwrap(),
        scenes: vec![Scene {
            id: SceneId(1),
            name: "one".into(),
            follow_actions: SceneFollowActions::default(),
        }],
        ..Project::default()
    };
    for (id, port, rate) in [(A, a, 60.0), (B, b, 50.0)] {
        let mut output = Track::new_output(id, "rx", "127.0.0.1", port);
        let stage = &mut output.output.as_mut().unwrap().stage;
        stage.frame_rate = rate;
        stage.bundle = bundle;
        project.outputs.push(output);
    }
    let after_each_tick =
        (0..3840 / TICKS_PER_FRAME).flat_map(|k| [1, 2, 3].map(|t| k * TICKS_PER_FRAME + t));
    project.tracks.push(clip_track(1, A, after_each_tick, 1));
    project
        .tracks
        .push(clip_track(2, B, (0..3840 / 64).map(|k| k * 64 + 16), 1));
    project.tracks.push(clip_track(3, A, std::iter::empty(), 1));
    project.tracks.push(ramp_track(1));
    project.tracks.extend(volumes(A, 11, 16, 1));
    project.tracks.extend(volumes(B, 31, 4, 1));
    project
}

/// Every message a receiver has, waiting at most 200 ms for stragglers.
fn drain(receiver: &TestReceiver) -> Vec<ReceivedMessage> {
    let mut all = Vec::new();
    while let Ok(mut more) = receiver.wait_for_messages(1, Duration::from_millis(200)) {
        all.append(&mut more);
    }
    all
}

/// A frame report: output, due, message count, and whether it was an ordinary grid frame.
struct Report {
    output: TrackId,
    due: Instant,
    messages: usize,
    ordinary: bool,
}

struct Run {
    start: Instant,
    /// When posting stopped and Stop was posted.
    end: Instant,
    reports: Vec<Report>,
    applied: Vec<(Instant, Instant)>,
    sent: Vec<(Instant, Duration)>,
    postings: Vec<(Instant, bool)>,
    received: HashMap<TrackId, Vec<ReceivedMessage>>,
}

/// Launch, then post commands for `seconds`: an unchanged `UpdateProject` every 3 ms, every
/// fifth an edit of v0 (track 11), and with `fires` a fire of track 3 straight after each edit.
fn run(bundle: bool, seconds: f64, fires: bool) -> Run {
    let a = TestReceiver::bind().unwrap();
    let b = TestReceiver::bind().unwrap();
    let mut project = project(a.addr().port(), b.addr().port(), bundle);
    let handle =
        scheduler::spawn_with_grid_lead(project.clone(), Duration::from_millis(20), Duration::ZERO)
            .unwrap();
    std::thread::sleep(Duration::from_millis(300));
    handle
        .send(SchedulerCommand::LaunchScene(SceneId(1)))
        .unwrap();
    let start = loop {
        match handle.events.recv_timeout(Duration::from_secs(2)).unwrap() {
            SchedulerEvent::Playing(at) => break at,
            SchedulerEvent::Error(error) => panic!("{error}"),
            _ => {}
        }
    };
    let fire = {
        let track = project.track(TrackId(3)).unwrap();
        let target = track.devices[0].target().unwrap();
        EventRef {
            device: track.devices[0].id,
            key: target.events[0].key.clone(),
        }
    };
    let (mut reports, mut applied, mut sent, mut postings) = (vec![], vec![], vec![], vec![]);
    let mut handle_event = |event| match event {
        SchedulerEvent::FrameSent {
            output,
            due,
            messages,
            profile,
            ..
        } => reports.push(Report {
            output,
            due,
            messages,
            ordinary: profile.jump.is_none() && due != start,
        }),
        SchedulerEvent::CommandApplied {
            arrived,
            applied: at,
        } => applied.push((arrived, at)),
        SchedulerEvent::Sent { due, lateness } => sent.push((due, lateness)),
        SchedulerEvent::Error(error) => panic!("{error}"),
        _ => {}
    };
    let end = start + Duration::from_secs_f64(seconds);
    let mut posts = 0u64;
    let mut next = start + Duration::from_millis(100);
    while Instant::now() < end {
        if Instant::now() >= next {
            posts += 1;
            if posts.is_multiple_of(5) {
                let track = project
                    .tracks
                    .iter_mut()
                    .find(|t| t.id == TrackId(11))
                    .unwrap();
                track.mixer_device.volume.value = (posts % 100) as f64 / 100.0;
            }
            postings.push((Instant::now(), false));
            handle
                .send(SchedulerCommand::UpdateProject(project.clone()))
                .unwrap();
            if fires && posts.is_multiple_of(5) {
                postings.push((Instant::now(), true));
                handle
                    .send(SchedulerCommand::FireEvent(TrackId(3), fire.clone()))
                    .unwrap();
            }
            next += Duration::from_millis(3);
        }
        if let Ok(event) = handle.events.recv_timeout(Duration::from_micros(500)) {
            handle_event(event);
        }
    }
    handle.send(SchedulerCommand::Stop).unwrap();
    let until = Instant::now() + Duration::from_millis(200);
    while Instant::now() < until {
        if let Ok(event) = handle.events.recv_timeout(Duration::from_millis(5)) {
            handle_event(event);
        }
    }
    drop(handle);
    let received = HashMap::from([(A, drain(&a)), (B, drain(&b))]);
    Run {
        start,
        end,
        reports,
        applied,
        sent,
        postings,
        received,
    }
}

fn is_frame_message(address: &str) -> bool {
    address.starts_with("/parameter/") || address.starts_with("/volume/")
}

/// W1's checks on one run. Returns (ordinary frames on A, skipped ticks on A).
fn check_frames(run: &Run) -> (usize, usize) {
    // From 100 ms, until Stop: after it, Every Frame ends and automation holds its stopped value.
    let window = |due: Instant| due > run.start + Duration::from_millis(100) && due < run.end;
    for output in [A, B] {
        let stream = &run.received[&output];
        // Frames on an output go oldest first and a hit on a sending output waits for them, so
        // each frame's messages are contiguous among that output's packets.
        let mut messages = stream
            .iter()
            .enumerate()
            .filter(|(_, (_, m))| is_frame_message(&m.addr));
        for report in run
            .reports
            .iter()
            .filter(|r| r.output == output && r.messages > 0)
        {
            let frame: Vec<_> = messages.by_ref().take(report.messages).collect();
            assert_eq!(
                frame.len(),
                report.messages,
                "{output:?}: frame due {:?} arrived short",
                report.due - run.start
            );
            let (first, last) = (frame[0].0, frame[frame.len() - 1].0);
            assert_eq!(
                last - first + 1,
                frame.len(),
                "{output:?}: frame due {:?} not contiguous",
                report.due - run.start
            );
            if output == A && report.ordinary && window(report.due) {
                let ramp: Vec<f64> = frame
                    .iter()
                    .filter(|(_, (_, m))| m.addr.starts_with("/parameter/p"))
                    .map(|(_, (_, m))| match m.args.first() {
                        Some(rosc::OscType::Float(v)) => *v as f64,
                        Some(rosc::OscType::Double(v)) => *v,
                        other => panic!("{other:?}"),
                    })
                    .collect();
                let tick_ms = (report.due - run.start).as_secs_f64() * 1000.0 % BAR_MS;
                // At the ramp's wrap an exact jump sends it at the same instant, and the grid
                // frame shares that send: it carries only the volumes.
                let jumped = run.reports.iter().any(|r| {
                    r.output == A
                        && !r.ordinary
                        && r.messages > 0
                        && r.due.max(report.due) - r.due.min(report.due) < Duration::from_micros(1)
                });
                if jumped {
                    let shape = (ramp.len(), frame.len());
                    assert_eq!(
                        shape,
                        (0, 16),
                        "A frame at a jump, {tick_ms:.3} ms into its bar"
                    );
                    continue;
                }
                assert_eq!(
                    ramp.len(),
                    16,
                    "A frame due {tick_ms:.3} ms into its bar: ramp missing"
                );
                assert_eq!(frame.len(), 32, "A frame due {tick_ms:.3} ms into its bar");
                for value in ramp {
                    assert!(
                        (value * BAR_MS - tick_ms).abs() < 0.05,
                        "ramp {} for tick {tick_ms:.3}",
                        value * BAR_MS
                    );
                }
            }
        }
    }
    let ordinary: Vec<_> = run
        .reports
        .iter()
        .filter(|r| r.output == A && r.ordinary && window(r.due))
        .collect();
    let period = Duration::from_secs_f64(1.0 / 60.0);
    let skipped = ordinary
        .windows(2)
        .map(|w| ((w[1].due - w[0].due).as_secs_f64() / period.as_secs_f64()).round() as usize - 1)
        .sum();
    (ordinary.len(), skipped)
}

fn w1(bundle: bool) {
    let run = run(bundle, 3.0, false);
    let (frames, skipped) = check_frames(&run);
    assert!(frames > 150, "{frames} ordinary frames on A");
    assert!(
        run.applied.len() >= 900,
        "{} commands applied",
        run.applied.len()
    );
    eprintln!(
        "W1 Bundle {bundle}: {frames} ordinary frames on A complete; {skipped} ticks skipped (reported, not asserted); {} commands",
        run.applied.len()
    );
}

#[test]
fn w1_every_frame_arrives_complete_under_commands_bundle_off() {
    w1(false);
}

#[test]
fn w1_every_frame_arrives_complete_under_commands_bundle_on() {
    w1(true);
}

fn percentile(values: &mut [Duration], p: f64) -> Duration {
    values.sort();
    values[((values.len() as f64 * p).ceil() as usize).clamp(1, values.len()) - 1]
}

/// W2: edits with fires queued straight behind them. FIFO: every command is taken after the one
/// before applied. Reports posting- and arrival-to-application and the fires' hit lateness.
#[test]
fn w2_commands_apply_in_order_and_their_latency_is_reported() {
    let run = run(false, 3.0, true);
    check_frames(&run);
    for pair in run.applied.windows(2) {
        assert!(pair[1].0 >= pair[0].1, "FIFO: {:?}", pair);
    }
    // The launch before the postings and the Stop after them aren't among them.
    let first = run.postings[0].0;
    let applied: Vec<_> = run
        .applied
        .iter()
        .copied()
        .filter(|(arrived, _)| *arrived >= first && *arrived < run.end)
        .collect();
    assert_eq!(
        applied.len(),
        run.postings.len(),
        "every posting applied once"
    );
    let run = Run { applied, ..run };
    let mut posting: Vec<_> = run
        .postings
        .iter()
        .zip(&run.applied)
        .map(|((p, _), (_, a))| *a - *p)
        .collect();
    let mut arrival: Vec<_> = run.applied.iter().map(|(r, a)| *a - *r).collect();
    // A fire's hit (no delay) is due at the fire's arrival.
    let fired: Vec<_> = run
        .postings
        .iter()
        .zip(&run.applied)
        .filter(|((_, fire), _)| *fire)
        .map(|(_, (arrived, _))| *arrived)
        .collect();
    let mut hits: Vec<_> = run
        .sent
        .iter()
        .filter(|(due, _)| fired.contains(due))
        .map(|(_, late)| *late)
        .collect();
    assert_eq!(hits.len(), fired.len(), "every fire's hit went");
    eprintln!(
        "W2: {} commands; posting→application p50 {:?} p99 {:?} max {:?}; arrival→application p50 {:?} p99 {:?} max {:?}; fire hit lateness p50 {:?} p99 {:?} max {:?}",
        run.applied.len(),
        percentile(&mut posting, 0.5),
        percentile(&mut posting, 0.99),
        percentile(&mut posting, 1.0),
        percentile(&mut arrival, 0.5),
        percentile(&mut arrival, 0.99),
        percentile(&mut arrival, 1.0),
        percentile(&mut hits, 0.5),
        percentile(&mut hits, 0.99),
        percentile(&mut hits, 1.0),
    );
}
