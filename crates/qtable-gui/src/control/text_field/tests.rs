//! The field withholds typed text from Cmd-modified presses and nothing else, checked through
//! real widgets: a click focuses the field, then key presses go to it as the window sends them.
use super::*;
use crate::control::{Control, Controls, LIVE_DARK, Number, Text};
use crate::control_focus::test_registry::{TestHelp, TestKey, TestMessage};
use crate::control_focus::{ControlFocus, ControlId, View};
use iced::keyboard::{Key, Location, Modifiers, key::Named};
use iced::{Point, Size};

fn press(character: &str, modifiers: Modifiers) -> Event {
    let key = Key::Character(character.into());
    Event::Keyboard(keyboard::Event::KeyPressed {
        modified_key: key.clone(),
        physical_key: keyboard::key::Physical::Unidentified(
            keyboard::key::NativeCode::Unidentified,
        ),
        key,
        location: Location::Standard,
        modifiers,
        // What the OS reports for Cmd-S on a Mac: the character, whatever the modifier.
        text: Some(character.into()),
        repeat: false,
    })
}

/// What a press did to a focused field: the messages it published and whether it took the key.
#[derive(Debug, PartialEq)]
struct Outcome {
    edits: usize,
    captured: bool,
}

/// Clicks the field to focus it, then sends `events` one at a time, counting the messages
/// each publishes on top of the click's. `is_edit` picks the field's own edit message.
fn press_in<M: Clone>(
    mut element: iced::Element<'_, M>,
    is_edit: impl Fn(&M) -> bool,
    modifiers: Modifiers,
    events: &[Event],
) -> Vec<Outcome> {
    let renderer = crate::grid::test_renderer();
    let mut tree = Tree::new(&element);
    let size = Size::new(200.0, 40.0);
    let node = element.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, size),
    );
    let viewport = Rectangle::with_size(size);
    let cursor = mouse::Cursor::Available(Point::new(10.0, 10.0));
    let mut send = |tree: &mut Tree, event: &Event| {
        let mut messages = Vec::new();
        let mut shell = Shell::new(&mut messages);
        element.as_widget_mut().update(
            tree,
            event,
            Layout::new(&node),
            cursor,
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut shell,
            &viewport,
        );
        let captured = shell.is_event_captured();
        drop(shell);
        (messages, captured)
    };
    send(
        &mut tree,
        &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
    );
    // iced's text_input learns the modifiers from their own event.
    send(
        &mut tree,
        &Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)),
    );
    events
        .iter()
        .map(|event| {
            let (messages, captured) = send(&mut tree, event);
            Outcome {
                edits: messages.iter().filter(|m| is_edit(m)).count(),
                captured,
            }
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
enum Msg {
    Edited(String),
}

fn raw_field<'a>() -> iced::Element<'a, Msg> {
    TextField::new("", "93")
        .on_input(Msg::Edited)
        .width(150)
        .into()
}

fn edited(message: &Msg) -> bool {
    matches!(message, Msg::Edited(_))
}

#[test]
fn a_command_key_never_types_its_character_and_the_window_still_sees_it() {
    for (character, modifiers) in [
        ("s", Modifiers::COMMAND),
        ("S", Modifiers::COMMAND | Modifiers::SHIFT),
        (",", Modifiers::COMMAND),
        ("z", Modifiers::COMMAND),
    ] {
        let outcome = press_in(
            raw_field(),
            edited,
            modifiers,
            &[press(character, modifiers)],
        );
        assert_eq!(
            outcome,
            [Outcome {
                edits: 0,
                captured: false
            }],
            "{modifiers:?} {character}: nothing typed, and the key goes on to the window's shortcuts"
        );
    }
}

#[test]
fn plain_and_shifted_keys_still_type() {
    for (character, modifiers) in [
        ("s", Modifiers::NONE),
        ("S", Modifiers::SHIFT),
        (",", Modifiers::NONE),
    ] {
        let outcome = press_in(
            raw_field(),
            edited,
            modifiers,
            &[press(character, modifiers)],
        );
        assert_eq!(
            outcome,
            [Outcome {
                edits: 1,
                captured: true
            }],
            "{character}"
        );
    }
}

#[test]
fn alt_with_the_command_modifier_still_types() {
    // Windows reports AltGr as Ctrl+Alt and types characters with it; iced's own Cmd-V
    // check makes the same exception.
    let modifiers = Modifiers::COMMAND | Modifiers::ALT;
    let outcome = press_in(raw_field(), edited, modifiers, &[press("@", modifiers)]);
    assert_eq!(outcome[0].edits, 1);
}

#[test]
fn the_fields_own_command_keys_keep_working() {
    // Cmd-A selects all and takes the key, as iced has it; Cmd-C takes it and types nothing.
    for character in ["a", "c"] {
        let outcome = press_in(
            raw_field(),
            edited,
            Modifiers::COMMAND,
            &[press(character, Modifiers::COMMAND)],
        );
        assert_eq!(
            outcome,
            [Outcome {
                edits: 0,
                captured: true
            }],
            "Cmd-{character}"
        );
    }
    // Cmd-V reaches the input too (the test clipboard is empty, so it types nothing).
    let outcome = press_in(
        raw_field(),
        edited,
        Modifiers::COMMAND,
        &[press("v", Modifiers::COMMAND)],
    );
    assert!(outcome[0].captured, "Cmd-V is the field's");
}

#[test]
fn backspace_and_enter_are_untouched() {
    let named = |key| {
        Event::Keyboard(keyboard::Event::KeyPressed {
            modified_key: Key::Named(key),
            key: Key::Named(key),
            physical_key: keyboard::key::Physical::Unidentified(
                keyboard::key::NativeCode::Unidentified,
            ),
            location: Location::Standard,
            modifiers: Modifiers::NONE,
            text: None,
            repeat: false,
        })
    };
    let outcome = press_in(
        raw_field(),
        edited,
        Modifiers::NONE,
        &[named(Named::Backspace)],
    );
    assert_eq!(outcome[0].edits, 1);
}

/// The Controls' Number and Text fields, built the way the app builds them, hold the same rule.
#[test]
fn the_number_and_text_fields_of_the_control_layer_withhold_command_text() {
    let focus = ControlFocus::<TestKey>::default();
    let controls = Controls::new(LIVE_DARK, &focus);
    let id = ControlId::field(View::Track, TestKey(1));
    let is_edit = |m: &TestMessage| matches!(m, TestMessage::Focused(_));
    let build = move |number: bool| -> iced::Element<'_, TestMessage> {
        if number {
            controls.number(
                Control::new(id, TestHelp::TargetNumber, Number::new("")),
                "93",
                move |_| TestMessage::Focused(id),
                TestMessage::Focused(id),
                None,
            )
        } else {
            controls.text(
                Control::new(
                    id,
                    TestHelp::TargetText,
                    Text {
                        placeholder: "".into(),
                    },
                ),
                "a",
                move |_| TestMessage::Focused(id),
                TestMessage::Focused(id),
            )
        }
    };
    for (name, number) in [("number", true), ("text", false)] {
        for modifiers in [Modifiers::COMMAND, Modifiers::COMMAND | Modifiers::SHIFT] {
            let cmd = press_in(build(number), is_edit, modifiers, &[press("s", modifiers)]);
            assert_eq!(cmd[0].edits, 0, "{name}: Cmd-S types nothing");
            assert!(!cmd[0].captured, "{name}: Cmd-S reaches the window");
        }
        let plain = press_in(
            build(number),
            is_edit,
            Modifiers::NONE,
            &[press("s", Modifiers::NONE)],
        );
        assert!(
            plain[0].captured,
            "{name}: a plain s still goes to the field"
        );
    }
}

fn enter_press() -> Event {
    Event::Keyboard(keyboard::Event::KeyPressed {
        modified_key: Key::Named(Named::Enter),
        key: Key::Named(Named::Enter),
        physical_key: keyboard::key::Physical::Unidentified(
            keyboard::key::NativeCode::Unidentified,
        ),
        location: Location::Standard,
        modifiers: Modifiers::NONE,
        text: None,
        repeat: false,
    })
}

/// Enter in a focused field publishes its submit message and then `on_enter`, in that order
/// (the app reads how the submit went when `on_enter` arrives); other keys publish neither.
#[test]
fn enter_publishes_the_submit_and_then_the_entered_marker() {
    #[derive(Clone, Debug, PartialEq)]
    enum Sent {
        Submit,
        Entered,
    }
    let field = |submit: bool| -> iced::Element<'static, Sent> {
        let mut field = TextField::new("", "93")
            .on_input(|_| Sent::Submit)
            .on_enter(Sent::Entered);
        if submit {
            field = field.on_submit(Sent::Submit);
        }
        field.width(150).into()
    };
    let sent = |element, event: Event| {
        // `press_in` counts the messages that `is_edit` picks; read them in order instead.
        let order = std::cell::RefCell::new(Vec::new());
        press_in(
            element,
            |m: &Sent| {
                order.borrow_mut().push(m.clone());
                true
            },
            Modifiers::NONE,
            &[event],
        );
        order.into_inner()
    };
    assert_eq!(
        sent(field(true), enter_press()),
        [Sent::Submit, Sent::Entered]
    );
    assert_eq!(
        sent(field(false), enter_press()),
        [],
        "no submit to follow: no marker"
    );
    assert_eq!(
        sent(field(true), press("s", Modifiers::NONE)),
        [Sent::Submit],
        "a typed key edits and nothing more"
    );
}

/// The control layer's fields all send the marker on Enter.
#[test]
fn the_control_layers_fields_send_the_entered_marker() {
    let focus = ControlFocus::<TestKey>::default();
    let controls = Controls::new(LIVE_DARK, &focus);
    let id = ControlId::field(View::Track, TestKey(1));
    let entered = |m: &TestMessage| matches!(m, TestMessage::Tick);
    let number = controls.number(
        Control::new(id, TestHelp::TargetNumber, Number::new("")),
        "93",
        move |_| TestMessage::Focused(id),
        TestMessage::Commit,
        None,
    );
    let text = controls.text(
        Control::new(
            id,
            TestHelp::TargetText,
            Text {
                placeholder: "".into(),
            },
        ),
        "a",
        move |_| TestMessage::Focused(id),
        TestMessage::Commit,
    );
    for (name, element) in [("number", number), ("text", text)] {
        let outcome = press_in(element, entered, Modifiers::NONE, &[enter_press()]);
        assert_eq!(outcome[0].edits, 1, "{name}: Enter sends the marker once");
    }
}
