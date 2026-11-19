//! Events go through Controls::number and a persistent iced tree, including view rebuilds.
use super::*;
use crate::control::{Control, Controls, LIVE_DARK, Number};
use crate::control_focus::test_registry::TestHelp;
use crate::control_focus::test_registry::TestKey;
use crate::control_focus::{ControlFocus, ControlId, ControlKey, View};
use iced::advanced::widget::operation::{self, Outcome};
use iced::keyboard::{self, Key, key::Named};
use iced::{Element, Point, keyboard::Modifiers};

#[derive(Clone, Debug, PartialEq)]
enum Message {
    Draft(usize, String),
    Submit(usize),
    Reset(usize),
    Focus,
    Entered,
}
impl crate::control::widgets::ControlMessage<TestKey> for Message {
    fn info(_: crate::control::help::Hover<TestHelp>) -> Self {
        Self::Focus
    }
    fn commit_number() -> Self {
        Self::Submit(0)
    }
    fn field_entered() -> Self {
        Self::Entered
    }
    fn focused(_: ControlId<TestKey>) -> Self {
        Self::Focus
    }
}

fn view(
    values: &[String; 2],
    invalid: bool,
    enabled: bool,
    chrome: bool,
) -> Element<'static, Message> {
    let focus = ControlFocus::default();
    let controls = Controls::new(LIVE_DARK, &focus);
    iced::widget::column(values.iter().enumerate().map(|(index, value)| {
        controls.number(
            Control::new(
                ControlId::field(View::Track, TestKey(index as u64)),
                TestHelp::TargetNumber,
                if chrome {
                    Number::new("").chrome()
                } else {
                    Number::new("")
                },
            )
            .invalid(invalid)
            .enabled(enabled),
            value,
            move |s| Message::Draft(index, s),
            Message::Submit(index),
            Some(Message::Reset(index)),
        )
    }))
    .into()
}

struct Harness {
    values: [String; 2],
    invalid: bool,
    enabled: bool,
    chrome: bool,
    redraw: iced::window::RedrawRequest,
    tree: Tree,
    renderer: Renderer,
}
impl Harness {
    fn new() -> Self {
        let values = ["120.00".into(), "30.00".into()];
        let tree = Tree::new(view(&values, false, true, false));
        Self {
            values,
            invalid: false,
            enabled: true,
            chrome: false,
            redraw: iced::window::RedrawRequest::Wait,
            tree,
            renderer: crate::grid::test_renderer(),
        }
    }
    fn element(&mut self) -> (Element<'static, Message>, layout::Node) {
        let mut element = view(&self.values, self.invalid, self.enabled, self.chrome);
        self.tree.diff(&element);
        let node = element.as_widget_mut().layout(
            &mut self.tree,
            &self.renderer,
            &layout::Limits::new(Size::ZERO, Size::new(200.0, 100.0)),
        );
        (element, node)
    }
    fn event(&mut self, event: Event, point: Point) -> (Vec<Message>, bool) {
        let (mut element, node) = self.element();
        let mut messages = vec![];
        let mut shell = Shell::new(&mut messages);
        element.as_widget_mut().update(
            &mut self.tree,
            &event,
            Layout::new(&node),
            mouse::Cursor::Available(point),
            &self.renderer,
            &mut iced::advanced::clipboard::Null,
            &mut shell,
            &Rectangle::with_size(Size::new(200.0, 100.0)),
        );
        let captured = shell.is_event_captured();
        self.redraw = shell.redraw_request();
        for message in &messages {
            if let Message::Draft(i, value) = message {
                self.values[*i] = value.clone();
            }
        }
        (messages, captured)
    }
    fn operate(&mut self, op: impl Operation + 'static) {
        let (mut element, node) = self.element();
        let mut op: Box<dyn Operation> = Box::new(op);
        loop {
            element.as_widget_mut().operate(
                &mut self.tree,
                Layout::new(&node),
                &self.renderer,
                op.as_mut(),
            );
            match op.finish() {
                Outcome::Chain(next) => op = next,
                _ => break,
            }
        }
    }
    fn click(&mut self, point: Point) -> Vec<Message> {
        let (messages, _) = self.event(
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            point,
        );
        self.event(
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            point,
        );
        messages
    }
    fn type_text(&mut self, text: &str) {
        for ch in text.chars() {
            self.event(
                key(Key::Character(ch.to_string().into()), Some(&ch.to_string())),
                Point::ORIGIN,
            );
        }
    }

