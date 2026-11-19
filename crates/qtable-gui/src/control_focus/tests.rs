use super::*;
use crate::control_focus::test_registry::TestKey;
use iced::Point;
use iced::advanced::renderer::Headless;
use iced::widget::{button, text};
use std::future::Future;
use std::task::{Context, Poll, Waker};

fn renderer() -> Renderer {
    let mut future = std::pin::pin!(Renderer::new(
        iced::Font::default(),
        iced::Pixels(crate::style::detail::TEXT_SIZE as f32),
        Some("tiny-skia")
    ));
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(Some(renderer)) => renderer,
        _ => panic!("software renderer must be immediately available"),
    }
}

fn id(name: &'static str, track: u64) -> ControlId<TestKey> {
    ControlId::action(
        View::Mixer,
        name,
        ControlOwner::Track(qtable_engine::model::TrackId(track)),
    )
}

fn live_button<'a>() -> Element<'a, ControlId<TestKey>> {
    button(text("x")).on_press(id("unused", 0)).into()
}

fn dead_button<'a>() -> Element<'a, ControlId<TestKey>> {
    button(text("x")).into()
}

/// Left-press at `point`, returning the claims published.
fn press(
    control: &mut Element<'_, ControlId<TestKey>>,
    point: Point,
    viewport: Rectangle,
) -> Vec<ControlId<TestKey>> {
    let renderer = renderer();
    let mut tree = Tree::new(&*control);
    let size = Size::new(100.0, 40.0);
    let node = control.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, size),
    );
    let mut messages = vec![];
    control.as_widget_mut().update(
        &mut tree,
        &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        Layout::new(&node),
        mouse::Cursor::Available(point),
        &renderer,
        &mut iced::advanced::clipboard::Null,
        &mut Shell::new(&mut messages),
        &viewport,
    );
    // Button messages other than claims (the `unused` press) are filtered by the caller's id.
    messages
}

fn viewport() -> Rectangle {
    Rectangle::with_size(Size::new(100.0, 40.0))
}

#[test]
fn control_bounds_operation_finds_only_the_requested_face() {
    let focus = ControlFocus::<TestKey>::default();
    let target = id("route", 1);
    let mut control = focus.wrap(target, true, false, live_button(), |id| id);
    let renderer = renderer();
    let mut tree = Tree::new(&control);
    let node = control
        .as_widget_mut()
        .layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, Size::new(100.0, 40.0)),
        )
        .move_to(Point::new(230.0, 320.0));
    for wanted in [target, id("route", 2)] {
        let mut operation = ControlBounds::new(wanted);
        control.as_widget_mut().operate(
            &mut tree,
            Layout::new(&node),
            &renderer,
            &mut widget::operation::black_box(&mut operation),
        );
        assert_eq!(
            operation.found,
            (wanted == target).then(|| Layout::new(&node).bounds())
        );
    }
}

#[test]
fn control_bounds_follow_scroll_translation_and_clip_hidden_faces() {
    use iced::widget::{Space, row, scrollable};
    let focus = ControlFocus::<TestKey>::default();
    let target = id("route", 1);
    let face = focus.wrap(
        target,
        true,
        false,
        button(text("face")).width(40).height(20).on_press(target),
        |id| id,
    );
    let mut control: Element<'_, ControlId<TestKey>> =
        scrollable(row![Space::new().width(120), face, Space::new().width(200)])
            .direction(scrollable::Direction::Horizontal(Default::default()))
            .id("bounds-test")
            .width(100)
            .height(40)
            .into();
    let renderer = renderer();
    let mut tree = Tree::new(&control);
    let node = control
        .as_widget_mut()
        .layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, Size::new(100.0, 40.0)),
        )
        .move_to(Point::new(230.0, 320.0));
    for (offset, expected) in [
        (
            100.0,
            Some(Rectangle {
                x: 250.0,
                y: 320.0,
                width: 40.0,
                height: 20.0,
            }),
        ),
        (
            150.0,
            Some(Rectangle {
                x: 230.0,
                y: 320.0,
                width: 10.0,
                height: 20.0,
            }),
        ),
        (200.0, None),
    ] {
        let mut scroll = widget::operation::scrollable::scroll_to::<()>(
            widget::Id::new("bounds-test"),
            widget::operation::scrollable::AbsoluteOffset {
                x: Some(offset),
                y: Some(0.0),
            },
        );
        control
            .as_widget_mut()
            .operate(&mut tree, Layout::new(&node), &renderer, &mut scroll);
        let mut operation = ControlBounds::new(target);
        control.as_widget_mut().operate(
            &mut tree,
            Layout::new(&node),
            &renderer,
            &mut widget::operation::black_box(&mut operation),
        );
        assert_eq!(operation.found, expected, "scroll offset {offset}");
    }
}

#[test]
fn the_same_help_entry_with_different_owners_moves_focus_between_them() {
    let mut focus = ControlFocus::<TestKey>::default();
    let (a, b) = (id("delay", 1), id("delay", 2));
    focus.claim(a);
    assert!(focus.has(a) && !focus.has(b));
    focus.claim(b);
    assert!(!focus.has(a) && focus.has(b));
    // The same field in two views stays two targets.
    let key = TestKey(0);
    assert_ne!(
        ControlId::field(View::ControlBar, key),
        ControlId::field(View::Main, key)
    );
    focus.clear();
    assert!(!focus.has(b));
}

#[test]
fn enabled_control_claims_on_click() {
    let target = id("delay", 1);
    let mut control =
        ControlFocus::<TestKey>::default().wrap(target, true, false, live_button(), |id| id);
    assert_eq!(
        press(&mut control, Point::new(3.0, 3.0), viewport()),
        [target]
    );
}

