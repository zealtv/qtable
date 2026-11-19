use super::*;
use crate::control_focus::test_registry::TestHelp;
use crate::control_focus::test_registry::TestKey;
use crate::control_focus::{ControlId, ControlOwner, View};
use iced::Size;
use qtable_engine::model::TrackId;

fn slider(fraction: f64) -> Slider {
    Slider {
        fraction,
        label: "Rate 0.5".into(),
    }
}

fn hosted() -> Hosted<impl Fn(f64) -> f64, impl Fn(f64) -> f64, TestKey> {
    Hosted {
        control: Control::new(
            ControlId::action(View::Track, "slider", ControlOwner::Track(TrackId(1))),
            TestHelp::TargetValue,
            slider(0.5),
        ),
        focused: false,
        shift: false,
        theme: crate::control::LIVE_DARK,
        on_change: |fraction: f64| fraction,
        // A release reports itself as the negative of the value.
        on_release: |fraction: f64| -fraction,
    }
}

const BOUNDS: Rectangle = Rectangle {
    x: 0.0,
    y: 0.0,
    width: 100.0,
    height: 20.0,
};

fn event(event: mouse::Event) -> canvas::Event {
    canvas::Event::Mouse(event)
}

fn message(action: Option<canvas::Action<f64>>) -> Option<f64> {
    action.and_then(|a| a.into_inner().0)
}

fn at(x: f32) -> mouse::Cursor {
    mouse::Cursor::Available(Point::new(x, 10.0))
}

fn moved(x: f32) -> canvas::Event {
    event(mouse::Event::CursorMoved {
        position: Point::new(x, 10.0),
    })
}

#[test]
fn press_is_silent_and_drag_is_relative_rounded_and_clamped() {
    let host = hosted();
    let mut state = Drag::default();
    let press = event(mouse::Event::ButtonPressed(mouse::Button::Left));
    assert_eq!(
        message(host.update(&mut state, &press, BOUNDS, at(25.0))),
        None
    );
    assert_eq!(
        message(host.update(&mut state, &moved(35.03), BOUNDS, at(35.03))),
        Some(0.6)
    );
    // A rebuilt view's value and bounds must not replace the press-time mapping.
    let mut rebuilt = hosted();
    rebuilt.control.kind.fraction = 0.6;
    let resized = Rectangle {
        width: 200.0,
        ..BOUNDS
    };
    assert_eq!(
        message(rebuilt.update(&mut state, &moved(45.0), resized, at(45.0))),
        Some(0.7)
    );
    assert_eq!(
        message(rebuilt.update(&mut state, &moved(45.0), resized, at(45.0))),
        None
    );
    assert_eq!(
        message(rebuilt.update(&mut state, &moved(200.0), resized, at(200.0))),
        Some(1.0)
    );
    assert_eq!(
        message(rebuilt.update(&mut state, &moved(-100.0), resized, at(-100.0))),
        Some(0.0)
    );
    let release = event(mouse::Event::ButtonReleased(mouse::Button::Left));
    assert_eq!(
        message(rebuilt.update(&mut state, &release, resized, at(-100.0))),
        Some(-0.0)
    );
    assert!(
        rebuilt
            .update(&mut state, &release, resized, at(-100.0))
            .is_none()
    );
}

#[test]
fn click_or_blur_without_drag_publishes_nothing() {
    for finish in [
        event(mouse::Event::ButtonReleased(mouse::Button::Left)),
        canvas::Event::Window(iced::window::Event::Unfocused),
    ] {
        let host = hosted();
        let mut state = Drag::default();
        let press = event(mouse::Event::ButtonPressed(mouse::Button::Left));
        assert_eq!(
            message(host.update(&mut state, &press, BOUNDS, at(25.0))),
            None
        );
        assert_eq!(
            message(host.update(&mut state, &moved(25.0), BOUNDS, at(25.0))),
            None
        );
        assert_eq!(
            message(host.update(&mut state, &finish, BOUNDS, at(25.0))),
            None
        );
        assert!(state.dragging.is_none());
    }
}

