use super::*;
use crate::model::*;
fn setup(action: FollowAction) -> (Planner, InternalClock, Instant) {
    let start = Instant::now();
    let mut project = tests::project(0.0, true);
    project.global_quantisation = GlobalQuantisation::None;
    project.follow_actions_enabled = true;
    project.random_seed = 42;
    for track in &mut project.tracks {
        for slot in &mut track.slots {
            slot.clip.as_mut().unwrap().quantisation = LaunchQuantisation::None;
        }
    }
    project.scenes[0].follow_actions.actions.enabled = true;
    project.scenes[0].follow_actions.actions.action_a = action;
    project.scenes[0].follow_actions.time = BeatTime(960);
    let mut planner = Planner::new(project, Duration::from_millis(600)).unwrap();
    let mut clock = InternalClock::default();
    planner.launch_scene(SceneId(1), &mut clock, start).unwrap();
    (planner, clock, start)
}
fn fire(p: &mut Planner, clock: &InternalClock, start: Instant) {
    let event = p.scene_event(clock, start).unwrap();
    p.consume(&event);
}
#[test]
fn all_actions_and_wrap() {
    for (action, destination) in [
        (FollowAction::NoAction, 1),
        (FollowAction::PlayAgain, 1),
        (FollowAction::Previous, 2),
        (FollowAction::Next, 2),
        (FollowAction::First, 1),
        (FollowAction::Last, 2),
        (FollowAction::Other, 2),
        (FollowAction::Jump(SceneId(2)), 2),
    ] {
        let (mut p, c, s) = setup(action);
        fire(&mut p, &c, s);
        assert_eq!(p.last_triggered_scene, Some(SceneId(destination)));
    }
    let (mut p, c, s) = setup(FollowAction::Stop);
    fire(&mut p, &c, s);
    assert!(p.voices.is_empty());
    let (mut p, c, s) = setup(FollowAction::Next);
    Arc::make_mut(&mut p.project).scenes[1].follow_actions =
        p.project.scenes[0].follow_actions.clone();
    fire(&mut p, &c, s);
    fire(&mut p, &c, s);
    assert_eq!(p.last_triggered_scene, Some(SceneId(1)));
}
#[test]
fn jump_to_clipless_scene_launches_then_terminates() {
    for stop_button in [false, true] {
        let (mut p, c, s) = setup(FollowAction::Jump(SceneId(2)));
        Arc::make_mut(&mut p.project).tracks[0].slots[1].clip = None;
        Arc::make_mut(&mut p.project).tracks[0].slots[1].has_stop_button = stop_button;
        Arc::make_mut(&mut p.project).scenes[1].follow_actions =
            p.project.scenes[0].follow_actions.clone();
        fire(&mut p, &c, s);
        assert_eq!(p.last_triggered_scene, Some(SceneId(2)));
        assert_eq!(p.voices.is_empty(), stop_button);
        assert!(p.scene_timer.is_none());
    }
}

