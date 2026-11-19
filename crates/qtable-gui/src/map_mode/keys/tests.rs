use super::*;
use crate::map_mode::{
    Action,
    tests::{act, app, slot, volume},
};
use crate::messages::Message;
use iced::keyboard::key::Named;
use qtable_engine::{
    edit::{MappingEdit, ProjectEdit},
    engine::Command,
    model::*,
};

fn key(value: &str) -> Key {
    Key::Character(value.into())
}
/// A press or release of the physical key `base`, which types `typed` under `modifiers`.
fn send_typed(
    app: &mut App,
    base: &str,
    typed: &str,
    modifiers: Modifiers,
    pressed: bool,
    repeat: bool,
) {
    let _ = crate::update::update(
        app,
        Message::MappedKey(crate::control::key_gate::KeyInput {
            key: key(base),
            modified_key: key(typed),
            modifiers,
            pressed,
            repeat,
        }),
    );
}
/// A letter or digit as typed: a capital arrives with Shift held and its lowercase base key.
fn send(app: &mut App, value: &str, pressed: bool, repeat: bool) {
    let base = value.to_lowercase();
    let modifiers = if base == value {
        Modifiers::NONE
    } else {
        Modifiers::SHIFT
    };
    send_typed(app, &base, value, modifiers, pressed, repeat);
}
fn bind(app: &mut App, name: &str, control: ControlRef) {
    act(app, Action::ToggleKey);
    act(app, Action::Pick(control));
    send(app, name, true, false);
    send(app, name, false, false);
    act(app, Action::ToggleKey);
}

#[test]
fn key_mode_is_exclusive_and_learns_only_keys_with_no_takeover() {
    let mut app = app();
    act(&mut app, Action::Toggle);
    act(&mut app, Action::ToggleKey);
    assert!(app.map_mode.on);
    assert_eq!(app.map_mode.mode, Mode::Key);
    act(&mut app, Action::Pick(volume()));
    crate::map_mode::tests::send(&mut app, &[0xb0, 21, 127]);
    assert!(app.project().mappings.is_empty());
    send(&mut app, "A", true, false);
    let row = &app.project().mappings[0];
    assert_eq!(
        row.source,
        Source::Key(KeySource {
            key: KeyName("A".into())
        })
    );
    assert_eq!(row.control, volume());
    assert_eq!(row.takeover, Takeover::None);
    let key_ink = crate::map_mode::ink(&app);
    assert_eq!(key_ink, app.theme.learn_key);
    act(&mut app, Action::Toggle);
    assert_eq!(app.map_mode.mode, Mode::Midi);
    assert!(crate::map_mode::marks(&app).is_empty());
    assert_ne!(crate::map_mode::ink(&app), key_ink);
    act(&mut app, Action::Pick(slot()));
    send(&mut app, "x", true, false);
    assert_eq!(app.project().mappings.len(), 1);
}

#[test]
fn relearning_a_key_replaces_its_row_and_preserves_the_midi_binding() {
    let mut app = app();
    act(&mut app, Action::Toggle);
    act(&mut app, Action::Pick(volume()));
    crate::map_mode::tests::send(&mut app, &[0xb0, 21, 127]);
    let midi = app.project().mappings[0].clone();
    act(&mut app, Action::Toggle);
    bind(&mut app, "a", volume());
    bind(&mut app, "a", slot());
    bind(&mut app, "b", volume());
    assert_eq!(app.project().mappings.len(), 3);
    assert_eq!(app.project().mappings[0], midi);
    let replacement = app.project().mappings.last().unwrap();
    assert_eq!(
        replacement.source,
        Source::Key(KeySource {
            key: KeyName("b".into())
        })
    );
    assert_eq!(replacement.control, volume());
    assert_eq!(replacement.takeover, Takeover::None);
    assert!(
        app.project()
            .mappings
            .iter()
            .any(|row| row.control == slot()
                && row.source
                    == Source::Key(KeySource {
                        key: KeyName("a".into())
                    }))
    );
    // Reopen Key Map Mode: picking the target selects its new row.
    let id = replacement.id;
    act(&mut app, Action::ToggleKey);
    act(&mut app, Action::Pick(volume()));
    assert_eq!(app.map_mode.selected_row, Some(id));
}

