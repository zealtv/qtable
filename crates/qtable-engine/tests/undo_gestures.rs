//! Gesture tokens: which commands join one step, what ends the open
//! run, and what a joined step does when it ends where it began.
use qtable_engine::{
    edit::ProjectEdit,
    engine::{Command, Engine, Gesture, Receipt},
    model::*,
};
use std::time::Instant;

fn volume(track: u64) -> ParameterRef {
    ParameterRef::volume(TrackId(track))
}
fn field(track: u64) -> FieldRef {
    FieldRef {
        parameter: volume(track),
        field: FieldKey("value".into()),
    }
}

fn engine() -> Engine {
    let tracks = vec![
        Track::new(TrackId(1), "One", 1),
        Track::new(TrackId(2), "Two", 1),
        Track::new(TrackId(3), "Three", 1),
    ];
    Engine::new(Project {
        tempo: Tempo::new(120.0).unwrap(),
        tracks,
        scenes: vec![Scene {
            id: SceneId(1),
            name: "Scene".into(),
            follow_actions: Default::default(),
        }],
        ..Project::default()
    })
    .unwrap()
}

fn context(engine: &Engine, gesture: Option<Gesture>) -> qtable_engine::engine::CommandContext {
    engine
        .command_context(Receipt {
            received_at: Instant::now(),
            sequence: None,
        })
        .with_gesture(gesture)
}

fn run(engine: &mut Engine, command: Command, gesture: Option<Gesture>) {
    let context = context(engine, gesture);
    engine.execute_with_context(command, context).unwrap();
}

fn set(engine: &mut Engine, track: u64, value: f64, gesture: Option<Gesture>) {
    run(
        engine,
        Command::Edit(ProjectEdit::SetParameter {
            target: volume(track),
            values: vec![(FieldKey("value".into()), value)],
        }),
        gesture,
    );
}

fn value(engine: &Engine, track: u64) -> f64 {
    engine.project().field(&field(track)).unwrap().value
}

/// How many steps the history holds. It counts by undoing and redoing, which ends the open run
/// (an undo does, and a redo lands closed steps), so call it last in a test, never between two
/// commands whose joining the test is about.
fn steps(engine: &mut Engine) -> usize {
    let mut count = 0;
    while engine.can_undo() {
        engine.execute(Command::Undo).unwrap();
        count += 1;
    }
    for _ in 0..count {
        engine.execute(Command::Redo).unwrap();
    }
    count
}

#[test]
fn one_gesture_makes_one_step_and_one_undo_returns_the_start() {
    let mut engine = engine();
    let start = value(&engine, 1);
    let drag = engine.gesture();
    for n in 1..=50 {
        set(&mut engine, 1, f64::from(n) / 100.0, Some(drag));
    }
    assert_eq!(value(&engine, 1), 0.5);
    assert_eq!(steps(&mut engine), 1);
    assert_eq!(engine.undo_label(), Some("Set Value"));
    engine.execute(Command::Undo).unwrap();
    assert_eq!(value(&engine, 1), start);
    assert!(!engine.can_undo());
    engine.execute(Command::Redo).unwrap();
    assert_eq!(value(&engine, 1), 0.5);
}

#[test]
fn separate_gestures_and_commands_without_one_make_separate_steps() {
    let mut engine = engine();
    let (a, b) = (engine.gesture(), engine.gesture());
    assert_ne!(a, b);
    set(&mut engine, 1, 0.2, Some(a));
    set(&mut engine, 1, 0.3, Some(a));
    set(&mut engine, 1, 0.4, Some(b));
    // No token: its own step, and it ends the open run.
    set(&mut engine, 1, 0.5, None);
    set(&mut engine, 1, 0.6, None);
    // The run was broken by the token-less steps, so `b` no longer joins anything: a, b, the
    // two token-less steps and a fifth for `b` again.
    set(&mut engine, 1, 0.7, Some(b));
    assert_eq!(steps(&mut engine), 5);
}

#[test]
fn a_gesture_that_ends_where_it_began_leaves_no_step_and_the_saved_identity() {
    let mut engine = engine();
    let saved = engine.content_id();
    let origin = value(&engine, 1);
    let drag = engine.gesture();
    set(&mut engine, 1, 0.9, Some(drag));
    assert_ne!(engine.content_id(), saved);
    set(&mut engine, 1, 0.3, Some(drag));
    assert_eq!(engine.undo_label(), Some("Set Value"));
    // Dragged back to the value it began with.
    set(&mut engine, 1, origin, Some(drag));
    assert!(!engine.can_undo(), "the step vanished");
    assert!(!engine.can_redo());
    assert_eq!(engine.content_id(), saved);
    // A later move of the same gesture starts a step again.
    set(&mut engine, 1, 0.4, Some(drag));
    assert_eq!(steps(&mut engine), 1);
}