#[test]
fn scene_groups_bound_actions_but_jump_crosses_gaps() {
    for seed in 0..64 {
        for (action, expected) in [
            (FollowAction::Previous, Some(2)),
            (FollowAction::Next, Some(2)),
            (FollowAction::First, Some(1)),
            (FollowAction::Last, Some(2)),
            (FollowAction::Any, None),
            (FollowAction::Other, Some(2)),
            (FollowAction::Jump(SceneId(4)), Some(4)),
        ] {
            let (mut p, mut c, s) = setup(FollowAction::NoAction);
            Arc::make_mut(&mut p.project).scenes[0]
                .follow_actions
                .actions
                .action_a = action;
            let template = p.project.scenes[1].clone();
            let slot = p.project.tracks[0].slots[1].clone();
            for id in [3, 4] {
                let mut scene = template.clone();
                scene.id = SceneId(id);
                Arc::make_mut(&mut p.project).scenes.push(scene);
                Arc::make_mut(&mut p.project).tracks[0]
                    .slots
                    .push(slot.clone());
            }
            Arc::make_mut(&mut p.project).tracks[0].slots[2] = Slot {
                clip: None,
                has_stop_button: false,
            };
            p.rng = seed;
            p.launch_scene(SceneId(1), &mut c, s).unwrap();
            let mut twin = p.clone();
            fire(&mut p, &c, s);
            fire(&mut twin, &c, s);
            assert_eq!(p.last_triggered_scene, twin.last_triggered_scene);
            if let Some(id) = expected {
                assert_eq!(p.last_triggered_scene, Some(SceneId(id)));
            } else {
                assert!(matches!(p.last_triggered_scene, Some(SceneId(1 | 2))));
            }
        }
    }
    // A stop button makes a clipless row part of the range, not a separator.
    let (mut p, c, s) = setup(FollowAction::Next);
    Arc::make_mut(&mut p.project).tracks[0].slots[1].clip = None;
    fire(&mut p, &c, s);
    assert_eq!(p.last_triggered_scene, Some(SceneId(2)));
    assert!(p.scene_timer.is_none());
}
#[test]
fn enable_weights_rng_and_singleton() {
    let (mut p, c, s) = setup(FollowAction::Any);
    let mut other = p.clone();
    fire(&mut p, &c, s);
    fire(&mut other, &c, s);
    assert_eq!(p.last_triggered_scene, other.last_triggered_scene);
    assert_eq!(p.rng, other.rng);
    let (mut p, c, s) = setup(FollowAction::Next);
    Arc::make_mut(&mut p.project).follow_actions_enabled = false;
    assert!(p.scene_event(&c, s).is_none());
    Arc::make_mut(&mut p.project).follow_actions_enabled = true;
    fire(&mut p, &c, s);
    assert_eq!(p.last_triggered_scene, Some(SceneId(2)));
    let (mut p, c, s) = setup(FollowAction::Next);
    super::follow_queue_tests::edit(&c, &mut p, |project| {
        project.scenes[0].follow_actions.actions.chance_a = 0;
    });
    fire(&mut p, &c, s);
    assert_eq!(p.last_triggered_scene, Some(SceneId(1)));
    assert!(p.scene_timer.is_none());
    for action in [FollowAction::Any, FollowAction::Other] {
        let (mut p, c, s) = setup(action);
        Arc::make_mut(&mut p.project).scenes.truncate(1);
        Arc::make_mut(&mut p.project).tracks[0].slots.truncate(1);
        fire(&mut p, &c, s);
        assert_eq!(p.voices.last().unwrap().launch, BeatTime(960));
    }
}
#[test]
fn clip_and_scene_timers_coexist_scene_wins_tie() {
    let (mut p, c, s) = setup(FollowAction::Next);
    let follow = &mut Arc::make_mut(&mut p.project).tracks[0].slots[0]
        .clip
        .as_mut()
        .unwrap()
        .follow_actions;
    follow.actions.enabled = true;
    follow.actions.action_a = FollowAction::PlayAgain;
    follow.linked = false;
    follow.time = BeatTime(960);
    p.voices[0].follow_done = false;
    let events = p.plan(&c, s);
    assert!(
        events
            .iter()
            .any(|e| matches!(e.action, PlannedAction::Follow { .. }))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e.action, PlannedAction::SceneFollow { .. }))
    );
    for e in events {
        p.consume(&e);
    }
    assert_eq!(p.last_triggered_scene, Some(SceneId(2)));
    assert!(p.voices.iter().any(|v| v.slot == 1));
}

#[test]
fn quantisation_timer_and_destination_boundaries() {
    let (mut p, mut c, s) = setup(FollowAction::Next);
    p.stop_all();
    Arc::make_mut(&mut p.project).tracks[0].slots[0]
        .clip
        .as_mut()
        .unwrap()
        .quantisation = LaunchQuantisation::Value(QuantisationValue::Note(NoteValue::Quarter));
    p.launch_scene(SceneId(1), &mut c, s).unwrap();
    assert_eq!(p.scene_timer, Some((SceneId(1), BeatTime(1920))));
    Arc::make_mut(&mut p.project).tracks[0].slots[1]
        .clip
        .as_mut()
        .unwrap()
        .quantisation = LaunchQuantisation::Global;
    Arc::make_mut(&mut p.project).global_quantisation =
        GlobalQuantisation::Value(QuantisationValue::Bars(1));
    fire(&mut p, &c, s);
    // Global-quantised destinations: bypassed (launch at the 1920 timer) or the next bar.
    let expected = if SCENE_FOLLOW_BYPASS_GLOBAL {
        1920
    } else {
        3840
    };
    assert!(
        p.voices
            .iter()
            .any(|v| v.slot == 1 && v.launch == BeatTime(expected))
    );
}