    fn screenshot(&mut self, cursor: Point) -> Vec<u8> {
        use iced::advanced::{Renderer as _, renderer::Headless};
        let (element, node) = self.element();
        let bounds = Rectangle::with_size(Size::new(200.0, 100.0));
        self.renderer.reset(bounds);
        element.as_widget().draw(
            &self.tree,
            &mut self.renderer,
            &Theme::Dark,
            &renderer::Style::default(),
            Layout::new(&node),
            mouse::Cursor::Available(cursor),
            &bounds,
        );
        self.renderer
            .screenshot(Size::new(200, 100), 1.0, Color::BLACK)
    }
}
fn key(key: Key, text: Option<&str>) -> Event {
    Event::Keyboard(keyboard::Event::KeyPressed {
        modified_key: key.clone(),
        key,
        physical_key: keyboard::key::Physical::Unidentified(
            keyboard::key::NativeCode::Unidentified,
        ),
        location: keyboard::Location::Standard,
        modifiers: Modifiers::NONE,
        text: text.map(Into::into),
        repeat: false,
    })
}
fn named(key_name: Named) -> Event {
    key(Key::Named(key_name), None)
}

#[test]
fn click_selects_the_whole_number_then_typing_and_navigation_continue_across_rebuilds() {
    let mut h = Harness::new();
    assert!(
        !h.click(Point::new(5.0, 5.0))
            .iter()
            .any(|m| matches!(m, Message::Draft(..) | Message::Submit(_)))
    );
    h.type_text("98.5");
    assert_eq!(h.values[0], "98.5");
    h.event(named(Named::ArrowLeft), Point::ORIGIN);
    h.type_text("2");
    assert_eq!(h.values[0], "98.25");
    h.event(named(Named::Backspace), Point::ORIGIN);
    assert_eq!(h.values[0], "98.5");
    // Read-only focus probes must not select again while an edit is active.
    h.operate(operation::map(operation::focusable::count(), |_| ()));
    h.type_text("3");
    assert_eq!(h.values[0], "98.35");
}

#[test]
fn tab_and_shift_tab_operations_select_each_new_number_once() {
    let mut h = Harness::new();
    h.operate(operation::focusable::focus_next::<()>());
    h.type_text("91");
    assert_eq!(h.values[0], "91");
    h.operate(operation::focusable::focus_next::<()>());
    h.type_text("45");
    assert_eq!(h.values[1], "45");
    h.operate(operation::focusable::focus_previous::<()>());
    h.type_text("87");
    assert_eq!(h.values[0], "87");
}

#[test]
fn return_submits_but_blur_and_escape_leave_commit_and_restoration_to_the_owner() {
    let mut h = Harness::new();
    h.click(Point::new(5.0, 5.0));
    h.type_text("93");
    let (messages, captured) = h.event(named(Named::Enter), Point::ORIGIN);
    assert!(captured);
    assert_eq!(
        messages,
        [Message::Submit(0), Message::Entered],
        "Return submits, then tells the app Enter was pressed"
    );
    h.type_text("5");
    assert_eq!(
        h.values[0], "5",
        "Return locks the edit and readies a new replacement"
    );
    let (messages, captured) = h.event(named(Named::Escape), Point::ORIGIN);
    assert!(
        captured,
        "numeric Escape still routes cancellation to its owner"
    );
    assert!(
        matches!(
            crate::focus::event_message(
                &Key::Named(Named::Escape),
                Modifiers::NONE,
                false,
                !captured
            ),
            Some(crate::messages::Message::CancelField)
        ),
        "the app routes Escape to its owner even after iced captures it"
    );
    assert!(messages.is_empty());
    // The owner restores committed text on Escape; the widget must keep that restoration.
    h.values[0] = "93.00".into();
    assert_eq!(first_state(&h.tree).unwrap().mode, Mode::Selected);
    h.type_text("82");
    assert_eq!(
        h.values[0], "82",
        "no click or focus operation is needed after Escape"
    );
}

