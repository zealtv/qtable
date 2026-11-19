use super::*;
use crate::control::{Control, Controls, LIVE_DARK, SizeRole};
use crate::control_focus::test_registry::TestHelp;
use crate::control_focus::{ControlFocus, ControlId, ControlKey, View, test_registry::TestKey};
use iced::advanced::widget::{
    Operation, Tree,
    operation::{self, Outcome},
};
use iced::advanced::{Clipboard, Layout, Shell, layout};
use iced::keyboard::{self, Key, Modifiers, key::Named};
use iced::{Element, Event, Point, Rectangle, Renderer, Size, mouse};

#[derive(Clone, Debug, PartialEq)]
enum Message {
    Draft(String),
    Submit,
    Focus,
}
impl crate::control::widgets::ControlMessage<TestKey> for Message {
    fn info(_: crate::control::help::Hover<TestHelp>) -> Self {
        Self::Focus
    }
    fn focused(_: ControlId<TestKey>) -> Self {
        Self::Focus
    }
    fn commit_number() -> Self {
        Self::Submit
    }
    fn field_entered() -> Self {
        Self::Focus
    }
}
fn view(field: TimeField, value: &str, enabled: bool, key: TestKey) -> Element<'static, Message> {
    Controls::new(LIVE_DARK, &ControlFocus::default()).time_field(
        Control::new(
            ControlId::field(View::Clip, key),
            TestHelp::ClipLength,
            field,
        )
        .role(SizeRole::Fill)
        .enabled(enabled),
        value,
        Message::Draft,
        Message::Submit,
    )
}
struct ClipBoard(String);
impl Clipboard for ClipBoard {
    fn read(&self, _: iced::advanced::clipboard::Kind) -> Option<String> {
        Some(self.0.clone())
    }
    fn write(&mut self, _: iced::advanced::clipboard::Kind, contents: String) {
        self.0 = contents;
    }
}
struct Harness {
    field: TimeField,
    key: TestKey,
    value: String,
    stored: String,
    enabled: bool,
    tree: Tree,
    renderer: Renderer,
    clipboard: ClipBoard,
}
impl Harness {
    fn new(kind: TimeKind) -> Self {
        let field = TimeField {
            signature: TimeSignature::default(),
            kind,
        };
        let value = field.format(BeatTime(4 * TICKS_PER_BEAT + 2 * (TICKS_PER_BEAT / 4)));
        let tree = Tree::new(view(field, &value, true, TestKey(0)));
        Self {
            field,
            key: TestKey(0),
            stored: value.clone(),
            value,
            enabled: true,
            tree,
            renderer: crate::grid::test_renderer(),
            clipboard: ClipBoard(String::new()),
        }
    }
    fn element(&mut self) -> (Element<'static, Message>, layout::Node) {
        let mut e = view(self.field, &self.value, self.enabled, self.key);
        self.tree.diff(&e);
        let node = e.as_widget_mut().layout(
            &mut self.tree,
            &self.renderer,
            &layout::Limits::new(Size::ZERO, Size::new(140.0, 30.0)),
        );
        (e, node)
    }
    fn event(&mut self, event: Event, point: Point) -> (Vec<Message>, bool) {
        let (mut e, node) = self.element();
        let mut messages = Vec::new();
        let mut shell = Shell::new(&mut messages);
        e.as_widget_mut().update(
            &mut self.tree,
            &event,
            Layout::new(&node),
            mouse::Cursor::Available(point),
            &self.renderer,
            &mut self.clipboard,
            &mut shell,
            &Rectangle::with_size(Size::new(140.0, 30.0)),
        );
        let captured = shell.is_event_captured();
        for message in &messages {
            match message {
                Message::Draft(value) => self.value = value.clone(),
                Message::Submit => {
                    if let Ok(value) = self.field.parse(&self.value) {
                        self.value = self.field.format(value);
                        self.stored = self.value.clone();
                    }
                }
                _ => (),
            }
        }
        (messages, captured)
    }
    fn operate(&mut self, mut op: impl Operation) {
        let (mut e, node) = self.element();
        e.as_widget_mut()
            .operate(&mut self.tree, Layout::new(&node), &self.renderer, &mut op);
    }
    fn focus(&mut self) {
        self.operate(operation::focusable::focus(self.key.input_id()));
    }
    fn key(&mut self, key: Named) -> (Vec<Message>, bool) {
        self.event(
            key_event(Key::Named(key), None, Modifiers::NONE),
            Point::ORIGIN,
        )
    }
    fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            self.event(
                key_event(
                    Key::Character(c.to_string().into()),
                    Some(&c.to_string()),
                    Modifiers::NONE,
                ),
                Point::ORIGIN,
            );
        }
    }
    fn paste(&mut self, text: &str) {
        self.clipboard.0 = text.into();
        self.event(
            Event::Keyboard(keyboard::Event::ModifiersChanged(Modifiers::COMMAND)),
            Point::ORIGIN,
        );
        self.event(
            key_event(Key::Character("v".into()), None, Modifiers::COMMAND),
            Point::ORIGIN,
        );
    }
    fn segment(&mut self, segment: usize) {
        self.focus();
        for _ in 0..segment {
            self.key(Named::ArrowRight);
        }
    }
    fn point(&mut self, segment: usize) -> Point {
        let (_, node) = self.element();
        let bounds = Layout::new(&node).bounds();
        // Field and control-focus wrappers have no text layout of their own. Use measured value
        // typography, matching the numeric paragraph and right alignment.
        let face = crate::control::measure::Face {
            size: crate::style::detail::TEXT_SIZE.into(),
            font: crate::style::VALUE_FONT,
            shaping: iced::advanced::text::Shaping::Advanced,
        };
        let x = 140.0 - 4.0 - crate::control::measure::width(&self.value, face);
        let range = ranges(&self.value)[segment].clone();
        Point::new(
            x + crate::control::measure::width(&self.value[..range.start], face)
                + crate::control::measure::width(&self.value[range], face) / 2.0,
            bounds.center_y(),
        )
    }
}
fn key_event(key: Key, text: Option<&str>, modifiers: Modifiers) -> Event {
    Event::Keyboard(keyboard::Event::KeyPressed {
        modified_key: key.clone(),
        key,
        physical_key: keyboard::key::Physical::Unidentified(
            keyboard::key::NativeCode::Unidentified,
        ),
        location: keyboard::Location::Standard,
        modifiers,
        text: text.map(Into::into),
        repeat: false,
    })
}
const KINDS: [TimeKind; 2] = [TimeKind::Duration, TimeKind::Position];