#[test]
fn learned_keys_launch_a_slot_and_toggle_once_per_press_with_fanout() {
    let mut app = app();
    bind(&mut app, "a", slot());
    bind(&mut app, "a", ControlRef::TrackSolo(TrackId(1)));
    send(&mut app, "a", true, false);
    assert!(app.project().tracks[0].mixer.solo);
    std::thread::sleep(std::time::Duration::from_millis(15));
    app.snapshot = app.engine.snapshot();
    assert!(
        app.snapshot.tracks[0].playing_slot.is_some() || app.snapshot.tracks[0].triggered.is_some()
    );
    send(&mut app, "a", true, true);
    send(&mut app, "a", true, false);
    assert!(app.project().tracks[0].mixer.solo);
    send(&mut app, "a", false, false);
    send(&mut app, "a", true, false);
    assert!(!app.project().tracks[0].mixer.solo);
    release_all(&mut app);
    send(&mut app, "a", true, false);
    assert!(app.project().tracks[0].mixer.solo);
}

fn map_key(app: &mut App, text: &str, control: ControlRef) {
    crate::edit_actions::run_edit(
        app,
        Command::Edit(ProjectEdit::EditMapping(MappingEdit::Add {
            source: Source::Key(KeySource {
                key: KeyName(text.into()),
            }),
            control,
            min: 0.0,
            max: 1.0,
        })),
    );
}

fn typed_name(key: &Key, modified: &Key, modifiers: Modifiers) -> Option<String> {
    typed(key, modified, modifiers).map(|name| name.0)
}

#[test]
fn typed_characters_are_the_sources_with_case_and_shifted_symbols_kept() {
    let n = |base: &str, typed: &str, modifiers| typed_name(&key(base), &key(typed), modifiers);
    assert_eq!(n("a", "a", Modifiers::NONE).as_deref(), Some("a"));
    assert_eq!(n("a", "A", Modifiers::SHIFT).as_deref(), Some("A"));
    assert_eq!(n("1", "1", Modifiers::NONE).as_deref(), Some("1"));
    assert_eq!(n("1", "!", Modifiers::SHIFT).as_deref(), Some("!"));
    for (base, shifted) in [("/", "?"), ("`", "~"), (",", "<"), (".", ">"), ("\\", "|")] {
        assert_eq!(n(base, base, Modifiers::NONE).as_deref(), Some(base));
        assert_eq!(n(base, shifted, Modifiers::SHIFT).as_deref(), Some(shifted));
    }
    for text in [
        "[", "]", "{", "}", ";", "'", "-", "=", "@", "#", "(", "£", "é",
    ] {
        assert_eq!(n(text, text, Modifiers::NONE).as_deref(), Some(text));
    }
    // Caps Lock types a capital without Shift; the typed character is what maps.
    assert_eq!(n("a", "A", Modifiers::NONE).as_deref(), Some("A"));
}

#[test]
fn named_keys_and_modifier_keys_are_never_sources() {
    for named in [
        Named::Space,
        Named::ArrowUp,
        Named::ArrowDown,
        Named::ArrowLeft,
        Named::ArrowRight,
        Named::Enter,
        Named::Delete,
        Named::Backspace,
        Named::Escape,
        Named::Tab,
        Named::F1,
        Named::Home,
        Named::Shift,
        Named::Control,
        Named::Alt,
        Named::Super,
        Named::CapsLock,
    ] {
        let key = Key::Named(named);
        assert_eq!(typed(&key, &key, Modifiers::NONE), None, "{named:?}");
        assert_eq!(typed(&key, &key, Modifiers::SHIFT), None, "{named:?}");
    }
    // A space typed as a character is still Space, and does not map.
    assert_eq!(typed(&key(" "), &key(" "), Modifiers::NONE), None);
    assert_eq!(typed(&key("\t"), &key("\t"), Modifiers::NONE), None);
}

