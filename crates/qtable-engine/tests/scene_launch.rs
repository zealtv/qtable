use qtable_engine::engine::{Command, Engine, EngineError};
use qtable_engine::model::SceneId;
use qtable_test_support::TestReceiver;
use std::time::{Duration, Instant};

#[path = "support/scheduler_project.rs"]
mod fixture;

#[test]
fn scene_command_validates_publishes_and_sends_all_tracks_at_one_anchor() {
    let receiver = TestReceiver::bind().unwrap();
    let project = fixture::project(receiver.addr().port(), &[0.0, 0.0]);
    let scene = project.scenes[0].id;
    let mut engine = Engine::with_diagnostics(project, true).unwrap();
    assert!(matches!(
        engine.execute(Command::LaunchScene {
            scene: SceneId(999)
        }),
        Err(EngineError::UnknownScene(SceneId(999)))
    ));
    assert!(!engine.snapshot().playing);
    engine.execute(Command::LaunchScene { scene }).unwrap();
    let limit = Instant::now() + Duration::from_secs(1);
    loop {
        let snapshot = engine.snapshot();
        if snapshot.last_triggered_scene == Some(scene) {
            assert!(snapshot.playing);
            assert!(
                snapshot
                    .tracks
                    .iter()
                    .all(|t| t.playing_slot == Some(0) && t.triggered.is_none())
            );
            break;
        }
        assert!(Instant::now() < limit, "scene snapshot was not published");
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        receiver
            .wait_for_messages(6, Duration::from_secs(1))
            .unwrap()
            .len(),
        6
    );
    let mut anchor = None;
    let mut deadlines = Vec::new();
    while let Ok(event) = engine.scheduler_events().try_recv() {
        match event {
            qtable_engine::scheduler::SchedulerEvent::Playing(at) => anchor = Some(at),
            qtable_engine::scheduler::SchedulerEvent::Sent { due, .. } => deadlines.push(due),
            qtable_engine::scheduler::SchedulerEvent::Error(error) => panic!("{error}"),
            _ => {}
        }
    }
    let anchor = anchor.expect("stopped scene must start transport");
    assert_eq!(deadlines.iter().filter(|&&due| due == anchor).count(), 2);
}