#[test]
fn each_segment_replaces_only_its_digits_in_both_kinds_across_rebuilds() {
    for kind in KINDS {
        for segment in 0..3 {
            let mut h = Harness::new(kind);
            let before: Vec<String> = h.value.split('.').map(|s| s.trim().into()).collect();
            h.segment(segment);
            h.type_text("1");
            let after: Vec<_> = h.value.split('.').map(str::trim).collect();
            for i in 0..3 {
                assert_eq!(
                    after[i],
                    if i == segment { "1" } else { &before[i] },
                    "{kind:?}, {segment}"
                );
            }
            h.type_text("2");
            assert_eq!(h.value.split('.').nth(segment).unwrap().trim(), "12");
        }
    }
}

#[test]
fn paste_always_replaces_whole_text_and_supplies_missing_trailing_parts() {
    for kind in KINDS {
        for text in ["5", "5.2", "5.2.3"] {
            let mut h = Harness::new(kind);
            h.segment(2);
            h.paste(text);
            assert_eq!(h.value, text);
            h.key(Named::Enter);
            let expected = match (kind, text) {
                (TimeKind::Duration, "5") => "5. 0. 0",
                (TimeKind::Duration, "5.2") => "5. 2. 0",
                (TimeKind::Position, "5") => "5. 1. 1",
                (TimeKind::Position, "5.2") => "5. 2. 1",
                _ => "5. 2. 3",
            };
            assert_eq!(h.value, expected, "{kind:?}, {text}");
        }
    }
}

