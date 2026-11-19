use super::*;
use crate::control_focus::test_registry::TestHelp;
use crate::control_focus::test_registry::TestKey;
use crate::control_focus::{ControlId, View};
use iced::widget::canvas::Program;
use iced::{Color, Size};

fn dial(fraction: f64, value: &str) -> Dial {
    Dial {
        fraction,
        label: "X".into(),
        value: value.into(),
    }
}

fn hosted(fraction: f64) -> Hosted<impl Fn(f64) -> f64, impl Fn(f64) -> f64, TestKey> {
    Hosted {
        control: Control::new(
            ControlId::action(
                View::Track,
                "dial",
                crate::control_focus::ControlOwner::Track(qtable_engine::model::TrackId(1)),
            ),
            TestHelp::TargetValue,
            dial(fraction, "0"),
        ),
        focused: false,
        shift: false,
        theme: crate::control::LIVE_DARK,
        on_change: |fraction: f64| fraction,
        // A release reports itself as the negative of the value.
        on_release: |fraction: f64| -fraction,
    }
}

#[test]
fn readout_draws_below_the_knob_inside_the_cell() {
    use iced::Element;
    use iced::advanced::renderer::Headless;
    use iced::advanced::{Layout, layout, renderer, widget::Tree};

    for value in ["0.25", "1"] {
        let mut renderer = crate::grid::test_renderer();
        let mut widget: Element<'_, ()> = iced::widget::Canvas::new(Hosted {
            control: Control::new(
                ControlId::<TestKey>::action(
                    View::Track,
                    "dial",
                    crate::control_focus::ControlOwner::Track(qtable_engine::model::TrackId(1)),
                ),
                TestHelp::TargetValue,
                dial(0.25, value),
            ),
            focused: false,
            shift: false,
            theme: crate::control::LIVE_DARK,
            on_change: |_| (),
            on_release: |_| (),
        })
        .width(WIDTH)
        .height(HEIGHT)
        .into();
        let mut tree = Tree::new(&widget);
        let size = Size::new(WIDTH, HEIGHT);
        let node = widget.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, size),
        );
        assert_eq!(node.size(), size);
        widget.as_widget().draw(
            &tree,
            &mut renderer,
            &iced::Theme::Dark,
            &renderer::Style::default(),
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            &Rectangle::with_size(size),
        );
        let pixels = renderer.screenshot(Size::new(WIDTH as u32, HEIGHT as u32), 1.0, Color::BLACK);
        let readout = &pixels[52 * WIDTH as usize * 4..66 * WIDTH as usize * 4];
        assert!(
            readout.chunks_exact(4).any(|p| p[..3] != [0, 0, 0]),
            "{value} must draw visible text in the readout band"
        );
    }
}

#[test]
fn relative_drag_clamps_and_reverses_at_endpoints_without_dead_travel() {
    let mut drag = Drag {
        y: 200.0,
        fraction: 0.375,
    };
    assert!(!drag.move_to(200.0, false));
    assert!(drag.move_to(175.0, false));
    assert_eq!(drag.fraction, 0.625);
    assert!(drag.move_to(-500.0, false));
    assert_eq!(drag.fraction, 1.0);
    assert!(!drag.move_to(-600.0, false));
    assert!(drag.move_to(500.0, false));
    assert_eq!(drag.fraction, 0.0);
    assert!(drag.move_to(200.0, false));
    assert_eq!(drag.fraction, 1.0);
}

#[test]
fn press_publishes_nothing_and_a_drag_steps_from_the_held_value_then_releases() {
    let host = hosted(0.5);
    let bounds = Rectangle::with_size(Size::new(WIDTH, HEIGHT));
    let mut state = Gesture::default();
    let at = |y| mouse::Cursor::Available(Point::new(20.0, y));
    let press = canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
    let press = host.update(&mut state, &press, bounds, at(30.0)).unwrap();
    assert!(press.into_inner().0.is_none());
    let moved = canvas::Event::Mouse(mouse::Event::CursorMoved {
        position: Point::new(20.0, 10.0),
    });
    let (message, ..) = host
        .update(&mut state, &moved, bounds, at(10.0))
        .unwrap()
        .into_inner();
    assert_eq!(message, Some(0.7));
    let release = canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
    let (message, ..) = host
        .update(&mut state, &release, bounds, at(10.0))
        .unwrap()
        .into_inner();
    assert_eq!(message, Some(-0.7));
    assert!(host.update(&mut state, &moved, bounds, at(0.0)).is_none());
}