#[test]
fn a_gesture_back_at_its_start_under_another_step_makes_a_new_step_and_loses_none() {
    let mut engine = engine();
    let (a, b) = (engine.gesture(), engine.gesture());
    let origin = value(&engine, 1);
    set(&mut engine, 1, 0.9, Some(a));
    set(&mut engine, 2, 0.9, Some(b));
    // `a` is under `b`, so it takes no more commands: this is a new step, and track 1 ends
    // where it started without any step vanishing.
    set(&mut engine, 1, origin, Some(a));
    assert_eq!(steps(&mut engine), 3);
}

#[test]
fn two_controls_moved_in_turn_make_two_steps_undone_in_reverse_order() {
    let mut engine = engine();
    let (a, b) = (engine.gesture(), engine.gesture());
    let (one, two) = (value(&engine, 1), value(&engine, 2));
    set(&mut engine, 1, 0.2, Some(a));
    set(&mut engine, 1, 0.3, Some(a));
    set(&mut engine, 2, 0.6, Some(b));
    set(&mut engine, 2, 0.7, Some(b));
    assert_eq!(steps(&mut engine), 2);
    engine.execute(Command::Undo).unwrap();
    assert_eq!((value(&engine, 1), value(&engine, 2)), (0.3, two));
    engine.execute(Command::Undo).unwrap();
    assert_eq!((value(&engine, 1), value(&engine, 2)), (one, two));
}

#[test]
fn a_gesture_moved_again_under_another_starts_a_new_step() {
    let mut engine = engine();
    let (a, b) = (engine.gesture(), engine.gesture());
    set(&mut engine, 1, 0.2, Some(a));
    set(&mut engine, 2, 0.6, Some(b));
    // Only the top step takes a move: `a` is under `b`, so each alternation is a step.
    set(&mut engine, 1, 0.3, Some(a));
    set(&mut engine, 2, 0.7, Some(b));
    assert_eq!(steps(&mut engine), 4);
}

#[test]
fn four_alternating_moves_undo_b2_a2_b1_a1() {
    // Live check, 2026-11-11: fader A, fader B, A, B. Live's undos
    // take back B's last move, A's last move, B's first, A's first.
    let mut engine = engine();
    let (a, b) = (engine.gesture(), engine.gesture());
    let (a0, b0) = (value(&engine, 1), value(&engine, 2));
    set(&mut engine, 1, 0.2, Some(a));
    set(&mut engine, 2, 0.6, Some(b));
    set(&mut engine, 1, 0.3, Some(a));
    set(&mut engine, 2, 0.7, Some(b));
    engine.execute(Command::Undo).unwrap();
    assert_eq!((value(&engine, 1), value(&engine, 2)), (0.3, 0.6), "B2");
    engine.execute(Command::Undo).unwrap();
    assert_eq!((value(&engine, 1), value(&engine, 2)), (0.2, 0.6), "A2");
    engine.execute(Command::Undo).unwrap();
    assert_eq!((value(&engine, 1), value(&engine, 2)), (0.2, b0), "B1");
    engine.execute(Command::Undo).unwrap();
    assert_eq!((value(&engine, 1), value(&engine, 2)), (a0, b0), "A1");
    assert!(!engine.can_undo());
}

#[test]
fn an_undo_a_redo_a_load_and_a_close_each_end_the_open_run() {
    let mut engine = engine();
    let g = engine.gesture();
    let other = engine.gesture();
    set(&mut engine, 1, 0.2, Some(g));
    set(&mut engine, 2, 0.2, Some(other));
    // Undo takes back `other`'s step; `g`'s step must not take later commands.
    engine.execute(Command::Undo).unwrap();
    set(&mut engine, 1, 0.3, Some(g));
    assert_eq!(steps(&mut engine), 2, "a new step, not a join");

    // A redo's landing is a closed step.
    let mut engine = self::engine();
    let g = engine.gesture();
    set(&mut engine, 1, 0.2, Some(g));
    engine.execute(Command::Undo).unwrap();
    engine.execute(Command::Redo).unwrap();
    set(&mut engine, 1, 0.3, Some(g));
    assert_eq!(steps(&mut engine), 2);

    // A client's explicit close.
    let mut engine = self::engine();
    let g = engine.gesture();
    set(&mut engine, 1, 0.2, Some(g));
    engine.close_gestures();
    set(&mut engine, 1, 0.3, Some(g));
    assert_eq!(steps(&mut engine), 2);
}

#[test]
fn a_load_clears_the_history_so_the_token_of_a_step_before_it_joins_nothing() {
    let mut engine = engine();
    let g = engine.gesture();
    set(&mut engine, 1, 0.4, Some(g));
    let project = engine.project().clone();
    engine.execute(Command::LoadProject(project)).unwrap();
    assert!(!engine.can_undo());
    set(&mut engine, 1, 0.5, Some(g));
    assert_eq!(steps(&mut engine), 1);
    engine.execute(Command::Undo).unwrap();
    assert_eq!(value(&engine, 1), 0.4, "the step began at the loaded set");
}

