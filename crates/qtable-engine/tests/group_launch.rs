use qtable_engine::engine::{Command, Engine, EngineError};
use qtable_engine::model::Route;
use qtable_engine::model::{Track, TrackId};
use qtable_engine::scheduler::SchedulerEvent;
use qtable_test_support::TestReceiver;
use std::time::{Duration, Instant};

#[path = "support/scheduler_project.rs"]
mod fixture;

#[test]
fn group_command_validates_and_sends_only_members_at_one_transport_anchor() {
    let receiver = TestReceiver::bind().unwrap();
    let mut project = fixture::project(receiver.addr().port(), &[0.0, 0.0, 0.0]);
    {
        let mut group = Track::new_group(TrackId(101), "group", project.scenes.len());
        group.colour = qtable_engine::model::LIVE_CLIP_PALETTE[0];
        group.fold_state = true;
        project.insert_group(group, &[TrackId(1), TrackId(2)]);
    }
    let mut inner = Track::new_group(TrackId(102), "inner", project.scenes.len());
    inner.group = Some(TrackId(101));
    inner.fold_state = true;
    project[TrackId(2)].group = Some(inner.id);
    project.tracks.insert(2, inner);
    let mut invalid = project.clone();
    invalid[TrackId(2)].route = Route::Group;
    let mut engine = Engine::with_diagnostics(invalid, true).unwrap();
    engine
        .execute(Command::LaunchGroupSlot {
            group: TrackId(101),
            slot: 0,
        })
        .unwrap();
    assert!(engine.snapshot().tracks[1].empty_route);
    engine.execute(Command::LoadProject(project)).unwrap();
    assert!(matches!(
        engine.execute(Command::LaunchGroupSlot {
            group: TrackId(1099),
            slot: 0
        }),
        Err(EngineError::UnknownGroup(TrackId(1099)))
    ));
    assert!(matches!(
        engine.execute(Command::LaunchGroupSlot {
            group: TrackId(101),
            slot: 99
        }),
        Err(EngineError::UnknownSlot { .. })
    ));
    engine
        .execute(Command::LaunchGroupSlot {
            group: TrackId(101),
            slot: 0,
        })
        .unwrap();
    let limit = Instant::now() + Duration::from_secs(1);
    loop {
        let snapshot = engine.snapshot();
        if snapshot.tracks[..2]
            .iter()
            .all(|track| track.playing_slot == Some(0))
        {
            assert!(snapshot.playing);
            assert_eq!(snapshot.tracks[2].playing_slot, None);
            assert_eq!(snapshot.last_triggered_scene, None);
            break;
        }
        assert!(Instant::now() < limit, "group snapshot was not published");
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
            SchedulerEvent::Playing(at) => anchor = Some(at),
            SchedulerEvent::Sent { due, .. } => deadlines.push(due),
            SchedulerEvent::Error(error) => panic!("{error}"),
            _ => {}
        }
    }
    let anchor = anchor.expect("group starts stopped transport");
    assert_eq!(deadlines.iter().filter(|&&due| due == anchor).count(), 2);
}