#[test]
fn invalid_draft_survives_refocus_and_can_be_navigated_or_replaced() {
    let mut h = Harness::new();
    h.values[0] = "-bad".into();
    h.invalid = true;
    h.click(Point::new(5.0, 5.0));
    assert_eq!(h.values[0], "-bad");
    h.click(Point::new(150.0, 80.0));
    h.operate(operation::focusable::focus(TestKey(0).input_id()));
    assert_eq!(h.values[0], "-bad");
    h.event(named(Named::ArrowRight), Point::ORIGIN);
    assert_eq!(h.values[0], "-bad");
    h.type_text("2");
    assert_eq!(
        h.values[0], "2",
        "navigation before typing keeps whole-value replacement ready"
    );
    h.click(Point::new(150.0, 80.0));
    h.operate(operation::focusable::focus(TestKey(0).input_id()));
    h.type_text("42");
    assert_eq!(h.values[0], "42");
}

#[test]
fn double_click_resets_without_draft_or_submit_and_disabled_numbers_ignore_typing() {
    let clock = crate::gesture_clock::test::Clock::new();
    let mut h = Harness::new();
    h.click(Point::new(5.0, 5.0));
    clock.advance(std::time::Duration::from_millis(100));
    let messages = h.click(Point::new(5.0, 5.0));
    assert_eq!(
        messages.iter().filter(|m| **m == Message::Reset(0)).count(),
        1
    );
    assert!(
        !messages
            .iter()
            .any(|m| matches!(m, Message::Draft(..) | Message::Submit(_)))
    );
    h.enabled = false;
    h.click(Point::new(5.0, 25.0));
    h.type_text("9");
    assert_eq!(h.values[1], "30.00");
}

#[test]
fn an_existing_exact_draft_can_explicitly_continue_at_its_end_after_focus() {
    let mut h = Harness::new();
    h.values[0] = "7".into();
    h.operate(operation::focusable::focus(TestKey(0).input_id()));
    h.operate(operation::text_input::move_cursor_to_end(
        TestKey(0).input_id(),
    ));
    h.type_text("8.5");
    assert_eq!(h.values[0], "78.5");
}

#[test]
fn pointer_motion_between_press_and_release_never_makes_a_partial_selection() {
    for end in [
        Point::new(25.0, 5.0),
        Point::new(48.0, 5.0),
        Point::new(150.0, 80.0),
    ] {
        let mut h = Harness::new();
        h.event(
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            Point::new(5.0, 5.0),
        );
        h.event(
            Event::Mouse(mouse::Event::CursorMoved { position: end }),
            end,
        );
        h.event(
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            end,
        );
        h.event(named(Named::ArrowLeft), Point::ORIGIN);
        h.event(named(Named::ArrowRight), Point::ORIGIN);
        h.type_text("93.5");
        assert_eq!(h.values[0], "93.5", "release at {end:?}");
    }
}

fn first_state(tree: &Tree) -> Option<&State> {
    if tree.tag == tree::Tag::of::<State>() {
        return Some(tree.state.downcast_ref::<State>());
    }
    tree.children.iter().find_map(first_state)
}

fn pixels_of(image: &[u8], color: Color) -> usize {
    let rgba = color.into_rgba8();
    image.chunks_exact(4).filter(|pixel| *pixel == rgba).count()
}

