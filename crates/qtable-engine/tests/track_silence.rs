use qtable_engine::model::*;
use qtable_engine::scheduler::{self, SchedulerCommand, SchedulerEvent};
use std::time::Duration;

#[path = "support/scheduler_project.rs"]
mod fixture;

#[test]
fn commands_and_edits_emit_silence_once_and_repeated_stops_are_quiet() {
    let mut project = fixture::project(9999, &[0.0]);
    project.global_quantisation = GlobalQuantisation::None;
    for slot in &mut project.tracks[0].slots {
        let clip = slot.clip.as_mut().unwrap();
        clip.events.clear();
        clip.looping = true;
    }
    project.tracks[0].slots[1].clip = None;
    {
        let mut group = Track::new_group(TrackId(101), "group", project.scenes.len());
        group.colour = qtable_engine::model::LIVE_CLIP_PALETTE[0];
        project.insert_group(group, &[TrackId(1)]);
    }
    let mut edited = project.clone();
    edited[TrackId(1)].slots[0].clip = None;
    for command in [
        SchedulerCommand::Stop,
        SchedulerCommand::StopAll,
        SchedulerCommand::StopTrack(TrackId(1)),
        SchedulerCommand::StopGroup(TrackId(101)),
        SchedulerCommand::Launch(TrackId(1), 1),
        SchedulerCommand::LaunchScene(SceneId(2)),
        SchedulerCommand::UpdateProject(edited),
    ] {
        let handle = scheduler::spawn(project.clone(), Duration::from_millis(12)).unwrap();
        handle
            .send(SchedulerCommand::Launch(TrackId(1), 0))
            .unwrap();
        assert!(matches!(
            handle.events.recv_timeout(Duration::from_secs(1)).unwrap(),
            SchedulerEvent::Playing(_)
        ));
        handle.send(command).unwrap();
        assert!(matches!(
            handle.events.recv_timeout(Duration::from_secs(1)).unwrap(),
            SchedulerEvent::TrackSilent(TrackId(1))
        ));
        // Playing is a command-order barrier after the repeated stop commands.
        handle
            .send(SchedulerCommand::StopTrack(TrackId(1)))
            .unwrap();
        handle.send(SchedulerCommand::StopAll).unwrap();
        handle.send(SchedulerCommand::Stop).unwrap();
        handle.send(SchedulerCommand::Play).unwrap();
        assert!(matches!(
            handle.events.recv_timeout(Duration::from_secs(1)).unwrap(),
            SchedulerEvent::Playing(_)
        ));
        assert!(handle.events.try_recv().is_err());
    }
}

#[test]
fn clip_follow_stop_reaches_scheduler_event_stream() {
    let mut project = fixture::project(9999, &[0.0]);
    let clip = project.tracks[0].slots[0].clip.as_mut().unwrap();
    clip.events.clear();
    clip.looping = true;
    clip.follow_actions.actions.enabled = true;
    clip.follow_actions.actions.action_a = FollowAction::Stop;
    clip.follow_actions.linked = false;
    clip.follow_actions.time = BeatTime(480);
    let handle = scheduler::spawn(project, Duration::from_millis(12)).unwrap();
    handle
        .send(SchedulerCommand::Launch(TrackId(1), 0))
        .unwrap();
    assert!(matches!(
        handle.events.recv_timeout(Duration::from_secs(1)).unwrap(),
        SchedulerEvent::Playing(_)
    ));
    assert!(matches!(
        handle.events.recv_timeout(Duration::from_secs(1)).unwrap(),
        SchedulerEvent::TrackSilent(TrackId(1))
    ));
    handle.send(SchedulerCommand::Stop).unwrap();
    handle.send(SchedulerCommand::Play).unwrap();
    assert!(matches!(
        handle.events.recv_timeout(Duration::from_secs(1)).unwrap(),
        SchedulerEvent::Playing(_)
    ));
}
