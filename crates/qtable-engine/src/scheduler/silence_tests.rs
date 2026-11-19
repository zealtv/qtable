//! Silence transitions for every planner path, including retained ended voices.
use super::*;
use crate::model::*;

fn setup(
    delay: f64,
    looping: bool,
    action: Option<FollowAction>,
) -> (Instant, InternalClock, Planner) {
    let mut project = super::tests::project(delay, looping);
    if let Some(action) = action {
        let clip = project.tracks[0].slots[0].clip.as_mut().unwrap();
        clip.follow_actions.actions.enabled = true;
        clip.follow_actions.actions.action_a = action;
        clip.follow_actions.linked = false;
        clip.follow_actions.time = BeatTime(480);
    }
    let start = Instant::now();
    let mut clock = InternalClock::default();
    clock.play(start);
    let mut planner = Planner::new(project, Duration::ZERO).unwrap();
    planner.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
    (start, clock, planner)
}

fn advance(planner: &mut Planner, clock: &InternalClock, now: Instant) -> Vec<TrackId> {
    // Re-plan after each consumption, as a follow action may create new voices.
    let mut silent = Vec::new();
    while let Some(event) = planner.plan(clock, now).into_iter().find(|e| e.at <= now) {
        silent.extend(planner.consume(&event));
    }
    silent
}

#[test]
fn clip_follow_stop_and_empty_jump_report_once() {
    for action in [FollowAction::Stop, FollowAction::Jump(SceneId(2))] {
        let (start, clock, mut planner) = setup(0.0, true, Some(action));
        if matches!(action, FollowAction::Jump(_)) {
            Arc::make_mut(&mut planner.project).tracks[0].slots[1].clip = None;
        }
        assert_eq!(
            advance(&mut planner, &clock, start + Duration::from_millis(250)),
            [TrackId(1)]
        );
        assert!(advance(&mut planner, &clock, start + Duration::from_secs(1)).is_empty());
    }
}

#[test]
fn clip_end_reports_once_even_when_voice_remains_for_packets_or_follow() {
    for (delay, action) in [(1000.0, None), (0.0, Some(FollowAction::Stop))] {
        let (start, clock, mut planner) = setup(delay, false, action);
        if action.is_some() {
            Arc::make_mut(&mut planner.project).tracks[0].slots[0]
                .clip
                .as_mut()
                .unwrap()
                .follow_actions
                .time = BeatTime(1920);
        }
        assert_eq!(
            advance(&mut planner, &clock, start + Duration::from_millis(500)),
            [TrackId(1)]
        );
        assert!(!planner.voices.is_empty());
        assert!(advance(&mut planner, &clock, start + Duration::from_secs(2)).is_empty());
        assert!(planner.voices.is_empty());
    }
}

#[test]
fn replacement_keeps_track_active_and_failed_follow_successor_reports_retirement() {
    let (start, clock, mut planner) = setup(0.0, true, None);
    planner.launch_at(TrackId(1), 1, BeatTime(480)).unwrap();
    assert!(advance(&mut planner, &clock, start + Duration::from_secs(1)).is_empty());
    assert_eq!(planner.voices.len(), 1);
    assert_eq!(planner.voices[0].slot, 1);

    let (start, clock, mut planner) = setup(0.0, true, Some(FollowAction::Next));
    // Force launch_at's checked voice-id allocation to fail after setting the
    // old voice's cutoff. Its retirement must still notify the client.
    planner.serial = u64::MAX;
    assert_eq!(
        advance(&mut planner, &clock, start + Duration::from_millis(250)),
        [TrackId(1)]
    );
    assert!(planner.voices.is_empty());
}

#[test]
fn scene_follow_stop_reports_each_track_once() {
    let (start, mut clock, mut planner) = setup(0.0, true, None);
    let project = Arc::make_mut(&mut planner.project);
    let mut second = crate::scheduler::tests::copy_track(&project.tracks[0], TrackId(2));
    for slot in &mut second.slots {
        slot.clip.as_mut().unwrap().id.0 += 10;
    }
    project.tracks.push(second);
    let follow = &mut project.scenes[0].follow_actions;
    follow.actions.enabled = true;
    follow.actions.action_a = FollowAction::Stop;
    follow.time = BeatTime(480);
    planner
        .set_global_quantisation(GlobalQuantisation::None)
        .unwrap();
    planner.launch_scene(SceneId(1), &mut clock, start).unwrap();
    assert_eq!(
        advance(&mut planner, &clock, start + Duration::from_millis(250)),
        [TrackId(1), TrackId(2)]
    );
    assert!(advance(&mut planner, &clock, start + Duration::from_secs(1)).is_empty());
}

#[test]
fn edits_report_lost_active_tracks_but_not_already_ended_tracks() {
    for looping in [true, false] {
        let (start, clock, mut planner) = setup(1000.0, looping, None);
        let now = start + Duration::from_millis(500);
        let ended = advance(&mut planner, &clock, now);
        assert_eq!(ended.is_empty(), looping);
        let mut project = (*planner.project).clone();
        project.tracks[0].slots[0].clip = None;
        let silent = planner
            .update_project(project.clone(), &clock, now)
            .unwrap();
        assert_eq!(silent, if looping { vec![TrackId(1)] } else { vec![] });
        assert!(
            planner
                .update_project(project, &clock, now)
                .unwrap()
                .is_empty()
        );
    }
}

#[test]
fn quantised_stop_after_clip_end_does_not_notify_twice() {
    let (start, clock, mut planner) = setup(0.0, false, Some(FollowAction::NoAction));
    Arc::make_mut(&mut planner.project).tracks[0].slots[0]
        .clip
        .as_mut()
        .unwrap()
        .follow_actions
        .time = BeatTime(2880);
    assert!(
        !planner
            .request_stop(TrackId(1), &clock, start + Duration::from_millis(100))
            .unwrap()
    );
    assert_eq!(
        advance(&mut planner, &clock, start + Duration::from_millis(500)),
        [TrackId(1)]
    );
    assert!(advance(&mut planner, &clock, start + Duration::from_secs(2)).is_empty());
}
