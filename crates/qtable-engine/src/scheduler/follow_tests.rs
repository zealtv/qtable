use super::*;
use crate::model::*;
pub(super) fn setup(action: FollowAction, source: usize) -> (InternalClock, Planner) {
    setup_seeded(action, source, 42)
}
/// The follow choice is drawn at launch, so the seed must be set before it.
pub(super) fn setup_seeded(
    action: FollowAction,
    source: usize,
    seed: u64,
) -> (InternalClock, Planner) {
    let mut project = super::tests::project(0.0, true);
    project.follow_actions_enabled = true;
    project.random_seed = seed;
    project.scenes = (0..6)
        .map(|i| Scene {
            id: SceneId(100 + i),
            name: i.to_string(),
            follow_actions: Default::default(),
        })
        .collect();
    let template = project.tracks[0].slots[0].clip.clone().unwrap();
    project.tracks[0].slots = (0..6)
        .map(|i| {
            let mut slot = Slot::default();
            if i != 3 {
                let mut clip = template.clone();
                clip.id = ClipId(i + 1);
                clip.events.clear();
                clip.quantisation = LaunchQuantisation::Global;
                slot.clip = Some(clip);
            }
            slot
        })
        .collect();
    let clip = project.tracks[0].slots[source].clip.as_mut().unwrap();
    clip.follow_actions.actions.enabled = true;
    clip.follow_actions.actions.action_a = action;
    clip.follow_actions.linked = false;
    clip.follow_actions.time = BeatTime(480);
    let mut clock = InternalClock::new(project.tempo, project.time_signature);
    clock.play(Instant::now());
    let mut planner = Planner::new(project, Duration::ZERO).unwrap();
    planner
        .launch_at(TrackId(1), source, BeatTime::ZERO)
        .unwrap();
    (clock, planner)
}
pub(super) fn fire(clock: &InternalClock, planner: &mut Planner, beat: u64) {
    let now = clock.instant_at(BeatTime(beat)).unwrap();
    for event in planner.plan(clock, now) {
        planner.consume(&event);
    }
}
#[test]
fn every_deterministic_action_and_gap_wrap() {
    for (action, source, expected) in [
        (FollowAction::NoAction, 1, Some(1)),
        (FollowAction::Stop, 1, None),
        (FollowAction::PlayAgain, 1, Some(1)),
        (FollowAction::Previous, 0, Some(2)),
        (FollowAction::Next, 2, Some(0)),
        (FollowAction::First, 2, Some(0)),
        (FollowAction::Last, 0, Some(2)),
        (FollowAction::Jump(SceneId(104)), 0, Some(4)),
        (FollowAction::Next, 5, Some(4)),
        (FollowAction::Previous, 4, Some(5)),
    ] {
        let (clock, mut p) = setup(action, source);
        fire(&clock, &mut p, 480);
        assert_eq!(p.voices.last().map(|v| v.slot), expected);
        if expected == Some(source) {
            assert_eq!(
                p.voices[0].launch,
                if action == FollowAction::NoAction {
                    BeatTime::ZERO
                } else {
                    BeatTime(480)
                }
            );
        }
    }
}
#[test]
fn random_actions_are_seeded_and_never_cross_gaps() {
    for action in [FollowAction::Any, FollowAction::Other] {
        let mut sequence = Vec::new();
        for seed in 0..100 {
            let (clock, mut p) = setup_seeded(action, 1, seed);
            let mut twin = p.clone();
            fire(&clock, &mut p, 480);
            fire(&clock, &mut twin, 480);
            let slot = p.voices.last().unwrap().slot;
            assert!(slot < 3);
            if action == FollowAction::Other {
                assert_ne!(slot, 1);
            }
            assert_eq!(slot, twin.voices.last().unwrap().slot);
            sequence.push(slot);
        }
        assert!(sequence.contains(&0) && sequence.contains(&2));
        if action == FollowAction::Any {
            assert!(sequence.contains(&1));
        }
    }
    for action in [FollowAction::Any, FollowAction::Other] {
        let (clock, mut p) = setup(action, 4);
        Arc::make_mut(&mut p.project).tracks[0].slots[5].clip = None;
        fire(&clock, &mut p, 480);
        assert_eq!(p.voices.last().unwrap().launch, BeatTime(480));
    }
}
#[test]
fn linked_multiplier_counts_loops_and_non_looping_fires_at_clip_end() {
    let (_, mut p) = setup(FollowAction::Next, 0);
    let length = p.project.tracks[0].slots[0]
        .clip
        .as_ref()
        .unwrap()
        .length()
        .0;
    // Loop off ignores the multiplier: Live shows "Clip End" instead (2026-10-12).
    for (n, d, looping) in [(1, 2, true), (3, 1, true), (3, 1, false)] {
        let clip = Arc::make_mut(&mut p.project).tracks[0].slots[0]
            .clip
            .as_mut()
            .unwrap();
        clip.follow_actions.linked = true;
        clip.follow_actions.multiplier = FollowMultiplier::new(n, d).unwrap();
        clip.looping = looping;
        let launch = p.voices[0].launch.0;
        assert_eq!(
            p.follow_beat(&p.voices[0]),
            Some(BeatTime(if looping {
                launch + length * u64::from(n) / u64::from(d)
            } else {
                launch + length
            }))
        );
    }
}