#[test]
fn a_join_clears_redo_and_launches_and_view_state_leave_the_run_open() {
    let mut engine = engine();
    let g = engine.gesture();
    set(&mut engine, 1, 0.2, Some(g));
    engine.execute(Command::Undo).unwrap();
    assert!(engine.can_redo());
    // After the undo, `g` starts a new step, which clears redo.
    set(&mut engine, 1, 0.3, Some(g));
    assert!(!engine.can_redo());
    // Neither a launch nor a stop is a step, so neither ends the run.
    engine.execute(Command::Play).unwrap();
    engine
        .execute(Command::StopTrack { track: TrackId(1) })
        .unwrap();
    set(&mut engine, 1, 0.4, Some(g));
    assert_eq!(steps(&mut engine), 1);
}

#[test]
fn a_view_state_edit_leaves_the_run_open() {
    let mut engine = engine();
    run(
        &mut engine,
        Command::Edit(ProjectEdit::CreateGroup {
            tracks: vec![TrackId(1), TrackId(2)],
        }),
        None,
    );
    let group = engine
        .project()
        .tracks
        .iter()
        .find(|t| t.is_group())
        .unwrap()
        .id;
    let g = engine.gesture();
    set(&mut engine, 3, 0.2, Some(g));
    // A fold in the middle of the run (the same gesture's token, or none) makes no step and
    // closes nothing.
    run(
        &mut engine,
        Command::Edit(ProjectEdit::SetGroupFolded {
            group,
            folded: true,
        }),
        None,
    );
    set(&mut engine, 3, 0.3, Some(g));
    // The group's step, and one for `g`.
    assert_eq!(steps(&mut engine), 2);
}

#[test]
fn a_token_less_step_ends_the_run_for_good_even_after_it_is_undone() {
    // A controller moves (a token), a mapped switch makes a token-less step,
    // an undo takes that back, and the controller moves again. The new move must not join the
    // step from before the switch.
    let mut engine = engine();
    let origin = value(&engine, 1);
    let knob = engine.gesture();
    set(&mut engine, 1, 0.2, Some(knob));
    set(&mut engine, 1, 0.3, Some(knob));
    set(&mut engine, 2, 0.9, None);
    engine.execute(Command::Undo).unwrap();
    set(&mut engine, 1, 0.4, Some(knob));
    engine.execute(Command::Undo).unwrap();
    assert_eq!(
        value(&engine, 1),
        0.3,
        "only the move after the undo is taken back"
    );
    engine.execute(Command::Undo).unwrap();
    assert_eq!(value(&engine, 1), origin);
    assert!(!engine.can_undo());
}

#[test]
fn an_undo_in_the_middle_of_a_gesture_takes_back_the_steps_so_far_and_the_rest_starts_one() {
    let mut engine = engine();
    let origin = value(&engine, 1);
    let drag = engine.gesture();
    set(&mut engine, 1, 0.2, Some(drag));
    set(&mut engine, 1, 0.3, Some(drag));
    engine.execute(Command::Undo).unwrap();
    assert_eq!(value(&engine, 1), origin);
    // The gesture carries on: its later moves are one new step, not a join with the step
    // the undo took away.
    set(&mut engine, 1, 0.4, Some(drag));
    set(&mut engine, 1, 0.5, Some(drag));
    assert!(!engine.can_redo());
    engine.execute(Command::Undo).unwrap();
    assert_eq!(value(&engine, 1), origin);
    assert!(!engine.can_undo());
}

#[test]
fn tempo_moves_of_one_gesture_join_and_other_commands_with_one_make_their_own() {
    let mut engine = engine();
    let g = engine.gesture();
    run(&mut engine, Command::SetTempo(130.0), Some(g));
    run(&mut engine, Command::SetTempo(140.0), Some(g));
    assert_eq!(steps(&mut engine), 1);
    engine.execute(Command::Undo).unwrap();
    assert_eq!(engine.project().tempo.bpm(), 120.0);
}

#[test]
fn many_moves_of_one_control_stay_one_step_while_alternating_controls_each_make_one() {
    let mut engine = engine();
    let (a, b) = (engine.gesture(), engine.gesture());
    for n in 0..1000 {
        set(&mut engine, 1, f64::from(n % 100) / 200.0 + 0.1, Some(a));
    }
    assert_eq!(engine.undo_label(), Some("Set Value"));
    // Nothing joins under another step: ten alternations are ten steps.
    for n in 0..10 {
        let x = f64::from(n) / 100.0 + 0.1;
        set(&mut engine, 2, x, Some(b));
        set(&mut engine, 1, x, Some(a));
    }
    assert_eq!(steps(&mut engine), 21);
}

#[test]
fn a_mouse_gesture_closed_by_the_client_leaves_a_controller_move_a_new_step() {
    // Live check, 2026-11-11: the controls move in turn, then a mouse drag, then the
    // first control again.
    let mut engine = engine();
    let (first, second, mouse) = (engine.gesture(), engine.gesture(), engine.gesture());
    set(&mut engine, 1, 0.2, Some(first));
    set(&mut engine, 1, 0.3, Some(first));
    set(&mut engine, 2, 0.6, Some(second));
    set(&mut engine, 3, 0.5, Some(mouse));
    set(&mut engine, 3, 0.6, Some(mouse));
    engine.close_gestures();
    set(&mut engine, 1, 0.4, Some(first));
    assert_eq!(steps(&mut engine), 4);
}
