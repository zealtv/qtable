//! The grid shortcuts (2026-10-11): clipboard, rename and structure keys belong to the grid.
//! ⌘R renames whatever is selected there, not only a clip. ⌘T, ⌘I, ⇧⌘M, ⌘G, ⇧⌘G and ⌘E
//! follow Live's Create menu and replace the Detail View's old button row.
use crate::control_focus::View;
use crate::messages::{EditMessage, OutputMessage};
use crate::mixer_band::MixerMessage;
use crate::{app::App, clip_actions, focus::Focus, messages::Message};
use iced::Task;
use iced::keyboard::{Key, Modifiers, key::Named};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shortcut {
    Copy,
    Cut,
    Paste,
    Duplicate,
    Rename,
    Delete,
    NewTrack,
    InsertScene,
    InsertClip,
    GroupTracks,
    Ungroup,
    ToggleStop,
}

pub(crate) fn key(
    key: &Key,
    modifiers: Modifiers,
    repeat: bool,
    ignored: bool,
) -> Option<Shortcut> {
    if repeat || !ignored {
        return None;
    }
    match (key.as_ref(), modifiers) {
        (Key::Character("c" | "C"), Modifiers::COMMAND) => Some(Shortcut::Copy),
        (Key::Character("x" | "X"), Modifiers::COMMAND) => Some(Shortcut::Cut),
        (Key::Character("v" | "V"), Modifiers::COMMAND) => Some(Shortcut::Paste),
        (Key::Character("d" | "D"), Modifiers::COMMAND) => Some(Shortcut::Duplicate),
        (Key::Character("r" | "R"), Modifiers::COMMAND) => Some(Shortcut::Rename),
        (Key::Character("t" | "T"), Modifiers::COMMAND) => Some(Shortcut::NewTrack),
        (Key::Character("i" | "I"), Modifiers::COMMAND) => Some(Shortcut::InsertScene),
        (Key::Character("m" | "M"), m) if m == Modifiers::COMMAND | Modifiers::SHIFT => {
            Some(Shortcut::InsertClip)
        }
        (Key::Character("g" | "G"), Modifiers::COMMAND) => Some(Shortcut::GroupTracks),
        (Key::Character("g" | "G"), m) if m == Modifiers::COMMAND | Modifiers::SHIFT => {
            Some(Shortcut::Ungroup)
        }
        (Key::Character("e" | "E"), Modifiers::COMMAND) => Some(Shortcut::ToggleStop),
        (Key::Named(Named::Delete | Named::Backspace), Modifiers::NONE) => Some(Shortcut::Delete),
        _ => None,
    }
}

pub(crate) fn update(app: &mut App, shortcut: Shortcut) -> Task<Message> {
    if app.pending_field.is_some()
        || app.clip_actions.renaming.is_some()
        || app.header_actions.renaming.is_some()
        || app.device_chain.renaming.is_some()
    {
        return Task::none();
    }
    // A latched fader focus takes Delete as Live's reset to default.
    if shortcut == Shortcut::Delete
        && let Some(track) = app.control_focus.track_action(View::Mixer, "volume")
    {
        return dispatch(app, Message::Mixer(MixerMessage::SetVolume(track, 1.0)));
    }
    if app.focus != Focus::Grid {
        if app.focus == Focus::Detail {
            if crate::device_chain::shown(app) {
                return crate::device_chain::shortcut(app, shortcut);
            }
            crate::inspector::event_editor::shortcut(app, shortcut);
        }
        return Task::none();
    }

    app.close_overlay();
    match shortcut {
        Shortcut::Copy => clip_actions::copy(app),
        Shortcut::Cut => clip_actions::cut(app),
        Shortcut::Paste => clip_actions::paste(app),
        Shortcut::Duplicate => clip_actions::duplicate(app),
        Shortcut::Delete => {
            if app.selection.clip.is_some() {
                clip_actions::delete(app);
            } else if let Some(scene) = app.selection.scene {
                return dispatch(app, Message::Edit(EditMessage::DeleteScenes(scene)));
            } else if !app.selection.cell.is_some_and(|cell| cell.active) {
                crate::header_actions::delete_selected(app);
            }
        }
        Shortcut::NewTrack => return dispatch(app, Message::Edit(EditMessage::AddTrack)),
        Shortcut::InsertScene => return dispatch(app, Message::Edit(EditMessage::AddScene)),
        Shortcut::InsertClip => {
            return dispatch(app, Message::Edit(EditMessage::CreateSelectedClip));
        }
        Shortcut::ToggleStop => {
            return dispatch(app, Message::Edit(EditMessage::ToggleSelectedStop));
        }
        Shortcut::GroupTracks => {
            return dispatch(
                app,
                Message::Outputs(OutputMessage::CreateGroupAroundSelected),
            );
        }
        Shortcut::Ungroup => {
            // Only a selected group header names a group; anything else has nothing to ungroup.
            return match crate::header_actions::selected_group(app) {
                Some(group) => dispatch(app, Message::Outputs(OutputMessage::Ungroup(group))),
                None => Task::none(),
            };
        }
        Shortcut::Rename => return crate::header_actions::begin_rename_selected(app),
    }
    crate::inspector::event_editor::reconcile(app);
    Task::none()
}

/// Runs a button-equivalent message, then hands focus back: the keys act on the grid and
/// leave it where it was, unlike a click in the Detail View.
fn dispatch(app: &mut App, message: Message) -> Task<Message> {
    let task = crate::update::update(app, message);
    app.focus = Focus::Grid;
    task
}

#[cfg(test)]
mod tests;