#[test]
fn dotted_typing_enters_a_whole_value_and_enter_rearms_segment_replacement() {
    for kind in KINDS {
        let mut h = Harness::new(kind);
        h.focus();
        h.type_text("5.2.3");
        assert_eq!(h.value, "5.2.3");
        h.key(Named::Enter);
        assert_eq!(h.value, "5. 2. 3");
        h.type_text("6");
        assert_eq!(h.value, "6. 2. 3");
    }
}

#[test]
fn select_all_allows_a_bare_whole_value_and_invalid_unicode_is_replaceable() {
    for kind in KINDS {
        let mut h = Harness::new(kind);
        h.segment(2);
        h.event(
            Event::Keyboard(keyboard::Event::ModifiersChanged(Modifiers::COMMAND)),
            Point::ORIGIN,
        );
        h.event(
            key_event(Key::Character("a".into()), None, Modifiers::COMMAND),
            Point::ORIGIN,
        );
        h.event(
            Event::Keyboard(keyboard::Event::ModifiersChanged(Modifiers::NONE)),
            Point::ORIGIN,
        );
        h.type_text("5");
        assert_eq!(h.value, "5");
        h.key(Named::Enter);
        assert_eq!(
            h.value,
            match kind {
                TimeKind::Duration => "5. 0. 0",
                TimeKind::Position => "5. 1. 1",
            }
        );
        h.value = "5. 😺. 1".into();
        h.key(Named::ArrowLeft);
        h.type_text("2");
        assert_eq!(h.value, "5. 2. 1");
    }
}

#[test]
fn focused_segment_is_lighter_without_a_caret_and_moves_with_keyboard_focus() {
    use iced::advanced::{
        Renderer as _,
        renderer::{Headless, Style},
    };
    for kind in KINDS {
        for long in [false, true] {
            let mut h = Harness::new(kind);
            if long {
                h.value = "123456789012345. 3. 3+1/3".into();
            }
            h.focus();
            let mut images = Vec::new();
            for _ in 0..3 {
                let (element, node) = h.element();
                let bounds = Rectangle::with_size(Size::new(140.0, 30.0));
                h.renderer.reset(bounds);
                element.as_widget().draw(
                    &h.tree,
                    &mut h.renderer,
                    &iced::Theme::Dark,
                    &Style::default(),
                    Layout::new(&node),
                    mouse::Cursor::Unavailable,
                    &bounds,
                );
                let pixels = h
                    .renderer
                    .screenshot(Size::new(140, 30), 1.0, iced::Color::BLACK);
                assert!(
                    pixels
                        .chunks_exact(4)
                        .any(|p| p == LIVE_DARK.surface_highlight.into_rgba8())
                );
                assert!(
                    !pixels
                        .chunks_exact(4)
                        .any(|p| p == LIVE_DARK.numeric_cursor.into_rgba8()),
                    "selection has no typing caret"
                );
                images.push(pixels);
                h.key(Named::ArrowRight);
            }
            assert_ne!(images[0], images[1]);
            assert_ne!(images[1], images[2]);
        }
    }
}

#[test]
fn each_segment_steps_with_carry_borrow_shift_invariance_and_lower_clamp() {
    for kind in KINDS {
        for segment in 0..3 {
            let mut h = Harness::new(kind);
            h.value = h
                .field
                .format(BeatTime(15 * TICKS_PER_BEAT + 3 * TICKS_PER_BEAT / 4));
            let initial = h.value.clone();
            h.segment(segment);
            let expected = h.field.step(&initial, segment, 1).unwrap();
            let (messages, captured) = h.key(Named::ArrowUp);
            assert!(captured);
            assert!(messages.contains(&Message::Submit));
            assert_eq!(h.value, expected);
            h.key(Named::ArrowDown);
            assert_eq!(h.value, initial);
            h.event(
                key_event(Key::Named(Named::ArrowUp), None, Modifiers::SHIFT),
                Point::ORIGIN,
            );
            assert_eq!(h.value, expected);
            let floor = match kind {
                TimeKind::Duration => BeatTime(1),
                TimeKind::Position => BeatTime::ZERO,
            };
            h.value = h.field.format(floor);
            let (messages, captured) = h.key(Named::ArrowDown);
            assert!(captured);
            assert!(messages.is_empty());
            assert_eq!(h.field.parse(&h.value), Ok(floor));
        }
    }
    assert_eq!(
        TimeField::position(TimeSignature::default())
            .step("1. 4. 4", 2, 1)
            .as_deref(),
        Some("2. 1. 1")
    );
    assert_eq!(
        TimeField::duration(TimeSignature::default())
            .step("2. 0. 0", 1, -1)
            .as_deref(),
        Some("1. 3. 0")
    );
}

