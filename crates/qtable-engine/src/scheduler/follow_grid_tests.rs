//! Follow-originated launches round to the boundary at or
//! after the timer, so on-grid timers do not lose a whole quantum.
use super::*;
use crate::model::*;

const BAR: u64 = 3840;

fn clip_setup(quantisation: LaunchQuantisation) -> (InternalClock, Planner) {
    let mut project = super::tests::project(0.0, true);
    project.follow_actions_enabled = true;
    project.random_seed = 42;
    for slot in &mut project.tracks[0].slots {
        slot.clip.as_mut().unwrap().quantisation = quantisation;
    }
    let clip = project.tracks[0].slots[0].clip.as_mut().unwrap();
    clip.follow_actions.actions.enabled = true;
    clip.follow_actions.actions.action_a = FollowAction::Next;
    clip.follow_actions.linked = false;
    clip.follow_actions.time = BeatTime(BAR);
    let mut clock = InternalClock::new(project.tempo, project.time_signature);
    clock.play(Instant::now());
    let mut planner = Planner::new(project, Duration::ZERO).unwrap();
    planner.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
    (clock, planner)
}

fn fire_clip(clock: &InternalClock, p: &mut Planner, beat: u64) {
    let now = clock.instant_at(BeatTime(beat)).unwrap();
    for event in p.plan(clock, now) {
        p.consume(&event);
    }
}

fn scene_setup(quantisation: LaunchQuantisation) -> (Planner, InternalClock, Instant) {
    let start = Instant::now();
    let mut project = super::tests::project(0.0, true);
    project.follow_actions_enabled = true;
    project.random_seed = 42;
    for slot in &mut project.tracks[0].slots {
        slot.clip.as_mut().unwrap().quantisation = quantisation;
    }
    project.scenes[0].follow_actions.actions.enabled = true;
    project.scenes[0].follow_actions.actions.action_a = FollowAction::Next;
    project.scenes[0].follow_actions.time = BeatTime(BAR);
    let mut planner = Planner::new(project, Duration::from_millis(600)).unwrap();
    let mut clock = InternalClock::default();
    clock.play(start);
    planner.launch_scene(SceneId(1), &mut clock, start).unwrap();
    // A launch at beat 0 on a playing clock is quantised; settle on the bar-0 grid.
    planner
        .voices
        .iter_mut()
        .for_each(|v| v.launch = BeatTime::ZERO);
    planner.scene_timer = Some((SceneId(1), BeatTime(BAR)));
    (planner, clock, start)
}

fn fire_scene(p: &mut Planner, clock: &InternalClock, start: Instant) {
    let event = p.scene_event(clock, start).unwrap();
    p.consume(&event);
}

#[test]
fn boundary_at_or_after_keeps_exact_ticks_and_rounds_up_others() {
    let sig = TimeSignature::default();
    let bar = GlobalQuantisation::Value(QuantisationValue::Bars(1));
    assert_eq!(bar.boundary_at_or_after(BeatTime(0), sig), Ok(BeatTime(0)));
    assert_eq!(
        bar.boundary_at_or_after(BeatTime(BAR), sig),
        Ok(BeatTime(BAR))
    );
    assert_eq!(
        bar.boundary_at_or_after(BeatTime(BAR + 1), sig),
        Ok(BeatTime(2 * BAR))
    );
    assert_eq!(
        bar.boundary_at_or_after(BeatTime(1), sig),
        Ok(BeatTime(BAR))
    );
    assert_eq!(
        GlobalQuantisation::None.boundary_at_or_after(BeatTime(7), sig),
        Ok(BeatTime(7))
    );
    assert!(bar.boundary_at_or_after(BeatTime(u64::MAX), sig).is_err());
}

#[test]
fn clip_follow_on_grid_timer_into_explicit_bar_launches_at_the_timer() {
    let (clock, mut p) = clip_setup(LaunchQuantisation::Value(QuantisationValue::Bars(1)));
    fire_clip(&clock, &mut p, BAR);
    let launched = p.voices.last().unwrap();
    assert_eq!((launched.slot, launched.launch), (1, BeatTime(BAR)));
}

#[test]
fn clip_follow_on_grid_timer_into_global_clip_launches_at_the_timer() {
    let (clock, mut p) = clip_setup(LaunchQuantisation::Global);
    fire_clip(&clock, &mut p, BAR);
    assert_eq!(p.voices.last().unwrap().launch, BeatTime(BAR));
}

#[test]
fn clip_follow_off_grid_timer_still_rounds_up_to_explicit_grid() {
    let (clock, mut p) = clip_setup(LaunchQuantisation::Value(QuantisationValue::Bars(1)));
    Arc::make_mut(&mut p.project).tracks[0].slots[0]
        .clip
        .as_mut()
        .unwrap()
        .follow_actions
        .time = BeatTime(480);
    fire_clip(&clock, &mut p, 480);
    assert_eq!(p.voices.last().unwrap().launch, BeatTime(BAR));
}

#[test]
fn scene_follow_default_global_bar_plays_one_bar_not_two() {
    let (mut p, clock, start) = scene_setup(LaunchQuantisation::Global);
    assert_eq!(
        p.project.global_quantisation,
        GlobalQuantisation::Value(QuantisationValue::Bars(1))
    );
    fire_scene(&mut p, &clock, start);
    assert_eq!(p.last_triggered_scene, Some(SceneId(2)));
    let launched: Vec<_> = p
        .voices
        .iter()
        .filter(|v| v.slot == 1)
        .map(|v| v.launch)
        .collect();
    assert_eq!(launched, vec![BeatTime(BAR)]);
}

#[test]
fn scene_follow_on_grid_timer_into_explicit_bar_clips_launches_at_the_timer() {
    let (mut p, clock, start) = scene_setup(LaunchQuantisation::Value(QuantisationValue::Bars(1)));
    fire_scene(&mut p, &clock, start);
    let launched: Vec<_> = p
        .voices
        .iter()
        .filter(|v| v.slot == 1)
        .map(|v| v.launch)
        .collect();
    assert_eq!(launched, vec![BeatTime(BAR)]);
}

#[test]
fn manual_launch_still_uses_strictly_next_boundary() {
    let start = Instant::now();
    let mut p = Planner::new(super::tests::project(0.0, true), Duration::ZERO).unwrap();
    let mut clock = InternalClock::default();
    clock.play(start);
    let at = clock.instant_at(BeatTime(BAR)).unwrap();
    assert_eq!(
        p.launch(TrackId(1), 0, &mut clock, at).unwrap(),
        BeatTime(2 * BAR)
    );
}