#[test]
fn chrome_selection_has_no_caret_until_typing_then_caret_fades_between_theme_colours() {
    let mut h = Harness::new();
    h.chrome = true;
    let outside = Point::new(150.0, 80.0);
    let idle = h.screenshot(outside);
    let hovered = h.screenshot(Point::new(5.0, 5.0));
    h.event(
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        Point::new(5.0, 5.0),
    );
    let pressed = h.screenshot(outside);
    let end = Point::new(35.0, 5.0);
    h.event(
        Event::Mouse(mouse::Event::CursorMoved { position: end }),
        end,
    );
    h.event(
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        end,
    );
    let selected = h.screenshot(outside);
    assert_eq!(
        &idle[(3 * 200 + 10) * 4..][..4],
        LIVE_DARK.transport_off_background.into_rgba8()
    );
    assert_eq!(
        &hovered[(3 * 200 + 10) * 4..][..4],
        LIVE_DARK.transport_selection_background.into_rgba8()
    );
    assert_eq!(
        &pressed[(3 * 200 + 10) * 4..][..4],
        LIVE_DARK.control_selection_frame.into_rgba8()
    );
    assert_eq!(
        &selected[(3 * 200 + 10) * 4..][..4],
        LIVE_DARK.transport_selection_background.into_rgba8()
    );
    assert_eq!(first_state(&h.tree).unwrap().mode, Mode::Selected);
    assert!(
        selected
            .chunks_exact(4)
            .all(|p| p[0] == p[1] && p[1] == p[2]),
        "Chrome selection is a grey face, with no coloured text-selection region or focus box"
    );
    assert_eq!(pixels_of(&selected, LIVE_DARK.numeric_cursor), 0);
    assert_eq!(pixels_of(&selected, LIVE_DARK.chosen_record), 0);
    h.type_text("65");
    assert_eq!(h.values[0], "65");
    let white = h.screenshot(outside);
    assert!(
        pixels_of(&white, LIVE_DARK.numeric_cursor) > 0,
        "typing caret must be visible at the right edge"
    );
    let epoch = first_state(&h.tree).unwrap().epoch;
    h.event(
        Event::Window(iced::window::Event::RedrawRequested(
            epoch + Duration::from_millis(250),
        )),
        outside,
    );
    let middle_color =
        first_state(&h.tree)
            .unwrap()
            .caret_color(crate::control::widgets::Looks::of(
                &Number::new("").chrome(),
                &LIVE_DARK,
                Default::default(),
            ));
    assert!(
        middle_color.g < LIVE_DARK.numeric_cursor.g && middle_color.g > LIVE_DARK.chosen_record.g
    );
    let middle = h.screenshot(outside);
    assert!(
        pixels_of(&middle, middle_color) > 0,
        "the rendered caret must show an intermediate colour"
    );
    assert_eq!(pixels_of(&middle, LIVE_DARK.numeric_cursor), 0);
    assert_eq!(pixels_of(&middle, LIVE_DARK.chosen_record), 0);
    let frame_time = epoch + Duration::from_millis(250);
    assert!(
        matches!(h.redraw, iced::window::RedrawRequest::At(next)
        if next > frame_time && next <= frame_time + Duration::from_millis(33)),
        "a smooth fade needs another animation frame, not just a redraw at the endpoint"
    );
    h.event(
        Event::Window(iced::window::Event::RedrawRequested(
            epoch + Duration::from_millis(500),
        )),
        outside,
    );
    let red = h.screenshot(outside);
    assert!(pixels_of(&red, LIVE_DARK.chosen_record) > 0);
    assert_eq!(pixels_of(&red, LIVE_DARK.numeric_cursor), 0);
    h.event(named(Named::Enter), outside);
    let committed = h.screenshot(outside);
    assert_eq!(pixels_of(&committed, LIVE_DARK.chosen_record), 0);
    assert_eq!(pixels_of(&committed, LIVE_DARK.numeric_cursor), 0);
    if let Some(path) = std::env::var_os("QTABLE_NUMERIC_PREVIEW_DIR") {
        let path = std::path::PathBuf::from(path);
        std::fs::create_dir_all(&path).unwrap();
        for (name, image) in [
            ("idle", idle),
            ("hovered", hovered),
            ("pressed", pressed),
            ("selected", selected),
            ("typing-white", white),
            ("typing-middle", middle),
            ("typing-red", red),
        ] {
            std::fs::write(path.join(format!("{name}.rgba")), image).unwrap();
        }
    }
}

#[test]
fn caret_fade_reaches_each_endpoint_every_half_second_and_is_smooth_at_the_turn() {
    let looks =
        crate::control::widgets::Looks::of(&Number::new(""), &LIVE_DARK, Default::default());
    let mut state = State::default();
    let mut sample = |ms| {
        state.now = state.epoch + Duration::from_millis(ms);
        state.caret_color(looks)
    };
    assert_eq!(sample(0), LIVE_DARK.numeric_cursor);
    let forward = sample(250);
    let near_red = sample(499);
    assert_eq!(sample(500), LIVE_DARK.chosen_record);
    let after_red = sample(501);
    let reverse = sample(750);
    assert_eq!(sample(1000), LIVE_DARK.numeric_cursor);
    assert!((forward.g - reverse.g).abs() < 0.00001);
    assert!((near_red.g - LIVE_DARK.chosen_record.g).abs() < 0.00001);
    assert!((near_red.g - after_red.g).abs() < 0.00001);
}

