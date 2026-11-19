//! A drag on a number box is one undo step, set on release, as in Live.
//! The box is the real one, on the real Target face, driven through a retained widget tree; the
//! messages it publishes go through `update`, and the window subscription's own messages for each
//! event (`crate::window_event`) follow, as iced delivers them.
use super::*;
use qtable_engine::engine::Command as EngineCommand;

/// One input event: the widgets answer first, then the subscription's message for it arrives.
fn input(
    interaction: &mut Interaction,
    app: &mut App,
    id: DeviceId,
    event: Event,
    point: Point,
) -> Vec<Message> {
    let messages = interaction.step(app, id, event.clone(), point);
    let status = if interaction.captured {
        iced::event::Status::Captured
    } else {
        iced::event::Status::Ignored
    };
    if let Some(message) = crate::window_event(event, status) {
        let task = crate::update::update(app, message);
        interaction.task(app, id, task);
    }
    messages
}

fn moved(point: Point) -> Event {
    Event::Mouse(mouse::Event::CursorMoved { position: point })
}

/// How many undo steps the history holds, the fixture's own included (take a count before the
/// input). Undoing and redoing closes the open run, so call it only when the input is done.
fn steps(app: &mut App) -> usize {
    let mut count = 0;
    while app.engine.can_undo() {
        app.engine.execute(EngineCommand::Undo).unwrap();
        count += 1;
    }
    for _ in 0..count {
        app.engine.execute(EngineCommand::Redo).unwrap();
    }
    count
}

fn value(app: &App, id: DeviceId) -> Option<f64> {
    target(app, id).unwrap().1.parameters[0].fields[0].value
}

/// Press on the Target face's number box, drag up by `points` pixel stops, release. Returns the
/// messages the box published during the drag.
fn drag(
    interaction: &mut Interaction,
    app: &mut App,
    id: DeviceId,
    start: Point,
    stops: &[f32],
) -> Vec<Message> {
    let mut published = vec![];
    input(interaction, app, id, button(true), start);
    for rise in stops {
        let point = Point::new(start.x, start.y - rise);
        published.extend(input(interaction, app, id, moved(point), point));
    }
    let end = Point::new(start.x, start.y - stops.last().copied().unwrap_or(0.0));
    input(interaction, app, id, button(false), end);
    published
}

#[test]
fn a_target_number_box_drag_is_one_step_and_one_undo_returns_the_start() {
    let (mut app, id, _) = fixture(Control::NumberBox, true, true);
    let start = value(&app, id);
    let setup = steps(&mut app);
    let point = Interaction::bounds(&app, id, control(id, true)).center();
    let mut interaction = Interaction::new();
    let published = drag(
        &mut interaction,
        &mut app,
        id,
        point,
        &[10.0, 20.0, 30.0, 20.0, 40.0, 60.0],
    );
    assert!(
        published.len() >= 6,
        "the drag publishes several changed-and-commit pairs: {published:?}"
    );
    let end = value(&app, id);
    assert_ne!(end, start, "the drag moved the value");
    assert_eq!(steps(&mut app) - setup, 1, "the whole drag is one step");
    app.engine.execute(EngineCommand::Undo).unwrap();
    assert_eq!(
        value(&app, id),
        start,
        "one undo returns the value to the start"
    );
}

#[test]
fn two_target_number_box_drags_are_two_steps() {
    let (mut app, id, _) = fixture(Control::NumberBox, true, true);
    let start = value(&app, id);
    let setup = steps(&mut app);
    let point = Interaction::bounds(&app, id, control(id, true)).center();
    let mut interaction = Interaction::new();
    drag(&mut interaction, &mut app, id, point, &[10.0, 20.0, 30.0]);
    let first = value(&app, id);
    // Away from the box so the second press is no double-click.
    interaction.clock.advance(Duration::from_secs(2));
    drag(&mut interaction, &mut app, id, point, &[10.0, 20.0, 30.0]);
    assert_eq!(steps(&mut app) - setup, 2);
    app.engine.execute(EngineCommand::Undo).unwrap();
    assert_eq!(value(&app, id), first);
    app.engine.execute(EngineCommand::Undo).unwrap();
    assert_eq!(value(&app, id), start);
}

fn key_released(key: keyboard::key::Named, modifiers: keyboard::Modifiers) -> Event {
    let key = keyboard::Key::Named(key);
    Event::Keyboard(keyboard::Event::KeyReleased {
        modified_key: key.clone(),
        key,
        physical_key: keyboard::key::Physical::Unidentified(
            keyboard::key::NativeCode::Unidentified,
        ),
        location: keyboard::Location::Standard,
        modifiers,
    })
}

fn modifiers_changed(modifiers: keyboard::Modifiers) -> Event {
    Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers))
}

