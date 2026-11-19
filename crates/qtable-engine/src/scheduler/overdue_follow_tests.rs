//! Live toggles/edits must not launch a past follow chain.
use super::*;
use crate::model::*;

const NOW: BeatTime = BeatTime(20_160); // 10.5 seconds at 120 BPM, off the bar grid.

#[test]
fn scheduling_jitter_keeps_clip_and_scene_follow_on_the_original_bar() {
    let bar = BeatTime(3840);
    for scene in [false, true] {
        for lateness in [Duration::from_millis(5), late::LATE_SEND_MARGIN] {
            let (mut p, clock, _) = setup(scene, LaunchQuantisation::None);
            let project = Arc::make_mut(&mut p.project);
            project.tracks[0].slots[1]
                .clip
                .as_mut()
                .unwrap()
                .quantisation = LaunchQuantisation::Value(QuantisationValue::Bars(1));
            if scene {
                project.scenes[0].follow_actions.time = bar;
                p.scene_timer = Some((SceneId(1), bar));
            } else {
                project.tracks[0].slots[0]
                    .clip
                    .as_mut()
                    .unwrap()
                    .follow_actions
                    .time = bar;
            }
            let now = clock.instant_at(bar).unwrap() + lateness;
            consume_due(&mut p, &clock, now);
            assert_eq!(p.voices.iter().find(|v| v.slot == 1).unwrap().launch, bar);
        }
    }
}

fn setup(scene: bool, grid: LaunchQuantisation) -> (Planner, InternalClock, Instant) {
    let start = Instant::now();
    let mut project = tests::project(0.0, true);
    project.follow_actions_enabled = true;
    project.global_quantisation = GlobalQuantisation::None;
    for slot in &mut project.tracks[0].slots {
        let clip = slot.clip.as_mut().unwrap();
        clip.quantisation = grid;
        clip.follow_actions.actions.enabled = !scene;
        clip.follow_actions.actions.action_a = FollowAction::Next;
        clip.follow_actions.linked = false;
        clip.follow_actions.time = BeatTime(960);
    }
    if scene {
        for scene in &mut project.scenes {
            scene.follow_actions.actions.enabled = true;
            scene.follow_actions.actions.action_a = FollowAction::Next;
            scene.follow_actions.time = BeatTime(960);
        }
    }
    let mut p = Planner::new(project, Duration::from_millis(12)).unwrap();
    let mut clock = InternalClock::default();
    if scene {
        p.launch_scene(SceneId(1), &mut clock, start).unwrap();
    } else {
        clock.play(start);
        p.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
    }
    let now = clock.instant_at(NOW).unwrap();
    (p, clock, now)
}

fn consume_due(p: &mut Planner, clock: &InternalClock, now: Instant) {
    for event in p.plan(clock, now) {
        if event.at <= now {
            p.consume_action(&event).unwrap();
        }
    }
}

fn assert_spent(p: &mut Planner, clock: &InternalClock, now: Instant, scene: bool) {
    consume_due(p, clock, now);
    assert!(p.voices.iter().all(|v| v.slot == 0 && v.cutoff.is_none()));
    if scene {
        assert_eq!(p.last_triggered_scene, Some(SceneId(1)));
        assert_eq!(p.scene_timer, None);
    } else {
        assert!(p.voices.iter().all(|v| v.follow_done));
    }
    // Advancing playback must not replay the missed timer or a past chain.
    let later = now + Duration::from_secs(1);
    assert!(p.plan(clock, later).iter().all(|e| !matches!(
        e.action,
        PlannedAction::Follow { .. } | PlannedAction::SceneFollow { .. }
    )));
}

fn assert_relaunch_arms_follow(
    p: &mut Planner,
    clock: &mut InternalClock,
    now: Instant,
    scene: bool,
) {
    if scene {
        p.launch_scene(SceneId(1), clock, now).unwrap();
    } else {
        p.launch_at(TrackId(1), 0, NOW).unwrap();
    }
    let beat = if scene {
        p.scene_timer.unwrap().1
    } else {
        p.follow_beat(p.voices.last().unwrap()).unwrap()
    };
    consume_due(p, clock, clock.instant_at(beat).unwrap());
    assert!(p.voices.iter().any(|v| v.slot == 1));
    if scene {
        assert_eq!(p.last_triggered_scene, Some(SceneId(2)));
    }
}

#[test]
fn global_follow_reenabled_after_timer_is_spent_for_clips_and_scenes() {
    // Live 12 check 5 (2026-10-11): re-enabling requires a relaunch.
    for scene in [false, true] {
        for grid in [
            LaunchQuantisation::None,
            LaunchQuantisation::Global,
            LaunchQuantisation::Value(QuantisationValue::Bars(1)),
        ] {
            let (mut p, mut clock, now) = setup(scene, grid);
            Arc::make_mut(&mut p.project).follow_actions_enabled = false;
            consume_due(&mut p, &clock, now);
            // Same model mutation as SchedulerCommand::SetFollowActionsEnabled.
            Arc::make_mut(&mut p.project).follow_actions_enabled = true;
            assert_spent(&mut p, &clock, now, scene);
            assert_relaunch_arms_follow(&mut p, &mut clock, now, scene);
        }
    }
}