#[test]
fn timer_link_multiplier_enable_chance_and_empty_jump_defaults() {
    let (clock, mut p) = setup(FollowAction::Next, 0);
    Arc::make_mut(&mut p.project).tracks[0].slots[0]
        .clip
        .as_mut()
        .unwrap()
        .follow_actions
        .linked = true;
    Arc::make_mut(&mut p.project).tracks[0].slots[0]
        .clip
        .as_mut()
        .unwrap()
        .follow_actions
        .multiplier = FollowMultiplier::new(2, 1).unwrap();
    assert_eq!(p.follow_beat(&p.voices[0]), Some(BeatTime(1920)));
    fire(&clock, &mut p, 480);
    assert_eq!(p.voices[0].slot, 0);
    fire(&clock, &mut p, 1920);
    assert_eq!(p.voices.last().unwrap().slot, 1);
    for global in [false, true] {
        let (clock, mut p) = setup(FollowAction::Next, 0);
        if global {
            Arc::make_mut(&mut p.project).follow_actions_enabled = false;
        } else {
            Arc::make_mut(&mut p.project).tracks[0].slots[0]
                .clip
                .as_mut()
                .unwrap()
                .follow_actions
                .actions
                .enabled = false;
        }
        fire(&clock, &mut p, 480);
        assert_eq!(p.voices[0].slot, 0);
    }
    for (a, b, expected) in [(0, 0, Some(0)), (0, 1, None), (1, 0, Some(1))] {
        let (clock, mut p) = setup(FollowAction::Next, 0);
        super::follow_queue_tests::edit(&clock, &mut p, |project| {
            let actions = &mut project.tracks[0].slots[0]
                .clip
                .as_mut()
                .unwrap()
                .follow_actions
                .actions;
            actions.chance_a = a;
            actions.chance_b = b;
            actions.action_b = FollowAction::Stop;
        });
        fire(&clock, &mut p, 480);
        assert_eq!(p.voices.last().map(|v| v.slot), expected);
    }
    for button in [false, true] {
        let (clock, mut p) = setup(FollowAction::Jump(SceneId(103)), 0);
        Arc::make_mut(&mut p.project).tracks[0].slots[3].has_stop_button = button;
        fire(&clock, &mut p, 480);
        assert_eq!(p.voices.is_empty(), button);
    }
}
#[test]
fn explicit_destination_grid_and_follow_after_nonloop_end() {
    let (clock, mut p) = setup(FollowAction::Next, 0);
    Arc::make_mut(&mut p.project).tracks[0].slots[1]
        .clip
        .as_mut()
        .unwrap()
        .quantisation = LaunchQuantisation::Value(QuantisationValue::Bars(1));
    fire(&clock, &mut p, 480);
    assert_eq!(p.voices.last().unwrap().launch, BeatTime(3840));
    let (clock, mut p) = setup(FollowAction::Next, 0);
    let clip = Arc::make_mut(&mut p.project).tracks[0].slots[0]
        .clip
        .as_mut()
        .unwrap();
    clip.looping = false;
    clip.follow_actions.time = BeatTime(1920);
    fire(&clock, &mut p, 960);
    assert!(p.voices[0].silent);
    fire(&clock, &mut p, 1920);
    assert_eq!(p.voices.last().unwrap().slot, 1);
}
#[test]
fn timer_begins_at_actual_quantised_launch_and_no_action_fires_once() {
    let (clock, mut p) = setup(FollowAction::NoAction, 0);
    p.voices.clear();
    p.launch_at(TrackId(1), 0, BeatTime(3840)).unwrap();
    assert_eq!(p.follow_beat(&p.voices[0]), Some(BeatTime(4320)));
    fire(&clock, &mut p, 4320);
    assert_eq!(p.follow_beat(&p.voices[0]), None);
}

#[test]
fn documented_stop_overrides_loop_and_unlinked_time_starts_at_launch() {
    for looping in [false, true] {
        let (clock, mut p) = setup(FollowAction::Stop, 0);
        Arc::make_mut(&mut p.project).tracks[0].slots[0]
            .clip
            .as_mut()
            .unwrap()
            .looping = looping;
        p.voices.clear();
        p.launch_at(TrackId(1), 0, BeatTime(960)).unwrap();
        assert_eq!(p.follow_beat(&p.voices[0]), Some(BeatTime(1440)));
        fire(&clock, &mut p, 1440);
        assert!(p.voices.is_empty());
    }
}

#[test]
fn live12_check_1_previous_at_group_top_wraps_to_bottom() {
    // Live 12 (exact version unrecorded), 2026-10-11: check 1, A fires C.
    // Fixture seed 42; the empty slot after C must keep the next group out.
    let (clock, mut p) = setup(FollowAction::Previous, 0);
    let mut twin = p.clone();
    fire(&clock, &mut p, 480);
    fire(&clock, &mut twin, 480);
    let voice = p.voices.last().unwrap();
    assert_eq!((voice.slot, voice.launch), (2, BeatTime(480)));
    assert_eq!(voice.slot, twin.voices.last().unwrap().slot);
    assert_eq!(p.rng, twin.rng);
}

#[test]
fn live12_check_2_clip_next_at_group_bottom_wraps_to_top() {
    // Live 12 (exact version unrecorded), 2026-10-11: check 2 describes
    // clip Next, not an unambiguous observation of a scene Follow Action.
    let (clock, mut p) = setup(FollowAction::Next, 2); // fixture seed 42
    fire(&clock, &mut p, 480);
    let voice = p.voices.last().unwrap();
    assert_eq!((voice.slot, voice.launch), (0, BeatTime(480)));
}