#[test]
fn a_window_losing_focus_releases_the_changed_value_once() {
    let host = hosted();
    let mut state = Drag::default();
    let press = event(mouse::Event::ButtonPressed(mouse::Button::Left));
    host.update(&mut state, &press, BOUNDS, at(40.0));
    host.update(&mut state, &moved(50.0), BOUNDS, at(50.0));
    let blur = canvas::Event::Window(iced::window::Event::Unfocused);
    assert_eq!(
        message(host.update(&mut state, &blur, BOUNDS, mouse::Cursor::Unavailable)),
        Some(-0.6)
    );
    assert!(
        host.update(&mut state, &blur, BOUNDS, mouse::Cursor::Unavailable)
            .is_none()
    );
}

#[test]
fn scrolled_motion_uses_translated_cursor_even_outside_the_viewport() {
    let host = hosted();
    let mut state = Drag::default();
    let press = event(mouse::Event::ButtonPressed(mouse::Button::Left));
    host.update(&mut state, &press, BOUNDS, at(40.0));
    // Raw event coordinates and content coordinates differ by a scroll offset.
    assert_eq!(
        message(host.update(&mut state, &moved(-550.0), BOUNDS, at(50.0))),
        Some(0.6)
    );
    assert_eq!(
        message(host.update(
            &mut state,
            &moved(-540.0),
            BOUNDS,
            mouse::Cursor::Levitating(Point::new(60.0, 10.0))
        )),
        Some(0.7)
    );
}

#[test]
fn zero_width_does_not_send_or_divide_by_zero() {
    let host = hosted();
    let mut movement = Movement {
        x: 0.0,
        width: 0.0,
        origin: host.control.kind.fraction,
        fraction: 0.5,
        changed: false,
        initial: 0.5,
    };
    assert!(!movement.move_to(10.0, false));
    assert_eq!(movement.fraction, 0.5);
}

#[test]
fn a_disabled_slider_ignores_the_pointer() {
    let mut host = hosted();
    host.control.state.enabled = false;
    let mut state = Drag::default();
    let press = event(mouse::Event::ButtonPressed(mouse::Button::Left));
    assert!(
        host.update(
            &mut state,
            &press,
            BOUNDS,
            mouse::Cursor::Available(Point::new(40.0, 10.0))
        )
        .is_none()
    );
}

#[test]
fn the_bar_follows_live_and_greys_when_inactive_or_disabled() {
    let t = crate::control::LIVE_DARK;
    let kind = slider(0.5);
    let active = kind.look(&t, State::default(), Interaction::Idle);
    assert_eq!(
        (active.accent, active.ink, active.fill, active.frame),
        (
            t.value_bar,
            t.control_foreground,
            t.fader_well,
            t.fader_edge
        )
    );
    // Inactive but editable: the bar greys; the text keeps its ink.
    let inactive = State {
        inactive: true,
        ..State::default()
    };
    let look = kind.look(&t, inactive, Interaction::Idle);
    assert_eq!(
        (look.accent, look.ink),
        (t.value_arc_inactive, t.control_foreground)
    );
    // Disabled dims the text too.
    let disabled = State {
        enabled: false,
        ..State::default()
    };
    let look = kind.look(&t, disabled, Interaction::Idle);
    assert_eq!(
        (look.accent, look.ink),
        (t.value_arc_inactive, t.text_disabled)
    );
}

#[test]
fn the_label_draws_inside_the_well_without_touching_its_edge_rows() {
    use iced::advanced::renderer::Headless;
    use iced::advanced::{Layout, layout, renderer, widget::Tree};
    use iced::{Color, Element};

    let theme = crate::control::Theme {
        // Hide the bar and frame so only the label can light pixels.
        fader_well: Color::BLACK,
        fader_edge: Color::BLACK,
        ..crate::control::LIVE_DARK
    };
    let mut renderer = crate::grid::test_renderer();
    let mut host = hosted();
    host.theme = theme;
    host.control.kind.fraction = 0.0;
    let size = Size::new(100.0, 20.0);
    let mut widget: Element<'_, f64> = iced::widget::Canvas::new(host).width(100).height(20).into();
    let mut tree = Tree::new(&widget);
    let node =
        widget
            .as_widget_mut()
            .layout(&mut tree, &renderer, &layout::Limits::new(Size::ZERO, size));
    widget.as_widget().draw(
        &tree,
        &mut renderer,
        &iced::Theme::Dark,
        &renderer::Style::default(),
        Layout::new(&node),
        mouse::Cursor::Unavailable,
        &Rectangle::with_size(size),
    );
    let pixels = renderer.screenshot(Size::new(100, 20), 1.0, Color::BLACK);
    let lit_rows: Vec<usize> = (0..20)
        .filter(|row| {
            pixels[row * 100 * 4..(row + 1) * 100 * 4]
                .chunks_exact(4)
                .any(|p| p[..3] != [0, 0, 0])
        })
        .collect();
    assert!(!lit_rows.is_empty(), "the label must draw");
    // 11 px text centred in a 20 px well keeps clear of the well's own edge rows.
    assert!(lit_rows.iter().all(|r| (3..17).contains(r)), "{lit_rows:?}");
}