/// ⌘Z as the window delivers a command chord: the key, no text (a focused text input inserts any
/// text a key event carries, whatever the modifiers).
fn cmd_z(modifiers: keyboard::Modifiers) -> Event {
    key(keyboard::Key::Character("z".into()), None, modifiers)
}

/// Tour step 4: a drag released outside the window, one that lost the window mid-drag, and a click
/// each make a step of their own, undone in turn.
#[test]
fn drags_released_outside_or_cut_short_by_the_window_and_a_click_are_steps_of_their_own() {
    let (mut app, id, _) = fixture(Control::NumberBox, true, true);
    let track = app.project().tracks[0].id;
    let start = value(&app, id);
    let setup = steps(&mut app);
    let point = Interaction::bounds(&app, id, control(id, true)).center();
    let mut interaction = Interaction::new();
    // A drag up and out of the window (far above the box), released out there.
    input(&mut interaction, &mut app, id, button(true), point);
    for rise in [10.0, 30.0, 90.0, 400.0, 700.0] {
        let there = Point::new(point.x, point.y - rise);
        input(&mut interaction, &mut app, id, moved(there), there);
    }
    let outside = Point::new(point.x, point.y - 700.0);
    input(&mut interaction, &mut app, id, button(false), outside);
    let first = value(&app, id);
    assert_ne!(first, start);
    // A second drag, cut short by the window losing focus (⌘Tab), then the release elsewhere.
    interaction.clock.advance(Duration::from_secs(2));
    input(&mut interaction, &mut app, id, button(true), point);
    for rise in [-10.0, -30.0] {
        let there = Point::new(point.x, point.y - rise);
        input(&mut interaction, &mut app, id, moved(there), there);
    }
    input(
        &mut interaction,
        &mut app,
        id,
        Event::Window(iced::window::Event::Unfocused),
        point,
    );
    input(&mut interaction, &mut app, id, button(false), point);
    let second = value(&app, id);
    assert_ne!(second, first);
    // A click on a toggle.
    let _ = crate::update::update(
        &mut app,
        Message::Mixer(crate::mixer_band::MixerMessage::Activator(track)),
    );
    let toggled = app.project().clone();
    assert_eq!(steps(&mut app) - setup, 3);
    app.engine.execute(EngineCommand::Undo).unwrap();
    assert_ne!(app.project(), &toggled, "the click is undone first");
    assert_eq!(value(&app, id), second);
    app.engine.execute(EngineCommand::Undo).unwrap();
    assert_eq!(value(&app, id), first, "then the drag the window cut short");
    app.engine.execute(EngineCommand::Undo).unwrap();
    assert_eq!(value(&app, id), start, "then the drag that ended outside");
}

/// Tour step 5: letting go of Shift in the middle of a fine drag ends nothing.
#[test]
fn letting_go_of_shift_in_the_middle_of_a_number_box_drag_keeps_one_step() {
    let (mut app, id, _) = fixture(Control::NumberBox, true, true);
    let start = value(&app, id);
    let setup = steps(&mut app);
    let point = Interaction::bounds(&app, id, control(id, true)).center();
    let mut interaction = Interaction::new();
    let shift = keyboard::Modifiers::SHIFT;
    input(
        &mut interaction,
        &mut app,
        id,
        modifiers_changed(shift),
        point,
    );
    input(&mut interaction, &mut app, id, button(true), point);
    for rise in [10.0, 20.0, 30.0] {
        let there = Point::new(point.x, point.y - rise);
        input(&mut interaction, &mut app, id, moved(there), there);
    }
    // Shift released first: the widgets hear the key and the modifier change, then the subscription.
    let none = keyboard::Modifiers::NONE;
    input(
        &mut interaction,
        &mut app,
        id,
        key_released(keyboard::key::Named::Shift, none),
        point,
    );
    input(
        &mut interaction,
        &mut app,
        id,
        modifiers_changed(none),
        point,
    );
    for rise in [40.0, 60.0, 80.0] {
        let there = Point::new(point.x, point.y - rise);
        input(&mut interaction, &mut app, id, moved(there), there);
    }
    let end = Point::new(point.x, point.y - 80.0);
    input(&mut interaction, &mut app, id, button(false), end);
    assert_ne!(value(&app, id), start);
    assert_eq!(steps(&mut app) - setup, 1);
    app.engine.execute(EngineCommand::Undo).unwrap();
    assert_eq!(value(&app, id), start);
}

