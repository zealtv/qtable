//! complete-clusters-before-frames (2026-10-20), gate 1 test 16: with frames due every tick, no
//! clip hit on the frame's output waits behind a frame that started part-way through its cluster,
//! with Bundle Off or On. Clusters fall every ~15.1 ms against a 16.7 ms frame period, so over a
//! bar their ticks fall at every phase, including 1 ms before the cluster's middle.
use qtable_engine::model::*;
use qtable_engine::scheduler::{self, SchedulerCommand, SchedulerEvent};
use qtable_test_support::TestReceiver;
use std::time::Duration;

const A: TrackId = TrackId(1001);
const B: TrackId = TrackId(1002);
/// Four hit tracks on A (one cluster spans 0.9 ms), two on B between them.
const HITS: [(u64, TrackId, f64); 6] = [
    (1, A, 0.1),
    (2, A, 0.4),
    (3, A, 0.7),
    (4, A, 1.0),
    (5, B, 0.25),
    (6, B, 0.55),
];
/// Clusters at every 29th tick of a bar (~15.1 ms apart at 120 BPM).
const CLUSTERS: u64 = 132;

fn project(port: u16, bundle: bool) -> Project {
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
        let stage = &mut track.output.as_mut().unwrap().stage;
        stage.frame_rate = 60.0;
        stage.bundle = bundle;
        project.outputs.push(track);
    }
    for (id, output, delay) in HITS {
        let mut track = Track::new(TrackId(id), "hits", 1);
        track.route = Route::single(output);
        track.delay_ms = delay;
        let definition =
            track.ensure_event(OscAddress::Relative(format!("/hit/{id}")), &[], || {
                DeviceId(id)
            });
        let mut clip = Clip::new(ClipId(id), "hits", BeatTime(3840), LIVE_CLIP_PALETTE[0]).unwrap();
        clip.events = (1..=CLUSTERS)
            .map(|k| Event {
                position: BeatTime(k * 29),
                definition: definition.clone(),
                arguments: vec![],
            })
            .collect();
        track.slots[0].clip = Some(clip);
        project.tracks.push(track);
    }
    for i in 0..4 {
        let mut track = Track::new(TrackId(10 + i), "parameter", 1);
        track.route = Route::single(A);
        let output = &mut track.mixer_device.volume.output;
        output.enabled = true;
        output.address = format!("/parameter/{i}");
        output.send_mode = SendMode::EveryFrame;
        project.tracks.push(track);
    }
    project
}

/// One attempt, ~3 s from a launch: the clusters on A, and those a parameter message arrived
/// inside (after A's first hit and before its last).
fn attempt(bundle: bool) -> (usize, Vec<usize>) {
    let receiver = TestReceiver::bind().unwrap();
    let handle = scheduler::spawn(
        project(receiver.addr().port(), bundle),
        Duration::from_millis(20),
    )
    .unwrap();
    std::thread::sleep(Duration::from_millis(50));
    handle
        .send(SchedulerCommand::LaunchScene(SceneId(1)))
        .unwrap();
    // ~400 hits and ~240 parameter messages a second.
    let messages = receiver
        .wait_for_messages(1_800, Duration::from_secs(6))
        .unwrap();
    while let Ok(event) = handle.events.try_recv() {
        if let SchedulerEvent::Error(error) = event {
            panic!("{error}");
        }
    }
    let (mut clusters, mut split, mut open) = (0, Vec::new(), false);
    for (_, message) in messages {
        match message.addr.as_str() {
            "/hit/1" => {
                clusters += 1;
                open = true;
            }
            "/hit/4" => open = false,
            address
                if open
                    && address.starts_with("/parameter/")
                    && split.last() != Some(&clusters) =>
            {
                split.push(clusters);
            }
            _ => {}
        }
    }
    (clusters, split)
}

/// Real time under `cargo test` load can delay the worker past the cap, which releases a frame
/// mid-cluster by design; up to three attempts, one of which must split nothing.
fn no_cluster_split(bundle: bool) {
    let mut attempts = Vec::new();
    for _ in 0..3 {
        let (clusters, split) = attempt(bundle);
        assert!(clusters > 150, "{clusters} clusters");
        if split.is_empty() {
            return;
        }
        attempts.push((clusters, split));
    }
    panic!("Bundle {bundle}: clusters split in every attempt (clusters, split ones): {attempts:?}");
}

#[test]
fn no_hit_waits_behind_a_frame_started_mid_cluster_bundle_off() {
    no_cluster_split(false);
}

#[test]
fn no_hit_waits_behind_a_frame_started_mid_cluster_bundle_on() {
    no_cluster_split(true);
}