#[test]
fn scene_next_wrap_is_inferred_from_live12_check_2_clip_observation() {
    // Live 12 (exact version unrecorded), 2026-10-11: check 2 observes
    // clip Next wrapping. This pins the retained scene inference, not a Live result.
    let (mut p, mut clock, start) = setup(FollowAction::Next); // fixture seed 42
    Arc::make_mut(&mut p.project).scenes[1].follow_actions =
        p.project.scenes[0].follow_actions.clone();
    p.launch_scene(SceneId(2), &mut clock, start).unwrap();
    let beat = p.scene_timer.unwrap().1;
    fire(&mut p, &clock, clock.instant_at(beat).unwrap());
    assert_eq!(p.last_triggered_scene, Some(SceneId(1)));
    assert!(p.voices.iter().any(|v| v.slot == 0 && v.launch == beat));
}

#[test]
fn queued_scene_matches_random_launch_and_does_not_redraw_at_timer() {
    use super::follow_queue_tests::published;
    for action in [
        FollowAction::Any,
        FollowAction::Other,
        FollowAction::PlayAgain,
    ] {
        for seed in 0..32 {
            let (mut p, mut c, s) = setup(action);
            p.rng = seed;
            // Give both A/B weight, including an outcome different from Any/Other.
            Arc::make_mut(&mut p.project).scenes[0]
                .follow_actions
                .actions
                .action_b = FollowAction::Last;
            Arc::make_mut(&mut p.project).scenes[0]
                .follow_actions
                .actions
                .chance_b = 1;
            p.launch_scene(SceneId(1), &mut c, s).unwrap();
            let expected = published(&p, &c).queued_follow_scene.unwrap();
            assert_eq!(published(&p, &c).queued_follow_scene, Some(expected));
            // Reading the snapshot or unrelated random draws cannot change the choice.
            p.next_u64();
            fire(&mut p, &c, s);
            assert_eq!(p.last_triggered_scene, Some(expected));
            if expected == SceneId(2) {
                assert_eq!(published(&p, &c).queued_follow_scene, None);
            }
        }
    }
}

#[test]
fn queued_scene_is_hidden_when_disabled_and_clears_on_spent_timer_or_stop() {
    use super::follow_queue_tests::{edit, published};
    for action in [FollowAction::NoAction, FollowAction::Stop] {
        let (p, c, _) = setup(action);
        assert_eq!(published(&p, &c).queued_follow_scene, None);
    }
    let (mut p, c, s) = setup(FollowAction::Next);
    assert_eq!(published(&p, &c).queued_follow_scene, Some(SceneId(2)));
    edit(&c, &mut p, |project| project.follow_actions_enabled = false);
    assert_eq!(published(&p, &c).queued_follow_scene, None);
    edit(&c, &mut p, |project| project.follow_actions_enabled = true);
    assert_eq!(published(&p, &c).queued_follow_scene, Some(SceneId(2)));
    fire(&mut p, &c, s + Duration::from_secs(2));
    assert_eq!(p.last_triggered_scene, Some(SceneId(1)));
    assert_eq!(published(&p, &c).queued_follow_scene, None);
    assert!(p.scene_queued.is_none());

    for quantised in [false, true] {
        let (mut p, c, s) = setup(FollowAction::Next);
        if quantised {
            Arc::make_mut(&mut p.project).global_quantisation =
                GlobalQuantisation::Value(QuantisationValue::Bars(1));
            p.request_stop_all(&c, s).unwrap();
        } else {
            p.stop_all();
        }
        assert_eq!(published(&p, &c).queued_follow_scene, None);
        assert!(p.scene_queued.is_none());
    }
}

#[test]
fn queued_scene_tracks_action_edits_layout_changes_and_manual_relaunch() {
    use super::follow_queue_tests::{edit, published};
    let (mut p, mut c, s) = setup(FollowAction::Next);
    edit(&c, &mut p, |project| {
        project.scenes[0].follow_actions.actions.action_a = FollowAction::Last;
    });
    assert_eq!(published(&p, &c).queued_follow_scene, Some(SceneId(2)));
    // A gap changes the range; resolve the retained choice against the new layout.
    edit(&c, &mut p, |project| {
        project.tracks[0].slots[1] = Slot {
            clip: None,
            has_stop_button: false,
        };
    });
    assert_eq!(published(&p, &c).queued_follow_scene, Some(SceneId(1)));
    edit(&c, &mut p, |project| {
        project.scenes[0].follow_actions.actions.enabled = false;
    });
    assert_eq!(published(&p, &c).queued_follow_scene, None);
    edit(&c, &mut p, |project| {
        project.scenes[0].follow_actions.actions.enabled = true;
    });
    assert_eq!(published(&p, &c).queued_follow_scene, Some(SceneId(1)));
    p.launch_scene(SceneId(2), &mut c, s).unwrap();
    assert_eq!(published(&p, &c).queued_follow_scene, None);
    p.launch_scene(SceneId(1), &mut c, s).unwrap();
    assert_eq!(published(&p, &c).queued_follow_scene, Some(SceneId(1)));
    edit(&c, &mut p, |project| {
        project.scenes.remove(0);
        project.tracks[0].slots.remove(0);
    });
    assert_eq!(published(&p, &c).queued_follow_scene, None);
    assert!(p.scene_queued.is_none());
}