#[test]
fn disabled_and_clipped_controls_do_not_claim() {
    let target = id("delay", 1);
    let mut disabled =
        ControlFocus::<TestKey>::default().wrap(target, false, false, live_button(), |id| id);
    assert!(!press(&mut disabled, Point::new(3.0, 3.0), viewport()).contains(&target));
    let mut inert =
        ControlFocus::<TestKey>::default().wrap(target, true, false, dead_button(), |id| id);
    assert!(press(&mut inert, Point::new(3.0, 3.0), viewport()).is_empty());
    let mut clipped =
        ControlFocus::<TestKey>::default().wrap(target, true, false, live_button(), |id| id);
    let away = Rectangle::new(Point::new(0.0, 100.0), Size::new(100.0, 40.0));
    assert!(!press(&mut clipped, Point::new(3.0, 3.0), away).contains(&target));
}

#[test]
fn nested_wrappers_leave_focus_with_the_deepest_control() {
    let (outer, inner) = (id("group", 1), id("delay", 1));
    let focus = ControlFocus::<TestKey>::default();
    let child = focus.wrap(inner, true, false, live_button(), |id| id);
    let mut control = focus.wrap(outer, true, false, child, |id| id);
    let claims = press(&mut control, Point::new(3.0, 3.0), viewport());
    assert!(claims.contains(&inner) && !claims.contains(&outer));
}

#[test]
fn removed_controls_leave_no_outline_on_another_owner() {
    let mut focus = ControlFocus::<TestKey>::default();
    focus.claim(id("delay", 1));
    // Track 1 disappears; a control for track 2 with the same action is not outlined.
    assert!(!focus.has(id("delay", 2)));
}

#[test]
fn a_dropdown_keeps_focus_while_its_menu_row_is_chosen() {
    let target = id("quantisation", 1);
    let picked = id("picked", 9);
    let renderer = renderer();
    let options = ["1 Bar", "1/4"];
    let mut control = ControlFocus::<TestKey>::default().wrap(
        target,
        true,
        false,
        iced::widget::pick_list(options, Some("1 Bar"), move |_| picked).width(80),
        |id| id,
    );
    let mut tree = Tree::new(control.as_widget());
    let node = control.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, Size::new(100.0, 40.0)),
    );
    let screen = Rectangle::with_size(Size::new(400.0, 400.0));
    let left_press = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
    // Pressing the picker opens its menu and claims focus.
    let mut messages = vec![];
    control.as_widget_mut().update(
        &mut tree,
        &left_press,
        Layout::new(&node),
        mouse::Cursor::Available(Point::new(5.0, 5.0)),
        &renderer,
        &mut iced::advanced::clipboard::Null,
        &mut Shell::new(&mut messages),
        &screen,
    );
    assert_eq!(messages, [target]);
    // Choosing a row in the menu overlay selects it without moving or clearing the focus.
    let mut overlay = control
        .as_widget_mut()
        .overlay(
            &mut tree,
            Layout::new(&node),
            &renderer,
            &screen,
            Vector::ZERO,
        )
        .expect("an open picker shows its menu overlay");
    let menu = overlay.as_overlay_mut().layout(&renderer, screen.size());
    let row = Point::new(menu.bounds().x + 5.0, menu.bounds().y + 4.0);
    let mut messages = vec![];
    for event in [
        Event::Mouse(mouse::Event::CursorMoved { position: row }),
        left_press.clone(),
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
    ] {
        overlay.as_overlay_mut().update(
            &event,
            Layout::new(&menu),
            mouse::Cursor::Available(row),
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut Shell::new(&mut messages),
        );
    }
    assert_eq!(messages, [picked]);
}

#[test]
fn indexed_field_actions_have_distinct_focus_from_values_and_other_arguments() {
    let key = TestKey;
    let picker = ControlId::field_action(View::Clip, key(0), "kind");
    assert_ne!(picker, ControlId::field(View::Clip, key(0)));
    assert_ne!(picker, ControlId::field_action(View::Clip, key(1), "kind"));
    assert!(picker.field_key().is_none());
}

#[test]
fn corner_brackets_are_one_px_arms_in_each_corner_and_never_cross() {
    let bounds = Rectangle::new(Point::new(10.0, 20.0), iced::Size::new(40.0, 20.0));
    let bars = corner_brackets(bounds, 4.0);
    // Top-left: a 4 px bar along the top and one down the left edge.
    assert_eq!(
        bars[0],
        Rectangle::new(Point::new(10.0, 20.0), iced::Size::new(4.0, 1.0))
    );
    assert_eq!(
        bars[1],
        Rectangle::new(Point::new(10.0, 20.0), iced::Size::new(1.0, 4.0))
    );
    // Bottom-right ends exactly on the control's far corner.
    assert_eq!(
        bars[6],
        Rectangle::new(Point::new(46.0, 39.0), iced::Size::new(4.0, 1.0))
    );
    assert_eq!(
        bars[7],
        Rectangle::new(Point::new(49.0, 36.0), iced::Size::new(1.0, 4.0))
    );
    // A control smaller than two arms shortens them to meet in the middle.
    let tiny = corner_brackets(
        Rectangle::new(Point::ORIGIN, iced::Size::new(6.0, 6.0)),
        4.0,
    );
    assert_eq!(tiny[0].width, 3.0);
}

#[test]
fn corner_tokens_follow_the_measured_live_radii() {
    let theme = crate::control::LIVE_DARK;
    assert_eq!(
        (theme.corner_device, theme.corner_cell, theme.corner_control),
        (3.0, 2.0, 1.0)
    );
    let strip = crate::control::device_layout::strip_border(theme.corner_device);
    assert_eq!(strip.radius, iced::border::top(2.0));
}
