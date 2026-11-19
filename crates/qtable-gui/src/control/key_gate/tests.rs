use super::*;
use iced::keyboard::{Event as KeyEvent, key::Named};
use iced::widget::{column, text_input};
use iced::{Point, Size};

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Key(Key, bool, bool),
    Edit(String),
    Submit,
}

fn key_msg(input: KeyInput) -> Msg {
    Msg::Key(input.key, input.pressed, input.repeat)
}

fn press(key: Key, modifiers: Modifiers, repeat: bool) -> Event {
    Event::Keyboard(KeyEvent::KeyPressed {
        modified_key: key.clone(),
        physical_key: iced::keyboard::key::Physical::Unidentified(
            iced::keyboard::key::NativeCode::Unidentified,
        ),
        key,
        location: iced::keyboard::Location::Standard,
        modifiers,
        repeat,
        text: None,
    })
}

#[test]
fn actual_focused_untouched_field_keeps_enter_and_command_keys() {
    let renderer = crate::grid::test_renderer();
    let content = column![
        text_input("", "unchanged")
            .on_input(Msg::Edit)
            .on_submit(Msg::Submit)
            .width(150)
    ];
    let mut element = key_gate(content, |_, _, _, _| true, key_msg);
    let mut tree = Tree::new(&element);
    let size = Size::new(200.0, 100.0);
    let node = element.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, size),
    );
    let viewport = Rectangle::with_size(size);
    let at = Point::new(10.0, 10.0);
    let mut messages = Vec::new();
    for event in [
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        press(Key::Named(Named::Enter), Modifiers::NONE, false),
        press(Key::Character("a".into()), Modifiers::COMMAND, false),
    ] {
        element.as_widget_mut().update(
            &mut tree,
            &event,
            Layout::new(&node),
            mouse::Cursor::Available(at),
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut Shell::new(&mut messages),
            &viewport,
        );
    }
    assert!(messages.contains(&Msg::Submit));
    assert!(!messages.iter().any(|msg| matches!(msg, Msg::Key(..))));
}

#[test]
fn a_captured_key_runs_before_a_latched_button_and_repeat_is_forwarded() {
    use crate::control_focus::{
        ControlFocus, ControlId, ControlOwner, View, test_registry::TestKey,
    };
    let id = ControlId::<TestKey>::action(View::ControlBar, "button", ControlOwner::Song);
    let mut focus = ControlFocus::default();
    focus.claim(id);
    let button = focus.wrap_enter(
        id,
        true,
        false,
        iced::widget::text("button"),
        |_| Msg::Submit,
        Msg::Submit,
    );
    let mut element = key_gate(button, |_, _, _, _| true, key_msg);
    let renderer = crate::grid::test_renderer();
    let mut tree = Tree::new(&element);
    let size = Size::new(200.0, 100.0);
    let node = element.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, size),
    );
    let mut messages = Vec::new();
    for repeat in [false, true] {
        let mut shell = Shell::new(&mut messages);
        element.as_widget_mut().update(
            &mut tree,
            &press(Key::Named(Named::Enter), Modifiers::NONE, repeat),
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut shell,
            &Rectangle::with_size(size),
        );
        assert!(shell.is_event_captured());
    }
    assert_eq!(
        messages,
        vec![
            Msg::Key(Key::Named(Named::Enter), true, false),
            Msg::Key(Key::Named(Named::Enter), true, true)
        ]
    );
}