#[test]
fn keys_held_with_command_option_or_control_are_never_captured_or_typed() {
    // (physical key, typed character, modifiers that type it)
    let keys = [
        ("b", "b", Modifiers::NONE),
        ("t", "t", Modifiers::NONE),
        ("d", "d", Modifiers::NONE),
        ("m", "m", Modifiers::NONE),
        ("k", "k", Modifiers::NONE),
        ("1", "1", Modifiers::NONE),
        ("/", "?", Modifiers::SHIFT),
        ("a", "A", Modifiers::SHIFT),
    ];
    for (base, typed_text, plain) in keys {
        // One app per key: a control holds at most one key binding.
        let mut app = app();
        map_key(&mut app, typed_text, ControlRef::TrackSolo(TrackId(1)));
        assert!(
            captures(&app, &key(base), &key(typed_text), plain, true),
            "{typed_text}"
        );
        for chord in [
            Modifiers::COMMAND,
            Modifiers::ALT,
            Modifiers::CTRL,
            Modifiers::LOGO,
            Modifiers::COMMAND | Modifiers::ALT,
            Modifiers::COMMAND | Modifiers::SHIFT,
            Modifiers::CTRL | Modifiers::ALT,
        ] {
            assert!(
                !captures(&app, &key(base), &key(typed_text), chord, true),
                "{typed_text} {chord:?}"
            );
            assert_eq!(typed(&key(base), &key(typed_text), chord), None);
        }
        // Neither an unmapped character nor a named key is captured.
        assert!(!captures(&app, &key("z"), &key("z"), Modifiers::NONE, true));
        let space = Key::Named(Named::Space);
        assert!(!captures(&app, &space, &space, Modifiers::NONE, true));
    }
}

#[test]
fn a_mapping_for_a_capital_does_not_fire_for_the_lowercase_and_the_reverse() {
    let mut app = app();
    map_key(&mut app, "A", ControlRef::TrackSolo(TrackId(1)));
    map_key(&mut app, "!", ControlRef::TrackActivator(TrackId(1)));
    send(&mut app, "a", true, false);
    send(&mut app, "a", false, false);
    send_typed(&mut app, "1", "1", Modifiers::NONE, true, false);
    send_typed(&mut app, "1", "1", Modifiers::NONE, false, false);
    assert!(!app.project().tracks[0].mixer.solo);
    assert!(app.project().tracks[0].mixer.active);
    send(&mut app, "A", true, false);
    send_typed(&mut app, "1", "!", Modifiers::SHIFT, true, false);
    assert!(app.project().tracks[0].mixer.solo);
    assert!(!app.project().tracks[0].mixer.active);
}

#[test]
fn a_release_finds_its_press_even_after_shift_or_a_chord_changed() {
    let mut app = app();
    map_key(&mut app, "A", ControlRef::TrackSolo(TrackId(1)));
    send(&mut app, "A", true, false);
    assert_eq!(app.remote.held_keys.len(), 1);
    // Shift goes up first: the release arrives as the lowercase key, with a chord held.
    assert!(captures(
        &app,
        &key("a"),
        &key("a"),
        Modifiers::COMMAND,
        false
    ));
    send_typed(&mut app, "a", "a", Modifiers::COMMAND, false, false);
    assert!(app.remote.held_keys.is_empty());
    assert!(!captures(
        &app,
        &key("a"),
        &key("a"),
        Modifiers::NONE,
        false
    ));
}

#[test]
fn learning_deduplicates_ignores_repeat_and_leaves_modes_available() {
    let mut app = app();
    bind(&mut app, "a", volume());
    act(&mut app, Action::ToggleKey);
    act(&mut app, Action::Pick(volume()));
    send(&mut app, "a", true, true);
    assert!(app.map_mode.selected_control.is_some());
    send(&mut app, "a", true, false);
    assert_eq!(app.project().mappings.len(), 1);
    assert!(app.map_mode.selected_control.is_none());
    act(&mut app, Action::Pick(slot()));
    assert!(!captures(
        &app,
        &key("k"),
        &key("k"),
        Modifiers::COMMAND,
        true
    ));
    assert!(captures(&app, &key("k"), &key("k"), Modifiers::NONE, true));
    let _ = crate::update::update(
        &mut app,
        Message::KeyPressed {
            key: key("k"),
            modifiers: Modifiers::COMMAND,
            repeat: false,
            ignored: true,
        },
    );
    assert!(!app.map_mode.on);
}