/// Tour step 6a: ⌘Z in the middle of a drag takes the value back to the start; the rest of the drag
/// is one new step, and one more ⌘Z returns to the start without touching anything older.
#[test]
fn undo_in_the_middle_of_a_number_box_drag_lets_the_rest_of_the_drag_be_one_new_step() {
    let (mut app, id, _) = fixture(Control::NumberBox, true, true);
    let start = value(&app, id);
    let setup = steps(&mut app);
    let point = Interaction::bounds(&app, id, control(id, true)).center();
    let mut interaction = Interaction::new();
    input(&mut interaction, &mut app, id, button(true), point);
    for rise in [10.0, 20.0, 30.0] {
        let there = Point::new(point.x, point.y - rise);
        input(&mut interaction, &mut app, id, moved(there), there);
    }
    assert_ne!(value(&app, id), start);
    input(
        &mut interaction,
        &mut app,
        id,
        cmd_z(keyboard::Modifiers::COMMAND),
        point,
    );
    assert_eq!(value(&app, id), start, "the undo took the drag back");
    for rise in [40.0, 50.0, 60.0] {
        let there = Point::new(point.x, point.y - rise);
        input(&mut interaction, &mut app, id, moved(there), there);
    }
    let end = Point::new(point.x, point.y - 60.0);
    input(&mut interaction, &mut app, id, button(false), end);
    assert_ne!(value(&app, id), start, "the drag carries on");
    assert_eq!(
        steps(&mut app) - setup,
        1,
        "the rest of the drag is one step"
    );
    app.engine.execute(EngineCommand::Undo).unwrap();
    assert_eq!(value(&app, id), start);
}

/// Tour step 6b: a held arrow key in the number box, ⌘Z while it repeats, the repeats carry on.
#[test]
fn undo_while_an_arrow_key_repeats_in_a_number_box_lets_the_repeats_be_one_new_step() {
    let (mut app, id, _) = fixture(Control::NumberBox, true, true);
    let start = value(&app, id);
    let setup = steps(&mut app);
    let point = Interaction::bounds(&app, id, control(id, true)).center();
    let mut interaction = Interaction::new();
    // A click selects the value for the keys.
    input(&mut interaction, &mut app, id, button(true), point);
    input(&mut interaction, &mut app, id, button(false), point);
    let up = keyboard::Key::Named(keyboard::key::Named::ArrowUp);
    let arrow = || key(up.clone(), None, keyboard::Modifiers::NONE);
    for _ in 0..3 {
        input(&mut interaction, &mut app, id, arrow(), point);
    }
    assert_ne!(value(&app, id), start);
    input(
        &mut interaction,
        &mut app,
        id,
        cmd_z(keyboard::Modifiers::COMMAND),
        point,
    );
    assert_eq!(value(&app, id), start, "the undo took the repeats back");
    for _ in 0..3 {
        input(&mut interaction, &mut app, id, arrow(), point);
    }
    input(
        &mut interaction,
        &mut app,
        id,
        key_released(keyboard::key::Named::ArrowUp, keyboard::Modifiers::NONE),
        point,
    );
    assert_ne!(value(&app, id), start, "the repeats carry on");
    assert_eq!(
        steps(&mut app) - setup,
        1,
        "the repeats after it are one step"
    );
    app.engine.execute(EngineCommand::Undo).unwrap();
    assert_eq!(value(&app, id), start);
}

/// The tour could not reach steps 5 and 6 on faders, sliders and dials either: the Target face's
/// slider and dial, in both send modes, make one step for a drag too.
#[test]
fn a_target_slider_or_dial_drag_is_one_step_in_either_send_mode() {
    for kind in [Control::Slider, Control::Dial] {
        for mode in [SendMode::Continuous, SendMode::OnRelease] {
            let (mut app, id, field) = fixture(kind, true, true);
            run(
                &mut app,
                edit(
                    id,
                    TargetEdit::SetParameterSendMode {
                        key: field.entry.clone(),
                        mode,
                    },
                ),
            );
            let start = value(&app, id);
            let setup = steps(&mut app);
            let bounds = Interaction::bounds(&app, id, control(id, true));
            let point = bounds.center();
            let mut interaction = Interaction::new();
            input(&mut interaction, &mut app, id, button(true), point);
            let mut there = point;
            for fraction in [0.1, 0.2, 0.3, 0.2, 0.15] {
                there = if kind == Control::Dial {
                    Point::new(point.x, point.y - 100.0 * fraction)
                } else {
                    Point::new(point.x + bounds.width * fraction, point.y)
                };
                input(&mut interaction, &mut app, id, moved(there), there);
            }
            input(&mut interaction, &mut app, id, button(false), there);
            let end = value(&app, id);
            assert_ne!(end, start, "{kind:?} {mode:?}: the drag moved the value");
            assert_eq!(
                steps(&mut app) - setup,
                1,
                "{kind:?} {mode:?}: the whole drag is one step"
            );
            app.engine.execute(EngineCommand::Undo).unwrap();
            assert_eq!(value(&app, id), start, "{kind:?} {mode:?}");
        }
    }
}