#[test]
fn copying_or_selecting_all_before_typing_keeps_numeric_selection() {
    let mut h = Harness::new();
    h.click(Point::new(5.0, 5.0));
    for character in ["a", "c"] {
        let mut event = key(Key::Character(character.into()), None);
        if let Event::Keyboard(keyboard::Event::KeyPressed { modifiers, .. }) = &mut event {
            *modifiers = Modifiers::COMMAND;
        }
        h.event(event, Point::ORIGIN);
        assert_eq!(first_state(&h.tree).unwrap().mode, Mode::Selected);
    }
    h.type_text("100");
    assert_eq!(h.values[0], "100");
}

#[test]
fn window_blur_rearms_replacement_and_does_not_leave_a_typing_cursor() {
    let mut h = Harness::new();
    h.click(Point::new(5.0, 5.0));
    h.type_text("93");
    h.event(Event::Window(iced::window::Event::Unfocused), Point::ORIGIN);
    assert_eq!(first_state(&h.tree).unwrap().mode, Mode::Selected);
    h.event(Event::Window(iced::window::Event::Focused), Point::ORIGIN);
    h.type_text("104");
    assert_eq!(h.values[0], "104");
}

#[test]
fn chrome_readout_uses_the_numeric_face_without_becoming_a_focus_target() {
    use iced::advanced::{Renderer as _, renderer::Headless};
    let focus = ControlFocus::default();
    let controls = Controls::new(LIVE_DARK, &focus);
    let mut element: Element<'_, Message> = controls.number_readout(
        Control::new(
            ControlId::field(View::ControlBar, TestKey(0)),
            TestHelp::Position,
            Number::new("").chrome(),
        ),
        "1. 1. 1",
        Font::MONOSPACE,
        12,
    );
    let mut tree = Tree::new(&element);
    let mut renderer = crate::grid::test_renderer();
    let node = element.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, Size::new(200.0, 100.0)),
    );
    let bounds = Rectangle::with_size(Size::new(200.0, 100.0));
    let cursor = mouse::Cursor::Available(Point::new(5.0, 5.0));
    let mut messages = Vec::new();
    element.as_widget_mut().update(
        &mut tree,
        &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        Layout::new(&node),
        cursor,
        &renderer,
        &mut iced::advanced::clipboard::Null,
        &mut Shell::new(&mut messages),
        &bounds,
    );
    let mut count = operation::focusable::count();
    element.as_widget_mut().operate(
        &mut tree,
        Layout::new(&node),
        &renderer,
        &mut operation::black_box(&mut count),
    );
    assert!(matches!(count.finish(), Outcome::Some(count) if count.total == 0));
    assert!(
        messages
            .iter()
            .all(|m| !matches!(m, Message::Draft(..) | Message::Submit(..)))
    );
    renderer.reset(bounds);
    element.as_widget().draw(
        &tree,
        &mut renderer,
        &Theme::Dark,
        &renderer::Style::default(),
        Layout::new(&node),
        cursor,
        &bounds,
    );
    let pressed = renderer.screenshot(Size::new(200, 100), 1.0, Color::BLACK);
    assert_eq!(
        &pressed[(3 * 200 + 10) * 4..][..4],
        LIVE_DARK.control_selection_frame.into_rgba8()
    );
    element.as_widget_mut().update(
        &mut tree,
        &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        Layout::new(&node),
        cursor,
        &renderer,
        &mut iced::advanced::clipboard::Null,
        &mut Shell::new(&mut messages),
        &bounds,
    );
    renderer.reset(bounds);
    element.as_widget().draw(
        &tree,
        &mut renderer,
        &Theme::Dark,
        &renderer::Style::default(),
        Layout::new(&node),
        cursor,
        &bounds,
    );
    let image = renderer.screenshot(Size::new(200, 100), 1.0, Color::BLACK);
    assert_eq!(
        &image[(3 * 200 + 10) * 4..][..4],
        LIVE_DARK.transport_selection_background.into_rgba8()
    );
}