/// Deliver a native press through the real window widget, then its subscription,
/// in iced's order. This catches a shortcut firing in addition to the mapping.
fn window_press(app: &mut App, key: Key, modified_key: Key, modifiers: Modifiers) {
    use iced::advanced::{Layout, Shell, layout, widget::Tree};
    let event = iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
        modified_key,
        physical_key: iced::keyboard::key::Physical::Unidentified(
            iced::keyboard::key::NativeCode::Unidentified,
        ),
        key,
        location: iced::keyboard::Location::Standard,
        modifiers,
        repeat: false,
        text: None,
    });
    let mut messages = Vec::new();
    let status = {
        let mut window = crate::view::view(app);
        let renderer = crate::grid::test_renderer();
        let mut tree = Tree::new(&window);
        let size = iced::Size::new(1024.0, 768.0);
        let node = window.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(iced::Size::ZERO, size),
        );
        let mut shell = Shell::new(&mut messages);
        window.as_widget_mut().update(
            &mut tree,
            &event,
            Layout::new(&node),
            iced::mouse::Cursor::Unavailable,
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut shell,
            &iced::Rectangle::with_size(size),
        );
        if shell.is_event_captured() {
            iced::event::Status::Captured
        } else {
            iced::event::Status::Ignored
        }
    };
    if let Some(message) = crate::window_event(event, status) {
        messages.push(message);
    }
    for message in messages {
        let _ = crate::update::update(app, message);
    }
}

fn solo_mapped_app(text: &str) -> App {
    let mut app = app();
    map_key(&mut app, text, ControlRef::TrackSolo(TrackId(1)));
    app.focus = crate::focus::Focus::Grid;
    app.selection.clip = Some(crate::grid::Selection {
        track: TrackId(1),
        slot: 0,
    });
    app
}

#[test]
fn real_window_mapped_characters_preempt_plain_key_shortcuts() {
    // (base key, typed character, modifiers): B draws, ? opens Info, Shift-letters and digits type.
    for (base, typed_text, modifiers) in [
        ("b", "b", Modifiers::NONE),
        ("/", "?", Modifiers::SHIFT),
        ("t", "T", Modifiers::SHIFT),
        ("1", "1", Modifiers::NONE),
        ("1", "!", Modifiers::SHIFT),
    ] {
        let mut app = solo_mapped_app(typed_text);
        let info_open = app.chrome.info_open;
        window_press(&mut app, key(base), key(typed_text), modifiers);
        assert!(app.project().tracks[0].mixer.solo, "{typed_text}");
        assert!(!app.map_mode.on, "{typed_text}");
        assert_eq!(app.chrome.info_open, info_open, "{typed_text}");
        assert!(app.clip_actions.renaming.is_none(), "{typed_text}");
        assert!(!app.clip_view.editor.draw.on, "{typed_text}");
    }
}

#[test]
fn real_window_chords_pass_through_to_shortcuts_even_when_their_character_is_mapped() {
    let chord = Modifiers::COMMAND;
    for (base, modifiers) in [
        ("t", chord),
        ("d", chord),
        ("r", chord),
        ("m", chord),
        ("k", chord),
        ("b", chord | Modifiers::ALT),
        ("t", chord | Modifiers::ALT),
    ] {
        let mut app = solo_mapped_app(base);
        let tracks = app.project().tracks.len();
        let outputs = app.project().outputs.len();
        let browser_open = app.chrome.browser_open;
        window_press(&mut app, key(base), key(base), modifiers);
        assert!(!app.project().tracks[0].mixer.solo, "{base} {modifiers:?}");
        match (base, modifiers.alt()) {
            ("t", false) => assert_eq!(app.project().tracks.len(), tracks + 1, "new track"),
            ("t", true) => assert_eq!(app.project().outputs.len(), outputs + 1, "insert output"),
            ("d", _) => assert!(app.project().tracks[0].slots[1].clip.is_some(), "duplicate"),
            ("m", _) => assert!(app.map_mode.on, "MIDI map"),
            ("k", _) => assert!(app.map_mode.on, "key map"),
            ("b", _) => assert_ne!(app.chrome.browser_open, browser_open, "browser"),
            _ => {}
        }
    }
}