#[test]
fn a_window_losing_focus_ends_the_drag_with_a_release() {
    let host = hosted(0.25);
    let bounds = Rectangle::with_size(Size::new(WIDTH, HEIGHT));
    let mut state = Gesture::default();
    let press = canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
    host.update(
        &mut state,
        &press,
        bounds,
        mouse::Cursor::Available(Point::new(20.0, 30.0)),
    );
    let blur = canvas::Event::Window(iced::window::Event::Unfocused);
    let (message, ..) = host
        .update(&mut state, &blur, bounds, mouse::Cursor::Unavailable)
        .unwrap()
        .into_inner();
    assert_eq!(message, Some(-0.25));
}

#[test]
fn a_disabled_dial_ignores_the_pointer() {
    let mut host = hosted(0.5);
    host.control.state.enabled = false;
    let bounds = Rectangle::with_size(Size::new(WIDTH, HEIGHT));
    let press = canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
    let mut state = Gesture::default();
    assert!(
        host.update(
            &mut state,
            &press,
            bounds,
            mouse::Cursor::Available(Point::new(20.0, 30.0))
        )
        .is_none()
    );
    assert!(state.drag.is_none());
}

#[test]
fn the_arc_follows_live_and_goes_grey_when_inactive_or_disabled_while_text_keeps_its_ink() {
    let t = crate::control::LIVE_DARK;
    let kind = dial(0.5, "0.5");
    let active = kind.look(&t, State::default(), Interaction::Idle);
    assert_eq!(active.accent, t.value_arc);
    assert_eq!(active.ink, t.control_foreground);
    // Inactive but editable: the arc greys; the label, value and needle keep their ink.
    let inactive = State {
        inactive: true,
        ..State::default()
    };
    let look = kind.look(&t, inactive, Interaction::Idle);
    assert_eq!(look.accent, t.value_arc_inactive);
    assert_eq!(look.ink, t.control_foreground);
    // Disabled dims the ink too.
    let disabled = State {
        enabled: false,
        ..State::default()
    };
    let look = kind.look(&t, disabled, Interaction::Idle);
    assert_eq!(look.accent, t.value_arc_inactive);
    assert_eq!(look.ink, t.text_disabled);
}

#[test]
fn a_double_click_publishes_the_default_as_a_release_and_starts_no_drag() {
    let clock = crate::gesture_clock::test::Clock::new();
    let mut host = hosted(0.5);
    host.control = host.control.default_to(0.25);
    let mut state = Gesture::default();
    let at = |y| mouse::Cursor::Available(Point::new(20.0, y));
    let bounds = Rectangle::with_size(Size::new(WIDTH, HEIGHT));
    let press = canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
    let release = canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
    let message = |action: Option<canvas::Action<f64>>| action.and_then(|a| a.into_inner().0);
    // A press publishes nothing; the release reports the unchanged value.
    assert_eq!(
        message(host.update(&mut state, &press, bounds, at(30.0))),
        None
    );
    assert_eq!(
        message(host.update(&mut state, &release, bounds, at(30.0))),
        Some(-0.5)
    );
    clock.advance(std::time::Duration::from_millis(100));
    assert_eq!(
        message(host.update(&mut state, &press, bounds, at(30.0))),
        Some(-0.25)
    );
    let moved = canvas::Event::Mouse(mouse::Event::CursorMoved {
        position: Point::new(20.0, 10.0),
    });
    assert!(host.update(&mut state, &moved, bounds, at(10.0)).is_none());
    assert!(
        host.update(&mut state, &release, bounds, at(10.0))
            .is_none()
    );
}

#[test]
fn shift_drag_is_four_times_finer_and_changing_shift_has_no_jump() {
    let mut host = hosted(0.5);
    host.shift = true;
    let mut state = Gesture::default();
    let bounds = Rectangle::with_size(Size::new(WIDTH, HEIGHT));
    let at = |y| mouse::Cursor::Available(Point::new(20.0, y));
    host.update(
        &mut state,
        &canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        bounds,
        at(50.0),
    );
    let moved = |y| {
        canvas::Event::Mouse(mouse::Event::CursorMoved {
            position: Point::new(20.0, y),
        })
    };
    let message = |a: Option<canvas::Action<f64>>| a.and_then(|a| a.into_inner().0);
    assert_eq!(
        message(host.update(&mut state, &moved(10.0), bounds, at(10.0))),
        Some(0.6)
    );
    host.update(
        &mut state,
        &canvas::Event::Keyboard(keyboard::Event::ModifiersChanged(keyboard::Modifiers::NONE)),
        bounds,
        at(10.0),
    );
    assert_eq!(
        message(host.update(&mut state, &moved(10.0), bounds, at(10.0))),
        None
    );
    assert_eq!(
        message(host.update(&mut state, &moved(0.0), bounds, at(0.0))),
        Some(0.7)
    );
}