#[test]
fn arrows_nudge_selected_numbers_capture_navigation_and_wait_for_typed_commit() {
    let mut h = Harness::new();
    let point = Point::new(20.0, 10.0);
    // An unfocused numeric box must leave navigation alone.
    let (messages, captured) = h.event(key(Key::Named(Named::ArrowUp), None), point);
    assert!(messages.is_empty());
    assert!(!captured);
    h.click(point);
    let (messages, captured) = h.event(key(Key::Named(Named::ArrowUp), None), point);
    assert!(captured);
    assert_eq!(
        messages,
        vec![Message::Draft(0, "120.01".into()), Message::Submit(0)]
    );
    assert_eq!(h.values[0], "120.01");
    let mut fine = key(Key::Named(Named::ArrowDown), None);
    if let Event::Keyboard(keyboard::Event::KeyPressed {
        modifiers, repeat, ..
    }) = &mut fine
    {
        *modifiers = Modifiers::SHIFT;
        *repeat = true;
    }
    h.event(fine, point);
    assert_eq!(h.values[0], "120.0075");
    h.type_text("93");
    let (messages, captured) = h.event(key(Key::Named(Named::ArrowUp), None), point);
    assert!(captured);
    assert!(messages.is_empty());
    assert_eq!(h.values[0], "93");
    h.event(key(Key::Named(Named::Enter), None), point);
    h.event(key(Key::Named(Named::ArrowDown), None), point);
    assert_eq!(h.values[0], "92.99");
    h.type_text("-");
    h.event(key(Key::Named(Named::Enter), None), point);
    let (messages, captured) = h.event(key(Key::Named(Named::ArrowUp), None), point);
    assert!(captured);
    assert!(messages.is_empty());
    assert_eq!(h.values[0], "-");
}

fn press(h: &mut Harness, point: Point) -> (Vec<Message>, bool) {
    h.event(
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        point,
    )
}
fn drag_to(h: &mut Harness, point: Point) -> (Vec<Message>, bool) {
    h.event(
        Event::Mouse(mouse::Event::CursorMoved { position: point }),
        point,
    )
}
fn release(h: &mut Harness, point: Point) {
    h.event(
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        point,
    );
}
fn shift(h: &mut Harness, on: bool) {
    h.event(
        Event::Keyboard(keyboard::Event::ModifiersChanged(if on {
            Modifiers::SHIFT
        } else {
            Modifiers::NONE
        })),
        Point::ORIGIN,
    );
}
/// What a gesture asked the owner to change; focus notices are not edits.
fn edits(messages: Vec<Message>) -> Vec<Message> {
    messages
        .into_iter()
        .filter(|m| {
            matches!(
                m,
                Message::Draft(..) | Message::Submit(_) | Message::Reset(_)
            )
        })
        .collect()
}
fn step() -> f32 {
    crate::style::detail::NUMBER_DRAG_STEP
}

#[test]
fn vertical_drag_moves_a_number_relative_to_the_press_up_increases_down_decreases() {
    let mut h = Harness::new();
    let start = Point::new(20.0, 10.0);
    let (messages, _) = press(&mut h, start);
    assert!(edits(messages).is_empty(), "a press alone sends nothing");
    let (messages, captured) = drag_to(&mut h, Point::new(20.0, 10.0 - 5.0 * step()));
    assert!(captured);
    assert_eq!(
        edits(messages),
        vec![Message::Draft(0, "120.05".into()), Message::Submit(0)]
    );
    // Back past the press point: the same relative scale, not a jump to the pointer.
    drag_to(&mut h, Point::new(20.0, 10.0 + 2.0 * step()));
    assert_eq!(h.values[0], "119.98");
    release(&mut h, start);
    assert_eq!(h.values[1], "30.00", "the other field never moves");
    // The release leaves the field selected for typing.
    h.type_text("7");
    assert_eq!(h.values[0], "7");
}

#[test]
fn shift_drag_is_four_times_finer_and_modifier_changes_apply_mid_drag() {
    let mut h = Harness::new();
    press(&mut h, Point::new(20.0, 10.0));
    shift(&mut h, true);
    drag_to(&mut h, Point::new(20.0, 10.0 - 4.0 * step()));
    assert_eq!(h.values[0], "120.01");
    shift(&mut h, false);
    drag_to(&mut h, Point::new(20.0, 10.0 - 5.0 * step()));
    assert_eq!(h.values[0], "120.02");
    release(&mut h, Point::new(20.0, 10.0 - 5.0 * step()));
}

