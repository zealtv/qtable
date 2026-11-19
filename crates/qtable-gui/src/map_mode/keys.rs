//! Typed characters enter the same ingress as MIDI. Stored names have no modifiers.
use super::Mode;
use crate::app::App;
use iced::keyboard::{Key, Modifiers};
use qtable_engine::model::{KeyName, KeySource, Source};
use qtable_remote::{Endpoint, Input};

/// Whether a chord is reserved for qtable's shortcuts: ⌘, ⌥ or ⌃ held, as in Live.
/// Shift alone is not a chord; it types the capital or the shifted symbol.
pub(crate) fn reserved(modifiers: Modifiers) -> bool {
    modifiers.command() || modifiers.control() || modifiers.alt() || modifiers.logo()
}

/// The held-key identity of a physical key: iced's key without modifiers, case folded, so a
/// release finds its press even when Shift went up first.
fn base(key: &Key) -> Option<String> {
    match key.as_ref() {
        Key::Character(value) => Some(value.to_lowercase()),
        _ => None,
    }
}

/// The character this press types, which is the key source: `a` and `A` differ, as do `1` and
/// `!`. Named keys (Space, arrows, Return, Escape, F-keys), modifier keys and keys held with
/// ⌘, ⌥ or ⌃ have none and never map. `modified_key` is iced's logical key with Shift applied.
pub(crate) fn typed(key: &Key, modified_key: &Key, modifiers: Modifiers) -> Option<KeyName> {
    if reserved(modifiers) || base(key).is_none() {
        return None;
    }
    match modified_key.as_ref() {
        Key::Character(value) => KeyName::from_typed(value),
        _ => None,
    }
}

pub(crate) fn captures(
    app: &App,
    key: &Key,
    modified_key: &Key,
    modifiers: Modifiers,
    pressed: bool,
) -> bool {
    if !pressed {
        // A release always follows its press, whatever is held or focused by then.
        return base(key).is_some_and(|base| app.remote.held_keys.contains_key(&base));
    }
    let Some(name) = typed(key, modified_key, modifiers) else {
        return false;
    };
    if app.map_mode.on {
        return app.map_mode.mode == Mode::Key && app.map_mode.selected_control.is_some();
    }
    let source = Source::Key(KeySource { key: name });
    app.project()
        .mappings
        .iter()
        .any(|row| row.source == source)
}

pub(crate) fn input(
    app: &mut App,
    key: &Key,
    modified_key: &Key,
    modifiers: Modifiers,
    pressed: bool,
    repeat: bool,
) {
    if !pressed {
        release(app, key);
        return;
    }
    let (Some(base), Some(name)) = (base(key), typed(key, modified_key, modifiers)) else {
        return;
    };
    // Learning also ignores auto-repeat and duplicate press events. A learned key
    // stays held until release, including when Map Mode is exited in between.
    if repeat || app.remote.held_keys.contains_key(&base) {
        return;
    }
    app.remote.held_keys.insert(base, name.0.clone());
    push(app, name.0, true);
}

fn push(app: &mut App, key: String, pressed: bool) {
    app.remote.ingress.push(
        Endpoint::Keyboard,
        Input::Key {
            key,
            pressed,
            repeat: false,
        },
    );
    // Drain now, before any shortcut can mutate the project. This is the same
    // owner-side command execution as the independent hardware wake.
    let _ = crate::update::update(
        app,
        crate::messages::Message::Remote(crate::remote::Action::Wake),
    );
}

pub(crate) fn release(app: &mut App, key: &Key) {
    if let Some(name) = base(key).and_then(|base| app.remote.held_keys.remove(&base)) {
        push(app, name, false);
    }
}

pub(crate) fn release_all(app: &mut App) {
    let held = std::mem::take(&mut app.remote.held_keys);
    for name in held.into_values() {
        push(app, name, false);
    }
}

#[cfg(test)]
mod tests;