#[test]
fn a_double_click_publishes_the_default_as_a_release_and_starts_no_drag() {
    let clock = crate::gesture_clock::test::Clock::new();
    let mut host = hosted();
    host.control = host.control.default_to(0.25);
    let mut state = Drag::default();
    let at = |x| mouse::Cursor::Available(Point::new(x, 10.0));
    let press = event(mouse::Event::ButtonPressed(mouse::Button::Left));
    // The first click is silent; only the second press sends the reset.
    assert_eq!(
        message(host.update(&mut state, &press, BOUNDS, at(80.0))),
        None
    );
    let release = event(mouse::Event::ButtonReleased(mouse::Button::Left));
    assert_eq!(
        message(host.update(&mut state, &release, BOUNDS, at(80.0))),
        None
    );
    clock.advance(std::time::Duration::from_millis(100));
    assert_eq!(
        message(host.update(&mut state, &press, BOUNDS, at(80.0))),
        Some(-0.25)
    );
    // No drag follows the reset, so moving and releasing publish nothing.
    let moved = event(mouse::Event::CursorMoved {
        position: Point::new(30.0, 10.0),
    });
    assert!(host.update(&mut state, &moved, BOUNDS, at(30.0)).is_none());
    assert!(
        host.update(&mut state, &release, BOUNDS, at(30.0))
            .is_none()
    );
}

#[test]
fn without_a_default_a_double_click_is_two_presses() {
    let clock = crate::gesture_clock::test::Clock::new();
    let host = hosted();
    let mut state = Drag::default();
    let at = mouse::Cursor::Available(Point::new(80.0, 10.0));
    let press = event(mouse::Event::ButtonPressed(mouse::Button::Left));
    host.update(&mut state, &press, BOUNDS, at);
    clock.advance(std::time::Duration::from_millis(100));
    // Without a default, even a second press only starts a pending drag.
    assert_eq!(message(host.update(&mut state, &press, BOUNDS, at)), None);
}

#[test]
fn a_stationary_pointer_preserves_a_precise_origin() {
    let mut host = hosted();
    host.control.kind.fraction = 0.12345;
    let mut state = Drag::default();
    let press = event(mouse::Event::ButtonPressed(mouse::Button::Left));
    host.update(&mut state, &press, BOUNDS, at(25.0));
    assert_eq!(
        message(host.update(&mut state, &moved(25.0), BOUNDS, at(25.0))),
        None
    );
    assert_eq!(
        message(host.update(&mut state, &moved(35.0), BOUNDS, at(35.0))),
        Some(0.12345 + 0.1)
    );
}

#[test]
fn shift_drag_is_four_times_finer_and_changing_shift_has_no_jump() {
    let mut host = hosted();
    host.shift = true;
    let mut state = Drag::default();
    host.update(
        &mut state,
        &event(mouse::Event::ButtonPressed(mouse::Button::Left)),
        BOUNDS,
        at(25.0),
    );
    assert_eq!(
        message(host.update(&mut state, &moved(65.0), BOUNDS, at(65.0))),
        Some(0.6)
    );
    host.update(
        &mut state,
        &canvas::Event::Keyboard(keyboard::Event::ModifiersChanged(keyboard::Modifiers::NONE)),
        BOUNDS,
        at(65.0),
    );
    assert_eq!(
        message(host.update(&mut state, &moved(65.0), BOUNDS, at(65.0))),
        None
    );
    assert_eq!(
        message(host.update(&mut state, &moved(75.0), BOUNDS, at(75.0))),
        Some(0.7)
    );
}