#[test]
fn left_right_stop_at_the_ends_and_tab_is_one_control() {
    for kind in KINDS {
        let mut h = Harness::new(kind);
        h.focus();
        assert!(h.key(Named::ArrowLeft).1);
        h.type_text("3");
        assert!(h.value.starts_with("3."));
        h.key(Named::ArrowRight);
        h.type_text("2");
        h.key(Named::ArrowRight);
        h.key(Named::ArrowRight);
        h.type_text("1");
        assert_eq!(h.value, "3. 2. 1");
        assert!(!h.key(Named::Tab).1);
        let mut count = operation::focusable::count();
        h.operate(operation::black_box(&mut count));
        assert!(matches!(count.finish(), Outcome::Some(count) if count.total == 1));
    }
}

#[test]
fn click_and_vertical_drag_use_the_clicked_segment_and_preserve_relative_ticks() {
    for kind in KINDS {
        for segment in 0..3 {
            let mut h = Harness::new(kind);
            let initial = h.value.clone();
            let p = h.point(segment);
            let (messages, _) = h.event(
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                p,
            );
            assert!(
                !messages
                    .iter()
                    .any(|m| matches!(m, Message::Draft(_) | Message::Submit))
            );
            let end = Point::new(p.x + 20.0, p.y - 2.0 * crate::style::detail::TIME_DRAG_STEP);
            h.event(
                Event::Mouse(mouse::Event::CursorMoved { position: end }),
                end,
            );
            assert_eq!(
                h.value,
                h.field.step(&initial, segment, 2).unwrap(),
                "{kind:?}, {segment}"
            );
            h.event(Event::Mouse(mouse::Event::CursorMoved { position: p }), p);
            assert_eq!(h.value, initial);
            h.event(
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                p,
            );
            h.type_text("1");
            assert_eq!(h.value.split('.').nth(segment).unwrap().trim(), "1");
        }
    }
}

#[test]
fn active_drafts_wait_for_enter_and_escape_keeps_the_segment_ready() {
    for kind in KINDS {
        let mut h = Harness::new(kind);
        h.segment(1);
        h.type_text("bad");
        let draft = h.value.clone();
        assert!(h.key(Named::ArrowUp).0.is_empty());
        assert_eq!(h.value, draft);
        assert!(h.key(Named::Escape).1);
        h.value = h.stored.clone();
        h.type_text("2");
        assert_eq!(h.value.split('.').nth(1).unwrap().trim(), "2");
        h.enabled = false;
        let old = h.value.clone();
        h.type_text("9");
        assert_eq!(h.value, old);
    }
}

#[test]
fn non_four_four_steps_and_exact_tick_fractions_round_trip() {
    for kind in KINDS {
        for signature in [
            TimeSignature::new(7, 8).unwrap(),
            TimeSignature::new(3, 2).unwrap(),
            TimeSignature::new(5, 16).unwrap(),
            TimeSignature::new(5, 32).unwrap(),
        ] {
            let field = TimeField { signature, kind };
            let initial = field.format(BeatTime(321));
            for segment in 0..3 {
                let up = field.step(&initial, segment, 1).unwrap();
                let bar = signature.bar_length().ticks();
                let beat = bar / u64::from(signature.numerator());
                let step = [bar, beat, (TICKS_PER_BEAT / 4).min(beat)][segment];
                assert_eq!(field.parse(&up).unwrap().ticks(), 321 + step);
                assert_eq!(field.step(&up, segment, -1).unwrap(), initial);
            }
        }
    }
}

