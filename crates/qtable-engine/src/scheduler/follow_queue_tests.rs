//! The follow destination is chosen at launch and published while its timer runs, as observed in Live 12.1 on 2026-10-11.
use super::follow_tests::{fire, setup, setup_seeded};
use super::*;
use crate::model::*;

/// Edit the project the way the engine does, so voices see the change.
pub(super) fn edit(
    clock: &InternalClock,
    planner: &mut Planner,
    change: impl FnOnce(&mut Project),
) {
    let mut project = (*planner.project).clone();
    change(&mut project);
    planner
        .update_project(project, clock, Instant::now())
        .unwrap();
}

fn queued(planner: &Planner) -> Option<usize> {
    planner.queued_follow_slot(&planner.voices[0])
}

pub(super) fn published(planner: &Planner, clock: &InternalClock) -> PublishedState {
    let now = Instant::now();
    let mut state = PublishedState {
        playing: false,
        tempo: clock.tempo(),
        signature: clock.time_signature(),
        global_quantisation: planner.project.global_quantisation,
        pending_stops: vec![],
        pending_stop_slots: Vec::new(),
        last_triggered_scene: None,
        queued_follow_scene: None,
        beat: BeatTime::ZERO,
        anchor: now,
        tracks: vec![],
        clip_positions: vec![],
        lane_positions: vec![],
        automation: vec![],
        failure: None,
    };
    state.publish(planner, clock, now);
    state
}

#[test]
fn destination_is_known_from_launch_and_publishes_until_the_timer_fires() {
    let (clock, mut p) = setup(FollowAction::Next, 0);
    assert_eq!(queued(&p), Some(1));
    assert_eq!(published(&p, &clock).tracks[0].queued_follow_slot, Some(1));
    fire(&clock, &mut p, 480);
    assert_eq!(p.voices.last().map(|v| v.slot), Some(1));
    // The destination clip has no follow action of its own: nothing is queued.
    assert!(
        published(&p, &clock)
            .tracks
            .iter()
            .all(|t| t.queued_follow_slot.is_none())
    );
}

#[test]
fn random_destination_matches_the_clip_that_launches() {
    for action in [FollowAction::Any, FollowAction::Other] {
        for seed in 0..50 {
            let (clock, mut p) = setup_seeded(action, 1, seed);
            let expected = queued(&p).expect("a three-clip group always has a destination");
            fire(&clock, &mut p, 480);
            assert_eq!(p.voices.last().unwrap().slot, expected);
        }
    }
}

#[test]
fn nothing_to_show_for_stop_no_action_or_global_off() {
    for action in [FollowAction::Stop, FollowAction::NoAction] {
        let (clock, p) = setup(action, 0);
        assert_eq!(published(&p, &clock).tracks[0].queued_follow_slot, None);
    }
    let (clock, mut p) = setup(FollowAction::Next, 0);
    edit(&clock, &mut p, |project| {
        project.follow_actions_enabled = false
    });
    assert_eq!(published(&p, &clock).tracks[0].queued_follow_slot, None);
}

#[test]
fn layout_edit_re_resolves_and_action_edit_draws_again() {
    let (clock, mut p) = setup(FollowAction::Next, 0);
    edit(&clock, &mut p, |project| {
        let actions = &mut project.tracks[0].slots[0]
            .clip
            .as_mut()
            .unwrap()
            .follow_actions
            .actions;
        actions.action_a = FollowAction::Last;
    });
    assert_eq!(queued(&p), Some(2));
    // Emptying slot 1 splits the group, so Last now stays on slot 0.
    edit(&clock, &mut p, |project| {
        project.tracks[0].slots[1].clip = None
    });
    assert_eq!(queued(&p), Some(0));
    fire(&clock, &mut p, 480);
    assert_eq!(p.voices.last().map(|v| v.slot), Some(0));
}
