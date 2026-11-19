//! Regression coverage for launch-from-stopped-starts-now (2026-10-11).
//! Exercise commands through the scheduler thread, not just Planner::launch_at.
use qtable_engine::engine::{Command, Engine};
use qtable_engine::model::*;
use qtable_engine::scheduler::SchedulerEvent;
use qtable_test_support::TestReceiver;
use std::time::{Duration, Instant};

#[path = "support/scheduler_project.rs"]
mod fixture;

fn check_launch(global: GlobalQuantisation, overrides: &[LaunchQuantisation], command: Command) {
    let receiver = TestReceiver::bind().unwrap();
    let mut project = fixture::project(receiver.addr().port(), &[0.0, 0.0]);
    project.tempo = Tempo::new(120.0).unwrap();
    project.global_quantisation = global;
    for (track, &quantisation) in project.tracks.iter_mut().zip(overrides) {
        for slot in &mut track.slots {
            let clip = slot.clip.as_mut().unwrap();
            clip.quantisation = quantisation;
            clip.end = BeatTime(3840);
            clip.loop_end = BeatTime(3840);
            clip.events.retain(|event| event.position == BeatTime::ZERO);
            clip.follow_actions.actions.enabled = true;
            clip.follow_actions.linked = false;
            clip.follow_actions.time = BeatTime(960);
        }
    }
    project.scenes[0].follow_actions.actions.enabled = true;
    project.scenes[0].follow_actions.time = BeatTime(960);
    {
        let mut group = Track::new_group(TrackId(101), "band", project.scenes.len());
        group.colour = qtable_engine::model::LIVE_CLIP_PALETTE[0];
        project.insert_group(group, &[TrackId(1), TrackId(2)]);
    }
    let count = if matches!(command, Command::LaunchClip { .. }) {
        1
    } else {
        2
    };
    let mut engine = Engine::with_diagnostics(project, true).unwrap();
    // Check both a fresh stopped engine and restart after actual playback.
    for _ in 0..2 {
        engine.execute(command.clone()).unwrap();
        let limit = Instant::now() + Duration::from_millis(300);
        let mut anchor = None;
        let mut deadlines = Vec::new();
        while deadlines.len() < count {
            match engine
                .scheduler_events()
                .recv_timeout(limit.saturating_duration_since(Instant::now()))
                .expect("launch must send before even the quarter-note grid")
            {
                SchedulerEvent::Playing(at) => anchor = Some(at),
                SchedulerEvent::Sent { due, .. } => deadlines.push(due),
                SchedulerEvent::Error(error) => panic!("{error}"),
                _ => {}
            }
        }
        let anchor = anchor.expect("launch must start stopped transport");
        assert!(deadlines.iter().all(|&due| due == anchor));
        assert_eq!(
            receiver
                .wait_for_messages(count, Duration::from_millis(300))
                .unwrap()
                .len(),
            count
        );
        loop {
            let snapshot = engine.snapshot();
            if snapshot.tracks[..count]
                .iter()
                .all(|track| track.playing_slot == Some(0))
            {
                assert!(snapshot.playing);
                for track in &snapshot.tracks[..count] {
                    assert!(track.triggered.is_none());
                    assert_eq!(track.next_follow_beat, Some(BeatTime(960)));
                }
                break;
            }
            assert!(Instant::now() < limit, "launch snapshot was not published");
            std::thread::sleep(Duration::from_millis(1));
        }
        engine.execute(Command::Stop).unwrap();
        let limit = Instant::now() + Duration::from_secs(1);
        while engine.snapshot().playing {
            assert!(Instant::now() < limit, "transport did not stop");
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

fn scene() -> Command {
    Command::LaunchScene { scene: SceneId(1) }
}
fn clip() -> Command {
    Command::LaunchClip {
        track: TrackId(1),
        slot: 0,
    }
}

#[test]
fn stopped_scene_with_global_one_bar_sends_at_transport_start() {
    check_launch(
        GlobalQuantisation::Value(QuantisationValue::Bars(1)),
        &[LaunchQuantisation::Global; 2],
        scene(),
    );
}

#[test]
fn stopped_clip_with_global_one_bar_sends_at_transport_start() {
    check_launch(
        GlobalQuantisation::Value(QuantisationValue::Bars(1)),
        &[LaunchQuantisation::Global; 2],
        clip(),
    );
}

#[test]
fn stopped_clip_with_global_four_bars_sends_at_transport_start() {
    check_launch(
        GlobalQuantisation::Value(QuantisationValue::Bars(4)),
        &[LaunchQuantisation::Global; 2],
        clip(),
    );
}

#[test]
fn stopped_scene_with_global_quarter_sends_at_transport_start() {
    check_launch(
        GlobalQuantisation::Value(QuantisationValue::Note(NoteValue::Quarter)),
        &[LaunchQuantisation::Global; 2],
        scene(),
    );
}

#[test]
fn stopped_clip_scene_and_group_ignore_explicit_clip_overrides() {
    let overrides = [
        LaunchQuantisation::Value(QuantisationValue::Bars(8)),
        LaunchQuantisation::Value(QuantisationValue::Triplet(NoteValue::Half)),
    ];
    for command in [
        clip(),
        scene(),
        Command::LaunchGroupSlot {
            group: TrackId(101),
            slot: 0,
        },
    ] {
        check_launch(
            GlobalQuantisation::Value(QuantisationValue::Bars(4)),
            &overrides,
            command,
        );
    }
}
