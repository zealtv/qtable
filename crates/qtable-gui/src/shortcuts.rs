//! The window-wide keyboard shortcuts, on one path with Cmd-Z and Cmd-Shift-Z (undo design,
//! section 9 holds the focus table all of them follow):
//!
//! | Shortcut | Behaviour |
//! | --- | --- |
//! | Space | starts or stops the transport; a text field that has the keyboard types the space, and an open pop-up menu swallows it |
//! | Cmd-S | commits unsaved typing, then Save, and the field releases the keyboard; a rejected value shows its error, nothing saves, the field keeps the keyboard, and the draft stays so the next save refuses too until Escape drops it |
//! | Cmd-Shift-S | the same, then Save As |
//! | Cmd-, | commits unsaved typing as a click elsewhere does, then opens Settings and releases the field; a rejected value reverts to the set's value, as Escape does |
//! | Cmd-Z, Cmd-Shift-Z | undo and redo (`undo.rs`): they drop the typing instead |
//!
//! Cmd is Ctrl off the Mac (`Modifiers::COMMAND`). `chord` recognises the Cmd chords and runs
//! wherever the focus is: a `TextField` never types a Cmd key, so it reaches here. `transport_key`
//! recognises Space, which only counts once nothing else (a text field, an open route list) has
//! taken it; a chord that goes ahead releases the field so a following Space plays. Each
//! shortcut sends the same message the matching button sends, so a button and its key cannot
//! drift apart.
use crate::app::App;
use crate::messages::{FileMessage, Message};
use iced::Task;
use iced::keyboard::{Key, Modifiers, key::Named};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shortcut {
    /// Cmd-Z or Cmd-Shift-Z.
    History(crate::undo::Step),
    Save,
    SaveAs,
    /// Cmd-, (Ctrl-, off the Mac).
    Settings,
}

/// The Cmd chords, whatever has the focus. Space is `transport_key`.
pub(crate) fn chord(key: &Key, modifiers: Modifiers) -> Option<Shortcut> {
    if let Some(step) = crate::undo::key(key, modifiers) {
        return Some(Shortcut::History(step));
    }
    match key.as_ref() {
        Key::Character("s" | "S") if modifiers == Modifiers::COMMAND => Some(Shortcut::Save),
        Key::Character("s" | "S") if modifiers == Modifiers::COMMAND | Modifiers::SHIFT => {
            Some(Shortcut::SaveAs)
        }
        Key::Character(",") if modifiers == Modifiers::COMMAND => Some(Shortcut::Settings),
        _ => None,
    }
}

/// A bare Space nothing else took: a field that has the keyboard captures it before this
/// runs (`ignored` is false), and so does an open route list, which handles it earlier in the
/// chain. A draft still waiting for its commit keeps the key too, as Space typed into it would,
/// and an open menu or chooser list holds the whole keyboard (`menu_keys.rs`).
pub(crate) fn transport_key(
    app: &App,
    key: &Key,
    modifiers: Modifiers,
    repeat: bool,
    ignored: bool,
) -> bool {
    ignored
        && !repeat
        && modifiers == Modifiers::NONE
        && matches!(key.as_ref(), Key::Named(Named::Space) | Key::Character(" "))
        && !crate::typing::unsaved(app)
        && !crate::menu_keys::is_open(app)
}

/// Starts the transport when it stops, stops it when it runs: the Play and Stop buttons' messages.
pub(crate) fn toggle_transport(app: &mut App) -> Task<Message> {
    let message = if app.snapshot.playing {
        Message::Stop
    } else {
        Message::Play
    };
    crate::update::update(app, message)
}

pub(crate) fn update(app: &mut App, shortcut: Shortcut) -> Task<Message> {
    match shortcut {
        Shortcut::History(step) => crate::undo::update(app, step),
        Shortcut::Save => save(app, FileMessage::Save),
        Shortcut::SaveAs => save(app, FileMessage::SaveAs),
        Shortcut::Settings => settings(app),
    }
}

/// Lets go of the keyboard a text field holds, so the next Space reaches the transport (tour,
/// 2026-11-15: "we want the field to release after Command S or command comma"). Enter in a field
/// releases it too, once the value is accepted (2026-11-15, `Message::FieldEntered`).
pub(crate) fn release_field() -> Task<Message> {
    iced::advanced::widget::operate(iced::advanced::widget::operation::focusable::unfocus())
}

/// Save and Save As write the file, so they never write over a value the engine rejected. A
/// rejected value leaves the field focused and its error showing, so it can be put right. After a
/// save that went ahead, the field releases the keyboard.
fn save(app: &mut App, message: FileMessage) -> Task<Message> {
    if !commit_typing(app) {
        return Task::none();
    }
    let saved = crate::update::update(app, Message::Files(message));
    Task::batch([saved, release_field()])
}

/// Cmd-, commits typing as a click elsewhere would. A rejected value is not kept: the field
/// reverts to what the set holds (2026-11-15, awaiting Live-style input filtering: "I would
/// expect the field to revert and the settings panel to open"), as Escape reverts it, which also
/// clears its error from the status bar. Either way Settings opens and the field releases the
/// keyboard.
fn settings(app: &mut App) -> Task<Message> {
    if !commit_typing(app) {
        let _ = crate::update::update(app, Message::CancelField);
    }
    Task::batch([open_settings(app), release_field()])
}

/// The Settings button's message, but it never closes the pane: the shortcut opens.
fn open_settings(app: &mut App) -> Task<Message> {
    if matches!(app.overlay, Some(crate::overlay::Overlay::Settings(_))) {
        return Task::none();
    }
    crate::update::update(app, Message::Settings(crate::settings_pane::Action::Toggle))
}

/// Commits what a click elsewhere would commit (`typing::commit_all`): the pending field, a
/// rename in the grid or the Detail View and a value typed on a Target face. Returns false if
/// the engine rejected one. A rejected field is put back as the pending draft, as Enter does,
/// so the field still shows what the next save would commit and the next save refuses again
/// until Escape drops it. (Renames close their editor on rejection and keep no draft.)
fn commit_typing(app: &mut App) -> bool {
    let outcome = crate::typing::commit_all(app);
    if let Some(key) = outcome.rejected_field() {
        app.pending_field = Some(key);
    }
    outcome.all_committed()
}

#[cfg(test)]
mod tests;