#[test]
fn changing_the_field_owner_discards_an_unreleased_segment_drag() {
    for kind in KINDS {
        let mut h = Harness::new(kind);
        let point = h.point(1);
        h.event(
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            point,
        );
        h.key = TestKey(1);
        let end = Point::new(point.x, point.y - crate::style::detail::TIME_DRAG_STEP);
        let (messages, _) = h.event(
            Event::Mouse(mouse::Event::CursorMoved { position: end }),
            end,
        );
        assert!(
            !messages
                .iter()
                .any(|m| matches!(m, Message::Draft(_) | Message::Submit))
        );
    }
}

#[test]
fn editing_keys_cannot_remove_a_neighbouring_segment_or_separator() {
    for kind in KINDS {
        for segment in 0..3 {
            let mut h = Harness::new(kind);
            h.segment(segment);
            h.type_text("12");
            let before = h.value.clone();
            h.key(Named::Home);
            h.key(Named::Backspace);
            assert_eq!(h.value, before);
            h.key(Named::End);
            h.key(Named::Delete);
            assert_eq!(h.value, before);
            h.event(
                Event::Keyboard(keyboard::Event::ModifiersChanged(Modifiers::COMMAND)),
                Point::ORIGIN,
            );
            h.event(
                key_event(Key::Named(Named::Backspace), None, Modifiers::COMMAND),
                Point::ORIGIN,
            );
            h.event(
                Event::Keyboard(keyboard::Event::ModifiersChanged(Modifiers::NONE)),
                Point::ORIGIN,
            );
            h.type_text("3");
            let before: Vec<_> = before.split('.').map(str::trim).collect();
            let after: Vec<_> = h.value.split('.').map(str::trim).collect();
            for i in 0..3 {
                assert_eq!(after[i], if i == segment { "3" } else { before[i] });
            }
        }
    }
}

#[test]
fn duration_nudges_and_drags_past_zero_publish_nothing() {
    let mut h = Harness::new(TimeKind::Duration);
    h.value = "1. 0. 0".into();
    h.stored = h.value.clone();
    h.focus();
    for _ in 0..2 {
        let (messages, captured) = h.key(Named::ArrowDown);
        assert!(captured);
        assert!(
            !messages
                .iter()
                .any(|m| matches!(m, Message::Draft(_) | Message::Submit))
        );
        assert_eq!(h.value, "1. 0. 0");
    }
    let point = h.point(0);
    h.event(
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        point,
    );
    for steps in [1.0, 2.0, 3.0, 0.0] {
        let end = Point::new(
            point.x,
            point.y + steps * crate::style::detail::TIME_DRAG_STEP,
        );
        let (messages, _) = h.event(
            Event::Mouse(mouse::Event::CursorMoved { position: end }),
            end,
        );
        assert!(
            !messages
                .iter()
                .any(|m| matches!(m, Message::Draft(_) | Message::Submit))
        );
        assert_eq!(h.value, "1. 0. 0");
    }
}

#[test]
fn blank_components_default_for_paste_dotted_typing_and_segment_deletion() {
    for kind in KINDS {
        let field = TimeField {
            signature: TimeSignature::default(),
            kind,
        };
        for (input, duration, position) in [
            ("5.", "5. 0. 0", "5. 1. 1"),
            ("5.2.", "5. 2. 0", "5. 2. 1"),
            (". . ", "0. 0. 0", "1. 1. 1"),
        ] {
            let expected = if kind == TimeKind::Duration {
                duration
            } else {
                position
            };
            let mut h = Harness::new(kind);
            h.focus();
            h.paste(input);
            h.key(Named::Enter);
            assert_eq!(h.value, expected);
            assert_eq!(field.format(field.parse(input).unwrap()), expected);
        }
        let mut h = Harness::new(kind);
        h.focus();
        h.type_text("5.");
        h.key(Named::Enter);
        assert_eq!(
            h.value,
            if kind == TimeKind::Duration {
                "5. 0. 0"
            } else {
                "5. 1. 1"
            }
        );
        h.key(Named::ArrowRight);
        h.key(Named::Backspace);
        h.key(Named::Enter);
        assert_eq!(
            h.value,
            if kind == TimeKind::Duration {
                "5. 0. 0"
            } else {
                "5. 1. 1"
            }
        );
    }
    let sig = TimeSignature::default();
    assert_eq!(parse_duration("1. . 0", sig), Ok(BeatTime(3840)));
    assert_eq!(parse_position("1. . ", sig), Ok(BeatTime::ZERO));
    assert!(
        parse_position("1. . 0", sig).is_err(),
        "an explicit position zero is not a blank"
    );
}

