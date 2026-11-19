#[path = "support/send_timing.rs"]
mod send_timing;
use qtable_engine::scheduler::SchedulerEvent;
use qtable_engine::{
    engine::{Command, Engine},
    model::*,
};
use qtable_test_support::TestReceiver;
use std::time::Duration;
#[path = "support/scheduler_project.rs"]
mod fixture;
#[test]
fn three_scenes_advance_with_timed_udp() {
    let receiver = TestReceiver::bind().unwrap();
    let mut project = fixture::project(receiver.addr().port(), &[0.0]);
    project.follow_actions_enabled = true;
    project.scenes.push(Scene {
        id: SceneId(3),
        name: "three".into(),
        follow_actions: Default::default(),
    });
    let mut third = project.tracks[0].slots[0].clone();
    third.clip.as_mut().unwrap().id = ClipId(3);
    project.tracks[0].slots.push(third);
    let hits: Vec<Event> = (0..project.tracks[0].slots.len())
        .map(|index| {
            let path = OscAddress::Absolute(format!("/clip{index}"));
            project
                .hit(TrackId(1), BeatTime::ZERO, path, vec![])
                .unwrap()
        })
        .collect();
    for (slot, hit) in project.tracks[0].slots.iter_mut().zip(hits) {
        let clip = slot.clip.as_mut().unwrap();
        clip.looping = true;
        clip.events = vec![hit];
        clip.follow_actions.actions.enabled = false;
        clip.quantisation = LaunchQuantisation::None;
    }
    project.global_quantisation = GlobalQuantisation::None;
    for scene in &mut project.scenes {
        scene.follow_actions.actions.enabled = true;
        scene.follow_actions.actions.action_a = FollowAction::Next;
        scene.follow_actions.time = BeatTime(960);
    }
    let mut engine = Engine::with_diagnostics(project, true).unwrap();
    engine
        .execute(Command::SetFollowActionsEnabled(false))
        .unwrap();
    assert!(!engine.project().follow_actions_enabled);
    engine
        .execute(Command::SetFollowActionsEnabled(true))
        .unwrap();
    engine
        .execute(Command::LaunchScene { scene: SceneId(1) })
        .unwrap();
    let packets = receiver
        .wait_for_messages(7, Duration::from_secs(2))
        .unwrap();
    for (i, packet) in packets.iter().take(7).enumerate() {
        let message = &packet.1;
        assert_eq!(message.addr, format!("/clip{}", i % 3));
    }
    let mut deadlines = Vec::new();
    let mut send_errors = Vec::new();
    while deadlines.len() < 7 {
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
    let arrival_errors: Vec<f64> = packets
        .iter()
        .zip(&deadlines)
        .map(|(packet, due)| {
            packet
                .0
                .saturating_duration_since(*due)
                .max(due.saturating_duration_since(packet.0))
                .as_secs_f64()
                * 1000.0
        })
        .collect();
    let send_mean = send_errors.iter().sum::<f64>() / send_errors.len() as f64;
    let send_max = send_errors.iter().copied().fold(0.0, f64::max);
    let arrival_max = arrival_errors.iter().copied().fold(0.0, f64::max);
    eprintln!("send lateness: mean={send_mean:.3} ms max={send_max:.3} ms");
    eprintln!("follow arrival error: max_abs={arrival_max:.3} ms");
    // Send latency also includes OS preemption; strict timing is a quiet-machine
    // measurement (QTABLE_ASSERT_SEND=1), separate from functional assertions.
    send_timing::assert_send_timing(send_mean, send_max);
    if std::env::var_os("QTABLE_ASSERT_ARRIVAL").is_some() {
        assert!(
            arrival_max < 5.0,
            "UDP arrival missed follow deadline by {arrival_max:.3} ms"
        );
    }
    for pair in deadlines.windows(2) {
        assert_eq!(pair[1].duration_since(pair[0]), Duration::from_millis(100));
    }
}

#[test]
fn engine_snapshot_publishes_queued_scene_and_clears_it_on_stop() {
    // Scene choice/pulse is inferred from clip behaviour; Live check pending.
    let mut project = fixture::project(9, &[0.0]);
    project.global_quantisation = GlobalQuantisation::None;
    project.follow_actions_enabled = true;
    for slot in &mut project.tracks[0].slots {
        let clip = slot.clip.as_mut().unwrap();
        clip.events.clear();
        clip.follow_actions.actions.enabled = false;
        clip.quantisation = LaunchQuantisation::None;
    }
    project.scenes[0].follow_actions.actions.enabled = true;
    project.scenes[0].follow_actions.actions.action_a = FollowAction::Next;
    project.scenes[0].follow_actions.time = BeatTime(38_400);
    let mut engine = Engine::new(project).unwrap();
    engine
        .execute(Command::LaunchScene { scene: SceneId(1) })
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    loop {
        let snapshot = engine.snapshot();
        if snapshot.last_triggered_scene == Some(SceneId(1)) {
            assert_eq!(snapshot.queued_follow_scene, Some(SceneId(2)));
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "scene launch was not published"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    engine.execute(Command::StopAll).unwrap();
    loop {
        if engine.snapshot().queued_follow_scene.is_none() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "stop left a queued scene"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}