/// One track with two clips and a one-bar global grid, the transport already playing.
fn on_the_grid() -> (Planner, InternalClock, Instant) {
    let start = Instant::now();
    let mut project = tests::project(0.0, true);
    project.global_quantisation = GlobalQuantisation::Value(QuantisationValue::Bars(1));
    project.follow_actions_enabled = true;
    for slot in &mut project.tracks[0].slots {
        slot.clip.as_mut().unwrap().quantisation = LaunchQuantisation::Global;
    }
    project.scenes[1].follow_actions.actions.enabled = true;
    project.scenes[1].follow_actions.actions.action_a = FollowAction::Next;
    let planner = Planner::new(project, Duration::from_millis(600)).unwrap();
    let mut clock = InternalClock::default();
    clock.play(start);
    (planner, clock, start)
}

fn ids(planner: &Planner) -> Vec<(usize, Option<BeatTime>)> {
    planner.voices.iter().map(|v| (v.slot, v.cutoff)).collect()
}

#[test]
fn cancel_scene_launch_drops_waiting_clips_and_the_follow_chain() {
    let (mut planner, mut clock, start) = on_the_grid();
    let later = start + Duration::from_millis(100);
    planner.launch_scene(SceneId(2), &mut clock, later).unwrap();
    assert_eq!(planner.voices.len(), 1);
    assert!(planner.voices[0].launch > clock.beat_at(later).unwrap());
    assert!(planner.queued_follow_scene().is_some());
    planner.cancel_scene_launch(&clock, later);
    // The track was stopped, so it stays stopped; the blink has nothing left to wait for.
    assert!(planner.voices.is_empty());
    assert!(clock.is_playing());
    assert_eq!(planner.last_triggered_scene, None);
    assert!(planner.scene_event(&clock, later).is_none());
    assert_eq!(planner.queued_follow_scene(), None);
}

#[test]
fn cancel_scene_launch_leaves_a_playing_clip_playing() {
    let (mut planner, mut clock, start) = on_the_grid();
    planner.launch_scene(SceneId(1), &mut clock, start).unwrap();
    // 120 bpm: the first bar line passes at 2 s, so the first scene's clip is playing
    // by 2.1 s and the second scene waits for the next one.
    let later = start + Duration::from_millis(2100);
    let first = planner.voices[0].id;
    planner.launch_scene(SceneId(2), &mut clock, later).unwrap();
    assert_eq!(planner.voices.len(), 2);
    assert!(
        planner
            .voices
            .iter()
            .any(|v| v.id == first && v.cutoff.is_some())
    );
    planner.cancel_scene_launch(&clock, later);
    assert_eq!(ids(&planner), vec![(0, None)]);
    assert_eq!(planner.voices[0].id, first);
    assert!(planner.voices[0].cutoff_deadline.is_none());
}

#[test]
fn cancel_scene_launch_keeps_clips_that_already_started_and_manual_launches() {
    let (mut planner, mut clock, start) = on_the_grid();
    planner.launch_scene(SceneId(2), &mut clock, start).unwrap();
    // A clip launched by hand after the scene is not the scene's to cancel.
    planner.launch(TrackId(1), 0, &mut clock, start).unwrap();
    planner.cancel_scene_launch(&clock, start);
    assert_eq!(planner.voices.len(), 1);
    assert_eq!(planner.voices[0].slot, 0);
    // Once the quantize point has passed the clip is playing and Cancel leaves it be.
    let (mut planner, mut clock, start) = on_the_grid();
    planner.launch_scene(SceneId(1), &mut clock, start).unwrap();
    let after = start + Duration::from_millis(2100);
    planner.cancel_scene_launch(&clock, after);
    assert_eq!(planner.voices.len(), 1);
    assert_eq!(planner.last_triggered_scene, None);
}