#[test]
fn navigation_does_not_make_a_pending_draft_eligible_for_nudging() {
    for kind in KINDS {
        for direction in [Named::ArrowLeft, Named::ArrowRight] {
            let mut h = Harness::new(kind);
            h.segment(1);
            h.type_text("2");
            h.key(direction);
            let draft = h.value.clone();
            for arrow in [Named::ArrowUp, Named::ArrowDown] {
                let (messages, captured) = h.key(arrow);
                assert!(captured);
                assert!(messages.is_empty());
                assert_eq!(h.value, draft);
            }
            h.type_text("3");
            let draft = h.value.clone();
            h.key(Named::Enter);
            let (messages, _) = h.key(Named::ArrowUp);
            assert!(messages.contains(&Message::Submit));
            assert_ne!(h.value, draft);
        }
    }
}

#[test]
fn sixteenth_grammar_excludes_signed_decimal_and_scientific_forms() {
    for kind in KINDS {
        let field = TimeField {
            signature: TimeSignature::default(),
            kind,
        };
        for last in ["1e0", "+1", "-1", ".5", "1+", "1+2", "1/2/3", "1+1/3+1/3"] {
            assert!(
                field.parse(&format!("1.1.{last}")).is_err(),
                "{kind:?}: {last}"
            );
        }
        for last in ["1", "3/2", "1+1/3", "1 + 1 / 3"] {
            assert!(
                field.parse(&format!("1.1.{last}")).is_ok(),
                "{kind:?}: {last}"
            );
        }
    }
}

#[test]
fn touch_drag_matches_mouse_segments_and_ignores_other_fingers() {
    use iced::touch::{Event as T, Finger};
    for kind in KINDS {
        for segment in 0..3 {
            let mut h = Harness::new(kind);
            let initial = h.value.clone();
            let point = h.point(segment);
            h.event(
                Event::Touch(T::FingerPressed {
                    id: Finger(1),
                    position: point,
                }),
                Point::ORIGIN,
            );
            let end = Point::new(
                point.x,
                point.y - 2.0 * crate::style::detail::TIME_DRAG_STEP,
            );
            h.event(
                Event::Touch(T::FingerPressed {
                    id: Finger(2),
                    position: point,
                }),
                Point::ORIGIN,
            );
            let (messages, _) = h.event(
                Event::Touch(T::FingerMoved {
                    id: Finger(2),
                    position: end,
                }),
                Point::ORIGIN,
            );
            assert!(
                !messages
                    .iter()
                    .any(|m| matches!(m, Message::Draft(_) | Message::Submit))
            );
            h.event(
                Event::Touch(T::FingerLifted {
                    id: Finger(2),
                    position: end,
                }),
                Point::ORIGIN,
            );
            h.event(
                Event::Touch(T::FingerMoved {
                    id: Finger(1),
                    position: end,
                }),
                Point::ORIGIN,
            );
            assert_eq!(h.value, h.field.step(&initial, segment, 2).unwrap());
            let released = if segment % 2 == 0 {
                T::FingerLost {
                    id: Finger(1),
                    position: end,
                }
            } else {
                T::FingerLifted {
                    id: Finger(1),
                    position: end,
                }
            };
            h.event(Event::Touch(released), Point::ORIGIN);
            let (messages, _) = h.event(
                Event::Touch(T::FingerMoved {
                    id: Finger(1),
                    position: point,
                }),
                Point::ORIGIN,
            );
            assert!(
                !messages
                    .iter()
                    .any(|m| matches!(m, Message::Draft(_) | Message::Submit))
            );
        }
    }
}