#[test]
fn a_wobbling_click_stays_a_click_and_selects_the_value_for_typing() {
    let mut h = Harness::new();
    let start = Point::new(20.0, 10.0);
    press(&mut h, start);
    let (messages, captured) = drag_to(&mut h, Point::new(21.0, 11.5));
    assert!(edits(messages).is_empty() && !captured);
    release(&mut h, start);
    h.type_text("93.5");
    assert_eq!(h.values[0], "93.5");
}

#[test]
fn drag_stops_at_the_limit_and_comes_straight_back_without_winding_up() {
    let focus = ControlFocus::default();
    let controls = Controls::new(LIVE_DARK, &focus);
    let build = |value: &str| {
        controls.number(
            Control::new(
                ControlId::field(View::Track, TestKey(0)),
                TestHelp::TargetNumber,
                Number::new(""),
            )
            .nudge(crate::control::Nudge::integer(0.0, 10.0)),
            value,
            |s| Message::Draft(0, s),
            Message::Submit(0),
            None,
        )
    };
    let mut tree = Tree::new(build("9"));
    let renderer = crate::grid::test_renderer();
    let mut run = |event: Event, point: Point, value: &str| {
        let mut element = build(value);
        tree.diff(&element);
        let node = element.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, Size::new(200.0, 100.0)),
        );
        let mut messages = vec![];
        let mut shell = Shell::new(&mut messages);
        element.as_widget_mut().update(
            &mut tree,
            &event,
            Layout::new(&node),
            mouse::Cursor::Available(point),
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut shell,
            &Rectangle::with_size(Size::new(200.0, 100.0)),
        );
        messages
    };
    let at = |steps: f32| Point::new(20.0, 10.0 - steps * step());
    run(
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        at(0.0),
        "9",
    );
    let moved = |p: Point| Event::Mouse(mouse::Event::CursorMoved { position: p });
    let up = run(moved(at(6.0)), at(6.0), "9");
    assert_eq!(
        edits(up),
        vec![Message::Draft(0, "10".into()), Message::Submit(0)]
    );
    // Six steps of pull past a one-step range, then one step back: it moves at once.
    let down = run(moved(at(5.0)), at(5.0), "10");
    assert_eq!(
        edits(down),
        vec![Message::Draft(0, "9".into()), Message::Submit(0)]
    );
}

#[test]
fn drag_never_moves_disabled_or_non_numeric_fields() {
    let mut h = Harness::new();
    h.enabled = false;
    press(&mut h, Point::new(20.0, 10.0));
    let (messages, _) = drag_to(&mut h, Point::new(20.0, 10.0 - 6.0 * step()));
    assert!(edits(messages).is_empty());
    h.enabled = true;
    h.values[0] = "-".into();
    press(&mut h, Point::new(20.0, 10.0));
    let (messages, _) = drag_to(&mut h, Point::new(20.0, 10.0 - 6.0 * step()));
    assert!(edits(messages).is_empty());
}

#[test]
fn double_click_on_a_number_with_a_declared_default_types_it_and_commits() {
    let clock = crate::gesture_clock::test::Clock::new();
    let focus = ControlFocus::default();
    let controls = Controls::new(LIVE_DARK, &focus);
    let build = || {
        controls.number(
            Control::new(
                ControlId::field(View::Track, TestKey(0)),
                TestHelp::TargetNumber,
                Number::new(""),
            )
            .default_to(0.5),
            "7",
            |s| Message::Draft(0, s),
            Message::Submit(0),
            None,
        )
    };
    let mut tree = Tree::new(build());
    let renderer = crate::grid::test_renderer();
    let mut press = || {
        let mut element = build();
        tree.diff(&element);
        let node = element.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, Size::new(200.0, 100.0)),
        );
        let point = Point::new(5.0, 5.0);
        let mut messages = vec![];
        let mut shell = Shell::new(&mut messages);
        element.as_widget_mut().update(
            &mut tree,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            Layout::new(&node),
            mouse::Cursor::Available(point),
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut shell,
            &Rectangle::with_size(Size::new(200.0, 100.0)),
        );
        edits(messages)
    };
    assert!(press().is_empty());
    clock.advance(std::time::Duration::from_millis(100));
    assert_eq!(
        press(),
        vec![Message::Draft(0, "0.50".into()), Message::Submit(0)]
    );
}