#[test]
fn enabling_global_follow_via_project_update_also_spends_timer() {
    // Live 12 check 5 (2026-10-11): overdue timers are spent until relaunch.
    for scene in [false, true] {
        let (mut p, clock, now) = setup(scene, LaunchQuantisation::None);
        Arc::make_mut(&mut p.project).follow_actions_enabled = false;
        consume_due(&mut p, &clock, now);
        let mut project = (*p.project).clone();
        project.follow_actions_enabled = true;
        p.update_project(project, &clock, now).unwrap();
        assert_spent(&mut p, &clock, now, scene);
    }
}

#[test]
fn local_follow_reenabled_after_timer_is_spent() {
    // Live 12 check 5 (2026-10-11): overdue timers are spent until relaunch.
    for scene in [false, true] {
        let (mut p, clock, now) = setup(scene, LaunchQuantisation::None);
        let mut project = (*p.project).clone();
        if scene {
            project.scenes[0].follow_actions.actions.enabled = false;
        } else {
            project.tracks[0].slots[0]
                .clip
                .as_mut()
                .unwrap()
                .follow_actions
                .actions
                .enabled = false;
        }
        p.update_project(project, &clock, now).unwrap();
        consume_due(&mut p, &clock, now);
        let mut project = (*p.project).clone();
        if scene {
            project.scenes[0].follow_actions.actions.enabled = true;
        } else {
            project.tracks[0].slots[0]
                .clip
                .as_mut()
                .unwrap()
                .follow_actions
                .actions
                .enabled = true;
        }
        p.update_project(project, &clock, now).unwrap();
        assert_spent(&mut p, &clock, now, scene);
    }
}

#[test]
fn ended_nonlooping_clip_with_disabled_follow_is_spent() {
    // Live 12 check 5 (2026-10-11): overdue timers are spent until relaunch.
    let (mut p, clock, now) = setup(false, LaunchQuantisation::None);
    let mut project = (*p.project).clone();
    project.follow_actions_enabled = false;
    project.tracks[0].slots[0].clip.as_mut().unwrap().looping = false;
    // Make it non-looping at launch; a later Loop-off now finishes that pass.
    p.update_project(project, &clock, clock.instant_at(BeatTime::ZERO).unwrap())
        .unwrap();
    consume_due(&mut p, &clock, now);
    assert!(p.voices[0].silent);
    Arc::make_mut(&mut p.project).follow_actions_enabled = true;
    assert_spent(&mut p, &clock, now, false);
}

#[test]
fn shortened_clip_time_length_and_multiplier_spend_timer() {
    // Inferred from Live 12 check 5 (2026-10-11), which observed the toggle case.
    for edit in 0..3 {
        let (mut p, clock, now) = setup(false, LaunchQuantisation::None);
        let clip = Arc::make_mut(&mut p.project).tracks[0].slots[0]
            .clip
            .as_mut()
            .unwrap();
        match edit {
            0 => clip.follow_actions.time = BeatTime(40_320),
            1 => {
                clip.follow_actions.linked = true;
                clip.end = BeatTime(40_320);
                clip.loop_end = BeatTime(40_320);
            }
            _ => {
                clip.follow_actions.linked = true;
                clip.follow_actions.multiplier = FollowMultiplier::new(100, 1).unwrap();
            }
        }
        consume_due(&mut p, &clock, now);
        assert_eq!(p.voices[0].slot, 0);
        let mut project = (*p.project).clone();
        let clip = project.tracks[0].slots[0].clip.as_mut().unwrap();
        match edit {
            0 => clip.follow_actions.time = BeatTime(960),
            1 => {
                clip.end = BeatTime(960);
                clip.loop_end = BeatTime(960);
            }
            _ => clip.follow_actions.multiplier = FollowMultiplier::new(1, 1).unwrap(),
        }
        p.update_project(project, &clock, now).unwrap();
        assert_spent(&mut p, &clock, now, false);
    }
}

#[test]
fn shortened_scene_time_uses_original_launch_boundary_and_spends_timer() {
    // Inferred from Live 12 check 5 (2026-10-11), which observed the toggle case.
    let (mut p, clock, now) = setup(true, LaunchQuantisation::None);
    let mut project = (*p.project).clone();
    project.scenes[0].follow_actions.time = BeatTime(40_320);
    p.update_project(project, &clock, now).unwrap();
    assert_eq!(p.scene_timer, Some((SceneId(1), BeatTime(40_320))));
    consume_due(&mut p, &clock, now);
    let mut project = (*p.project).clone();
    project.scenes[0].follow_actions.time = BeatTime(960);
    p.update_project(project, &clock, now).unwrap();
    assert_spent(&mut p, &clock, now, true);
}

#[test]
fn overdue_follow_after_tempo_reanchor_is_still_spent() {
    // Live 12 check 5 (2026-10-11): overdue timers are spent until relaunch.
    for scene in [false, true] {
        let (mut p, mut clock, now) = setup(scene, LaunchQuantisation::None);
        Arc::make_mut(&mut p.project).follow_actions_enabled = false;
        consume_due(&mut p, &clock, now);
        assert!(p.set_tempo(&mut clock, Tempo::new(90.0).unwrap(), now));
        Arc::make_mut(&mut p.project).follow_actions_enabled = true;
        assert_spent(&mut p, &clock, now, scene);
    }
}