#[test]
fn switching_modes_clears_the_hidden_selection_before_delete() {
    let mut app = app();
    bind(&mut app, "a", volume());
    act(&mut app, Action::ToggleKey);
    let id = app.project().mappings[0].id;
    act(&mut app, Action::SelectRow(id));
    act(&mut app, Action::Toggle);
    assert_eq!(app.map_mode.selected_row, None);
    let delete = crate::map_mode::keyboard(&app, &Key::Named(Named::Delete), Modifiers::NONE, true)
        .expect("Map Mode takes Delete");
    act(&mut app, delete);
    assert_eq!(
        app.project().mappings[0].id,
        id,
        "nothing selected, nothing removed"
    );
}

#[test]
fn learning_takes_typed_characters_only_and_leaves_chords_and_named_keys_alone() {
    let mut app = app();
    act(&mut app, Action::ToggleKey);
    act(&mut app, Action::Pick(volume()));
    let escape = Key::Named(Named::Escape);
    let space = Key::Named(Named::Space);
    for chord in [Modifiers::COMMAND, Modifiers::ALT, Modifiers::CTRL] {
        assert!(
            !captures(&app, &key("s"), &key("s"), chord, true),
            "{chord:?}"
        );
    }
    assert!(!captures(&app, &escape, &escape, Modifiers::NONE, true));
    assert!(!captures(&app, &space, &space, Modifiers::NONE, true));
    // Chords and named keys fall through: nothing is learned, the control stays picked.
    send_typed(&mut app, "s", "s", Modifiers::COMMAND, true, false);
    assert!(app.project().mappings.is_empty());
    assert!(app.map_mode.selected_control.is_some());
    // ⌘⌥T still reaches the shortcut handler with a mapping for `t` in place.
    map_key(&mut app, "t", ControlRef::TrackSolo(TrackId(1)));
    let outputs = app.project().outputs.len();
    let _ = crate::update::update(
        &mut app,
        Message::KeyPressed {
            key: key("t"),
            modifiers: Modifiers::COMMAND | Modifiers::ALT,
            repeat: false,
            ignored: true,
        },
    );
    assert_eq!(app.project().outputs.len(), outputs + 1);
    // A typed capital learns as itself.
    send(&mut app, "S", true, false);
    assert_eq!(
        app.project().mappings.last().unwrap().source,
        Source::Key(KeySource {
            key: KeyName("S".into())
        })
    );
}

#[test]
fn real_window_delete_clears_the_picked_control_instead_of_the_selected_clip() {
    for (named, mode) in [
        (Named::Delete, Action::Toggle),
        (Named::Backspace, Action::ToggleKey),
    ] {
        let mut app = solo_mapped_app("a");
        act(&mut app, mode.clone());
        act(&mut app, Action::Pick(ControlRef::TrackSolo(TrackId(1))));
        if matches!(mode, Action::Toggle) {
            crate::map_mode::tests::send(&mut app, &[0xb0, 21, 127]);
            act(&mut app, Action::Pick(ControlRef::TrackSolo(TrackId(1))));
        }
        let rows = app.project().mappings.len();
        window_press(
            &mut app,
            Key::Named(named),
            Key::Named(named),
            Modifiers::NONE,
        );
        assert_eq!(app.project().mappings.len(), rows - 1, "{named:?}");
        assert!(app.project().tracks[0].slots[0].clip.is_some(), "{named:?}");
    }
}
